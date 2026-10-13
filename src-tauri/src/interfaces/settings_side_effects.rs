//! The `SettingsSideEffectsPort` the Container wires: LLM cache
//! invalidation, the custom-tool refresh, and the vault's backfill and
//! pre-flight. It reaches into several features, so it is composed here
//! rather than inside settings.

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::application::ports::{SettingsRepositoryPort, SettingsSideEffectsPort};
use crate::features::function_calling::FunctionExecutorTrait;
use crate::features::vault::writeback::VaultWriterHandle;
use crate::interfaces::di::container::{ChatLlmCache, NamedLlmCache};
use crate::shared::error::Result;

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
    async fn check_vault_enable(&self, vault_path: &str) -> Result<()> {
        crate::features::vault::preflight::check_vault_root(vault_path).await
    }

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
                        if let Err(error) = self.vault_writer.enqueue_backfill(&self.db_pool).await
                        {
                            tracing::warn!(%error, "Could not persist vault backfill intent");
                        }
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
