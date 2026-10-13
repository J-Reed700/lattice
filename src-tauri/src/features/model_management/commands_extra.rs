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

use crate::domain::models::downloaded::{DownloadedModel, ModelLocation};
use crate::domain::models::metadata::ModelType;
use crate::features::download::downloaded_model_repository::DownloadedModelRepository;
use crate::features::model_management::use_cases::{
    CheckIsDownloadedUseCase, DeleteDownloadedModelUseCase, GetActiveChatModelUseCase,
    GetActiveEmbeddingModelUseCase, GetDownloadedModelsWithMetadataUseCase,
    SetActiveChatModelUseCase, SetActiveEmbeddingModelUseCase, SetActiveUtilityModelUseCase,
};
use crate::features::model_management::use_cases::{
    ClearActiveChatModelUseCase, ClearActiveEmbeddingModelUseCase,
};
use crate::infrastructure::audit::{get_audit_logger, AuditAction};
use crate::interfaces::di::Container;
use crate::shared::error::AppError;
use crate::{audit_failure, audit_success};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
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
pub use warm_up::{warm_up_active_chat_model_impl, warm_up_active_utility_model_impl};

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
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(skip)]
    pub metadata: Option<serde_json::Value>,
}

impl From<DownloadedModel> for DownloadedModelResponse {
    fn from(model: DownloadedModel) -> Self {
        let file_path = model
            .loadable_path()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let backend = if model.location().is_local() {
            "local"
        } else {
            "ollama"
        };
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
    let security_arc = Arc::clone(container.security_context());
    let rate_limiters = security_arc.rate_limiters();

    rate_limiters
        .model_management
        .check_rate_limit("list_models")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    if let Err(e) = sync_external_model_directories(container, &repository).await {
        warn!(error = %e, "External model directory sync failed");
    }

    let logger = get_audit_logger();
    let use_case = GetDownloadedModelsWithMetadataUseCase::new(repository);

    match use_case.execute().await {
        Ok(models) => {
            let response: Vec<DownloadedModelResponse> =
                models.into_iter().map(|m| m.into()).collect();

            info!(count = response.len(), "Retrieved downloaded models");

            audit_success!(
                logger,
                AuditAction::ModelViewed,
                "models",
                "count" => response.len().to_string().as_str()
            )
            .await
            .ok();

            serde_json::to_string(&response)
                .map_err(|e| AppError::Serialization(format!("Failed to serialize models: {}", e)))
        }
        Err(e) => {
            error!(error = %e, "Failed to get downloaded models");

            audit_failure!(
                logger,
                AuditAction::ModelViewed,
                "models",
                format!("Failed: {}", e)
            )
            .await
            .ok();

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
    let security_arc = Arc::clone(container.security_context());
    let rate_limiters = security_arc.rate_limiters();
    let input_validator = security_arc.input_validator();

    rate_limiters
        .model_management
        .check_rate_limit("model_management")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    let validated_model_id = input_validator.validate_model_name(model_id)?;

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
    let security_arc = Arc::clone(container.security_context());
    let rate_limiters = security_arc.rate_limiters();

    rate_limiters
        .model_management
        .check_rate_limit("model_management")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    if model_id.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "Model ID cannot be empty".to_string(),
        ));
    }

    let logger = get_audit_logger();
    let use_case = DeleteDownloadedModelUseCase::new(repository);

    match use_case.execute(model_id, delete_file).await {
        Ok(()) => {
            info!(id = %model_id, file_deleted = delete_file, "Downloaded model deleted");
            audit_success!(
                logger,
                AuditAction::ModelDeleted,
                "downloaded_model",
                "id" => model_id,
                "file_deleted" => delete_file.to_string().as_str()
            )
            .await
            .ok();
            Ok(())
        }
        Err(e) => {
            error!(error = %e, id = %model_id, "Failed to delete downloaded model");
            audit_failure!(
                logger,
                AuditAction::ModelDeleted,
                "downloaded_model",
                format!("Failed: {}", e),
                "id" => model_id
            )
            .await
            .ok();
            Err(e)
        }
    }
}
