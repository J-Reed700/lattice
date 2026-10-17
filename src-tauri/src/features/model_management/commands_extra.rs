//! Model Management Commands
//!
//! Commands for managing downloaded models and chat model selection.
//!
//! # Gateway Pattern
//!
//! All commands are routed through the Gateway pattern which provides async dispatch.
//! The `*_impl` functions are the async implementations called by the gateway.
//!
//! Async functions store their state on the heap (in Future objects), not the stack.
//! Deep async call chains don't cause stack overflow - that's the whole point of async.

use crate::features::download::downloaded_model_repository::DownloadedModelRepository;
use crate::features::model_management::use_cases::{
    CheckIsDownloadedUseCase, DeleteDownloadedModelUseCase, GetActiveChatModelUseCase,
    GetActiveEmbeddingModelUseCase, GetDownloadedModelsWithMetadataUseCase,
    SetActiveChatModelUseCase, SetActiveEmbeddingModelUseCase, SetActiveUtilityModelUseCase,
};
use crate::features::model_management::use_cases::{
    ClearActiveChatModelUseCase, ClearActiveEmbeddingModelUseCase,
};
use crate::interfaces::di::Container;
use chrono::Utc;
use lattice_core::application::contracts::settings::LocalModelConfigDto;
use lattice_core::application::ports::llm_port::InvalidationCause;
use lattice_core::application::ports::ModelFileInspector;
use lattice_core::domain::models::downloaded::{DownloadedModel, ModelLocation};
use lattice_core::domain::models::metadata::ModelType;
use lattice_core::domain::models::model_ref::ModelRef;
use lattice_core::shared::error::AppError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::task;
use tracing::{debug, error, info, warn};

mod active_models;
mod external_models;
mod warm_up;

pub use active_models::{
    clear_active_chat_model_impl, clear_active_embedding_model_impl,
    clear_active_utility_model_impl, get_active_chat_model_impl, get_active_embedding_model_impl,
    set_active_chat_model_impl, set_active_embedding_model_impl, set_active_utility_model_impl,
};
use external_models::sync_external_model_directories;
pub use warm_up::{
    warm_up_active_chat_model_impl, warm_up_active_utility_model_impl, warm_up_learning_model_impl,
};

/// Response for get_models_with_metadata command
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct DownloadedModelResponse {
    pub id: String,
    pub model_name: String,
    pub model_id: String,
    pub file_path: String,
    pub file_size_bytes: i64,
    pub model_type: String,
    pub downloaded_at: String,
    pub last_used_at: Option<String>,
    pub use_count: i64,
    pub is_active_for_chat: bool,
    pub is_active_for_embedding: bool,
    pub is_active_for_utility: bool,
    pub backend: String,
    #[serde(default)]
    pub trained_context_length: Option<u32>,
    /// Whether the GGUF's chat template declares tool calling.
    #[serde(default)]
    pub supports_tool_calling: Option<bool>,
    /// The context window saved for this model in Settings; `None` is Auto.
    #[serde(default)]
    pub configured_context_length: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(skip)]
    pub metadata: Option<serde_json::Value>,
}

/// A model loaded right now, for diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct LoadedModelDto {
    pub model: ModelRef,
    pub provider: String,
    pub context_tokens: u32,
    /// Turns and jobs holding it now.
    pub in_flight: u32,
    /// Who holds it: a role and the conversation or job it serves.
    pub pinned_by: Vec<String>,
    pub loaded_at: String,
    pub last_used_at: String,
    pub gpu_bytes: Option<u64>,
    /// Why it runs degraded (on CPU), when it does.
    pub degraded: Option<String>,
}

/// The last learning work refused because the Learning model's window was
/// too small, while that model has not covered the window since. Settings →
/// Models shows it beside the model that holds the Learning role.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct LearningWindowShortfallDto {
    pub model: ModelRef,
    /// The window the refused work needed, in tokens.
    pub needed_tokens: u32,
    /// The window the model had, in tokens.
    pub window_tokens: u32,
    /// Whether that window was the one saved for the model in Settings →
    /// Models, rather than what Auto or the model's server gave it.
    pub window_is_saved: bool,
}

/// What describing a downloaded model reads besides its row: its file's
/// header, and the context window saved for it in Settings.
pub struct ModelFacts {
    inspector: Arc<dyn ModelFileInspector>,
    configs: BTreeMap<String, LocalModelConfigDto>,
}

impl ModelFacts {
    /// Settings that cannot be read leave every model's window at Auto.
    pub async fn read(container: &Container) -> Self {
        let configs = match container.get_settings_use_case().execute().await {
            Ok(settings) => settings.llm.local_model_configs,
            Err(error) => {
                warn!(%error, "Settings unreadable; listing models without their saved windows");
                BTreeMap::new()
            }
        };
        Self {
            inspector: container.model_file_inspector(),
            configs,
        }
    }
}

