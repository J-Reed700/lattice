//! The `SettingsSideEffectsPort` the Container wires: invalidating the
//! loaded LLMs a change makes stale, the custom-tool refresh, and the vault's
//! backfill and pre-flight. It reaches into several features, so it is
//! composed here rather than inside settings.

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::features::function_calling::FunctionExecutorTrait;
use crate::features::llm::instances::LlmInstances;
use crate::features::vault::writeback::VaultWriterHandle;
use lattice_core::application::ports::llm_port::InvalidationCause;
use lattice_core::application::ports::{SettingsRepositoryPort, SettingsSideEffectsPort};
use lattice_core::shared::error::Result;

/// Container-bound implementation of `SettingsSideEffectsPort`, so a
/// settings update, reset or import through any code path (Tauri command,
/// internal helper, test) invalidates the loaded LLMs it makes stale and
/// refreshes the custom-tool runtime configuration.
///
/// Each field is the same `Arc` that lives on Container, so this impl
/// observes the same state the rest of Container does.
pub(crate) struct ContainerSettingsSideEffects {
    /// The loaded LLMs. Same `Arc` as `Container::llm_instances`.
    pub(crate) llm_instances: Arc<LlmInstances>,

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
        hints: lattice_core::application::ports::settings_side_effects_port::SettingsTransitionHints,
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

        // Only LLM settings (or a global / no-category mutation, which could
        // touch anything) reach a loaded model; a search-config change must
        // not cost the next chat turn a reload.
        let touched_llm = matches!(category, Some(SettingsCategory::Llm) | None);

        if touched_llm {
            // Re-read what was written; compare it with what was there, so
            // only the models whose client or server it changes are dropped.
            // Best-effort: log but don't fail.
            match self.settings_repository.get_all().await {
                Ok(settings) => {
                    if let Some(previous) = &hints.previous_llm {
                        if previous.learning_model != settings.llm.learning_model {
                            tracing::info!(
                                from = ?previous.learning_model.as_ref().map(ToString::to_string),
                                to = ?settings.llm.learning_model.as_ref().map(ToString::to_string),
                                "settings.learning_model_set"
                            );
                        }
                    }
                    self.llm_instances
                        .invalidate(&InvalidationCause::SettingsChanged {
                            previous: hints.previous_llm.map(Box::new),
                            current: Some(Box::new(settings.llm.clone())),
                        });
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
                    // Unread, the change cannot be compared: every loaded
                    // model may be stale.
                    self.llm_instances
                        .invalidate(&InvalidationCause::SettingsChanged {
                            previous: hints.previous_llm.map(Box::new),
                            current: None,
                        });
                    tracing::warn!(
                        "Failed to refresh custom tools after settings update; \
                         keeping previous configuration: {error}"
                    );
                }
            }
        }
    }
}
