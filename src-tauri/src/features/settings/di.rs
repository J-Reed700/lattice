//! Settings feature dependency injection.

use std::sync::Arc;

use crate::application::ports::{SettingsRepositoryPort, SettingsSideEffectsPort};
use crate::features::function_calling::FunctionExecutorTrait;
use crate::features::settings::use_cases::{
    ExportSettingsUseCase, GetSettingsUseCase, ImportSettingsUseCase, ResetSettingsUseCase,
    UpdateSettingsUseCase, ValidateSettingsUseCase,
};
use crate::features::vault::writeback::VaultWriterHandle;
use crate::infrastructure::persistence::repositories::SettingsRepository;
use crate::interfaces::di::container::{ChatLlmCache, NamedLlmCache};
use crate::interfaces::di::Container;
use crate::shared::error::Result;
use sqlx::SqlitePool;
use std::path::Path;

#[derive(Clone)]
pub struct SettingsDi {
    pub settings_repo: Arc<dyn SettingsRepositoryPort>,
    pub get_settings_use_case: Arc<GetSettingsUseCase>,
    pub update_settings_use_case: Arc<UpdateSettingsUseCase>,
    pub reset_settings_use_case: Arc<ResetSettingsUseCase>,
    pub export_settings_use_case: Arc<ExportSettingsUseCase>,
    pub import_settings_use_case: Arc<ImportSettingsUseCase>,
    pub validate_settings_use_case: Arc<ValidateSettingsUseCase>,
}

pub async fn build(settings_path: &Path) -> Result<SettingsDi> {
    let settings_repo = Arc::new(SettingsRepository::new(settings_path.to_path_buf()).await?)
        as Arc<dyn SettingsRepositoryPort>;

    Ok(SettingsDi {
        get_settings_use_case: Arc::new(GetSettingsUseCase::new(settings_repo.clone())),
        update_settings_use_case: Arc::new(UpdateSettingsUseCase::new(settings_repo.clone())),
        reset_settings_use_case: Arc::new(ResetSettingsUseCase::new(settings_repo.clone())),
        export_settings_use_case: Arc::new(ExportSettingsUseCase::new(settings_repo.clone())),
        import_settings_use_case: Arc::new(ImportSettingsUseCase::new(settings_repo.clone())),
        validate_settings_use_case: Arc::new(ValidateSettingsUseCase::new(settings_repo.clone())),
        settings_repo,
    })
}

/// Settings' registrar surface on `Container`.
impl Container {
    // Settings (from SystemModule)
    pub fn get_settings_use_case(&self) -> Arc<GetSettingsUseCase> {
        Arc::clone(self.system.get_settings_use_case())
    }

    /// Settings update use case, wired with the side-effects port so
    /// LLM cache invalidation fires after a successful write.
    ///
    /// Constructs a fresh use case on each call rather than returning
    /// the pre-built `Arc` from `SystemModule`. SystemModule's stored
    /// instance is built before the side-effects port exists
    /// (bootstrap order: System → AI → side-effects), so it has only
    /// noop side effects. Production code paths should always go
    /// through Container, never directly through SystemModule.
    /// Audit P0-3 fix.
    pub fn update_settings_use_case(&self) -> Arc<UpdateSettingsUseCase> {
        Arc::new(UpdateSettingsUseCase::with_side_effects(
            self.system.settings_repo().clone(),
            self.settings_side_effects.clone(),
        ))
    }

    /// Settings reset use case, wired with the side-effects port. See
    /// `update_settings_use_case` for the bootstrap-order rationale.
    /// Audit P0-3 fix: reset previously did NOT invalidate any caches,
    /// so a "reset to defaults" left stale LLM caches until app
    /// restart.
    pub fn reset_settings_use_case(&self) -> Arc<ResetSettingsUseCase> {
        Arc::new(ResetSettingsUseCase::with_side_effects(
            self.system.settings_repo().clone(),
            self.settings_side_effects.clone(),
        ))
    }

    pub fn export_settings_use_case(&self) -> Arc<ExportSettingsUseCase> {
        Arc::clone(self.system.export_settings_use_case())
    }

    pub fn import_settings_use_case(&self) -> Arc<ImportSettingsUseCase> {
        Arc::clone(self.system.import_settings_use_case())
    }