impl DownloadedModelResponse {
    /// Maps a stored model, reading a local file's header for its trained
    /// context length and whether it can call tools.
    pub fn from_model(model: DownloadedModel, facts: &ModelFacts) -> Self {
        let file_path = model
            .loadable_path()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let is_local = model.location().is_local();
        let backend = if is_local { "local" } else { "ollama" };
        let header = (is_local && !file_path.is_empty()).then(|| Path::new(&file_path));
        let trained_context_length =
            header.and_then(|path| facts.inspector.trained_context_length(path));
        let supports_tool_calling =
            header.and_then(|path| facts.inspector.supports_tool_calling(path));
        let configured_context_length = facts
            .configs
            .get(model.model_id())
            .and_then(|config| config.context_length);
        Self {
            id: model.id().to_string(),
            model_name: model.model_name().to_string(),
            model_id: model.model_id().to_string(),
            file_path,
            file_size_bytes: model.file_size_bytes(),
            model_type: model.model_type().to_db_string().to_string(),
            downloaded_at: model.downloaded_at().to_rfc3339(),
            last_used_at: model.last_used_at().map(|dt| dt.to_rfc3339()),
            use_count: model.use_count(),
            is_active_for_chat: model.is_active_for_chat(),
            is_active_for_embedding: model.is_active_for_embedding(),
            is_active_for_utility: model.is_active_for_utility(),
            backend: backend.to_string(),
            trained_context_length,
            supports_tool_calling,
            configured_context_length,
            metadata: model.metadata().cloned(),
        }
    }
}

/// Get all downloaded models with metadata
///
/// Returns list of all models that have been downloaded, with full metadata.
/// Called by gateway - async dispatch.
pub async fn get_models_with_metadata_impl(container: &Container) -> Result<String, AppError> {
    info!("Command: get_models_with_metadata - ENTRY");

    let repository = (*container.downloaded_model_repository()).clone();

    if let Err(e) = sync_external_model_directories(container, &repository).await {
        warn!(error = %e, "External model directory sync failed");
    }

    let use_case = GetDownloadedModelsWithMetadataUseCase::new(repository);

    match use_case.execute().await {
        Ok(models) => {
            let facts = ModelFacts::read(container).await;
            let response: Vec<DownloadedModelResponse> = models
                .into_iter()
                .map(|m| DownloadedModelResponse::from_model(m, &facts))
                .collect();

            info!(count = response.len(), "Retrieved downloaded models");

            serde_json::to_string(&response)
                .map_err(|e| AppError::Serialization(format!("Failed to serialize models: {}", e)))
        }
        Err(e) => {
            error!(error = %e, "Failed to get downloaded models");

            Err(e)
        }
    }
}

/// Check if a model is already downloaded
///
/// Returns true if the model exists in the downloaded models table.
/// Called by gateway - async dispatch.
pub async fn is_model_already_downloaded_impl(
    container: &Container,
    model_id: &str,
) -> Result<bool, AppError> {
    let repository = (*container.downloaded_model_repository()).clone();
    let validated_model_id = container
        .security_context()
        .input_validator()
        .validate_model_name(model_id)?;

    let use_case = CheckIsDownloadedUseCase::new(repository);
    use_case.execute(&validated_model_id).await
}

/// Delete a downloaded model and optionally its file
///
/// Removes the model record from database and optionally deletes the file from filesystem.
/// Called by gateway - async dispatch.
pub async fn delete_downloaded_model_and_file_impl(
    container: &Container,
    model_id: &str,
    delete_file: bool,
) -> Result<(), AppError> {
    let repository = (*container.downloaded_model_repository()).clone();

    if model_id.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "Model ID cannot be empty".to_string(),
        ));
    }

    let deleted = repository.find_by_id(model_id).await?;
    let use_case = DeleteDownloadedModelUseCase::new(repository);

    match use_case.execute(model_id, delete_file).await {
        Ok(()) => {
            info!(id = %model_id, file_deleted = delete_file, "Downloaded model deleted");
            // Work running on it finishes on the open file; nothing else is
            // handed it again. No setting that names it changes: a chat or
            // the Learning role set to it says it is gone when next used.
            if let Some(deleted) = deleted {
                container.invalidate_llm_instances(InvalidationCause::ModelDeleted {
                    model_id: deleted.model_id().to_owned(),
                });
            }
            Ok(())
        }
        Err(e) => {
            error!(error = %e, id = %model_id, "Failed to delete downloaded model");
            Err(e)
        }
    }
}