    pub fn validate_settings_use_case(&self) -> Arc<ValidateSettingsUseCase> {
        Arc::clone(self.system.validate_settings_use_case())
    }
}

/// Container-bound implementation of `SettingsSideEffectsPort`. Holds
/// references to the same primitive caches the legacy
/// `Container::invalidate_*` methods touched, so a settings update or
/// reset triggered through any code path (Tauri command, internal
/// helper, test) invalidates the LLM cache, router LLM cache, and
/// custom-tool runtime configuration.
///
/// Each field is the same `Arc` that lives on Container — they share
/// the same underlying cache state, so this impl observes the same state
/// the rest of Container does.
pub(crate) struct ContainerSettingsSideEffects {
    /// LLM client cache. Same `Arc` as `Container::llm_cache`.
    pub(crate) llm_cache: ChatLlmCache,

    /// Router LLM cache. Same `Arc` as `Container::router_llm_cache`.
    pub(crate) router_llm_cache: NamedLlmCache,

    pub(crate) utility_llm_cache: NamedLlmCache,

    /// Function executor for refreshing custom-tool runtime config.
    pub(crate) function_executor: Arc<dyn FunctionExecutorTrait>,

    /// Settings repository for re-reading custom_tools after a write.
    pub(crate) settings_repository: Arc<dyn SettingsRepositoryPort>,

    /// Used by the vault backfill side effect.
    pub(crate) db_pool: SqlitePool,

    /// Backfill submits through this so it shares the FIFO queue with
    /// per-note writes.
    pub(crate) vault_writer: VaultWriterHandle,
}

#[async_trait::async_trait]
impl SettingsSideEffectsPort for ContainerSettingsSideEffects {
    async fn on_settings_updated(
        &self,
        category: Option<crate::features::settings::dto::SettingsCategory>,
        hints: crate::application::ports::settings_side_effects_port::SettingsTransitionHints,
    ) {
        use crate::features::settings::dto::SettingsCategory;

        // Submit through the writer queue so backfill shares the FIFO
        // with per-note writes and can't race fresh edits.
        if hints.vault_just_enabled {
            match self.settings_repository.get_all().await {
                Ok(settings) => {
                    if let Some(vault_root) = crate::features::vault::writeback::resolve_vault_root(
                        &settings.vault.vault_path,
                    ) {
                        tracing::info!(
                            vault_root = %vault_root.display(),
                            "Vault enabled — enqueueing one-shot backfill"
                        );
                        self.vault_writer.submit(
                            crate::features::vault::writeback::VaultWriteJob::Backfill {
                                pool: self.db_pool.clone(),
                                vault_root,
                            },
                        );
                    } else {
                        tracing::warn!(
                            "Vault enabled but vault root could not be resolved — skipping backfill"
                        );
                    }
                }
                Err(e) => tracing::warn!(
                    error = %e,
                    "Vault enabled but settings re-read failed — skipping backfill"
                ),
            }
        }

        // Invalidate the LLM caches when LLM settings (or a global
        // / no-category mutation, which could touch anything) changed.
        // Other categories (Search, Indexing, Display, etc.) don't
        // affect the LLM cache — skipping the invalidation here means
        // a search-config change doesn't cause a needless model
        // reload on the next chat turn.
        let touched_llm = matches!(category, Some(SettingsCategory::Llm) | None);

        if touched_llm {
            self.llm_cache.invalidate();
            self.router_llm_cache.invalidate();
            self.utility_llm_cache.invalidate();
            // Custom-tool runtime config refresh — re-read settings
            // and push the active custom_tools map into the function
            // executor. Best-effort: log but don't fail.
            match self.settings_repository.get_all().await {
                Ok(settings) => {
                    let custom_tool_map: std::collections::HashMap<
                        String,
                        crate::features::settings::dto::CustomToolSettingsDto,
                    > = settings
                        .llm
                        .custom_tools
                        .into_iter()
                        .filter(|tool| tool.enabled)
                        .map(|tool| (tool.name.clone(), tool))
                        .collect();
                    self.function_executor.set_custom_tools(custom_tool_map);
                }
                Err(error) => {
                    tracing::warn!(
                        "Failed to refresh custom tools after settings update; \
                         keeping previous configuration: {error}"
                    );
                }
            }
        }
    }
}
