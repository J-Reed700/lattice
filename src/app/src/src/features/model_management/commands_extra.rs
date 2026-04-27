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

use crate::features::model_management::use_cases::{
    ClearActiveChatModelUseCase, ClearActiveEmbeddingModelUseCase,
};
use crate::features::model_management::use_cases::{
    CheckIsDownloadedUseCase, DeleteDownloadedModelUseCase, GetActiveChatModelUseCase,
    GetActiveEmbeddingModelUseCase, GetDownloadedModelsWithMetadataUseCase,
    SetActiveChatModelUseCase, SetActiveEmbeddingModelUseCase, SetActiveUtilityModelUseCase,
};
use crate::domain::downloaded_model::{DownloadedModel, ModelBackend, ModelType};
use crate::infrastructure::audit::{get_audit_logger, AuditAction};
use crate::infrastructure::persistence::repositories::DownloadedModelRepository;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(skip)]
    pub metadata: Option<serde_json::Value>,
}

impl From<DownloadedModel> for DownloadedModelResponse {
    fn from(model: DownloadedModel) -> Self {
        Self {
            id: model.id().to_string(),
            model_name: model.model_name().to_string(),
            model_id: model.model_id().to_string(),
            file_path: model.file_path().to_string_lossy().to_string(),
            file_size_bytes: model.file_size_bytes(),
            model_type: model.model_type().to_db_string().to_string(),
            downloaded_at: model.downloaded_at().to_rfc3339(),
            last_used_at: model.last_used_at().map(|dt| dt.to_rfc3339()),
            use_count: model.use_count(),
            is_active_for_chat: model.is_active_for_chat(),
            is_active_for_embedding: model.is_active_for_embedding(),
            metadata: model.metadata().cloned(),
        }
    }
}

const EXTERNAL_MODEL_SOURCE: &str = "external_directory";
const MAX_EXTERNAL_MODEL_FILES: usize = 5000;
const MAX_EXTERNAL_SCAN_DEPTH: usize = 8;

#[derive(Debug, Clone)]
struct DiscoveredExternalModel {
    file_path: PathBuf,
    file_size_bytes: i64,
    source_directory: String,
}

fn normalize_external_directories(paths: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();

    for raw in paths {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }

        let candidate = PathBuf::from(trimmed);
        if !candidate.is_absolute() {
            continue;
        }

        let canonical = std::fs::canonicalize(&candidate).unwrap_or(candidate);
        let canonical_str = canonical.to_string_lossy().to_string();
        if seen.insert(canonical_str.clone()) {
            normalized.push(canonical_str);
        }
    }

    normalized
}

fn is_supported_external_model_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            let lower = ext.to_ascii_lowercase();
            lower == "gguf" || lower == "onnx"
        })
        .unwrap_or(false)
}

fn derive_external_model_id(path: &Path) -> String {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut hasher = Sha256::new();
    hasher.update(canonical.to_string_lossy().as_bytes());
    let digest = hex::encode(hasher.finalize());
    format!("external-{}", &digest[..24])
}

fn derive_external_model_name(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| stem.trim())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("External Model")
        .to_string()
}

fn derive_external_architecture(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if ext == "onnx" {
        return "onnx".to_string();
    }
    if ext == "gguf" {
        return "gguf".to_string();
    }

    "unknown".to_string()
}

fn derive_external_model_type(path: &Path) -> ModelType {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "onnx" => ModelType::TextEmbeddings,
        _ => ModelType::LanguageModel,
    }
}

fn scan_directory_recursive(
    root_dir: &Path,
    current: &Path,
    depth: usize,
    discovered: &mut Vec<DiscoveredExternalModel>,
    seen_files: &mut HashSet<String>,
) {
    if depth > MAX_EXTERNAL_SCAN_DEPTH || discovered.len() >= MAX_EXTERNAL_MODEL_FILES {
        return;
    }

    let Ok(entries) = std::fs::read_dir(current) else {
        return;
    };

    for entry in entries.flatten() {
        if discovered.len() >= MAX_EXTERNAL_MODEL_FILES {
            return;
        }

        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }

        let path = entry.path();
        if file_type.is_dir() {
            scan_directory_recursive(root_dir, &path, depth + 1, discovered, seen_files);
            continue;
        }

        if !file_type.is_file() || !is_supported_external_model_file(&path) {
            continue;
        }

        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        if metadata.len() == 0 {
            continue;
        }

        let canonical = std::fs::canonicalize(&path).unwrap_or(path);
        let canonical_str = canonical.to_string_lossy().to_string();
        if !seen_files.insert(canonical_str.clone()) {
            continue;
        }

        discovered.push(DiscoveredExternalModel {
            file_path: PathBuf::from(canonical_str),
            file_size_bytes: metadata.len() as i64,
            source_directory: root_dir.to_string_lossy().to_string(),
        });
    }
}

fn scan_external_models_sync(directories: &[String]) -> Vec<DiscoveredExternalModel> {
    let mut discovered = Vec::new();
    let mut seen_files = HashSet::new();

    for dir in directories {
        if discovered.len() >= MAX_EXTERNAL_MODEL_FILES {
            break;
        }

        let root = PathBuf::from(dir);
        if !root.is_dir() {
            continue;
        }

        scan_directory_recursive(&root, &root, 0, &mut discovered, &mut seen_files);
    }

    discovered
}

fn is_external_directory_model(model: &DownloadedModel) -> bool {
    let Some(metadata) = model.metadata() else {
        return false;
    };

    let source_matches = metadata
        .get("source")
        .and_then(|v| v.as_str())
        .map(|v| v == EXTERNAL_MODEL_SOURCE)
        .unwrap_or(false);
    let external_flag = metadata
        .get("external")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    source_matches || external_flag
}

async fn sync_external_model_directories(
    container: &Container,
    repository: &DownloadedModelRepository,
) -> Result<(), String> {
    let settings = container
        .get_settings_use_case()
        .execute()
        .await
        .map_err(|e| format!("Failed to load settings for external model sync: {}", e))?;

    let directories = normalize_external_directories(&settings.llm.external_model_directories);
    let directories_for_scan = directories.clone();
    let discovered = task::spawn_blocking(move || scan_external_models_sync(&directories_for_scan))
        .await
        .map_err(|e| format!("External model scan task failed: {}", e))?;

    let mut discovered_model_ids: HashSet<String> = HashSet::new();

    for candidate in discovered {
        let model_id = derive_external_model_id(&candidate.file_path);
        discovered_model_ids.insert(model_id.clone());

        let already_exists = repository
            .find_by_model_id(&model_id)
            .await
            .map_err(|e| format!("Failed checking external model existence: {}", e))?
            .is_some();
        if already_exists {
            continue;
        }

        let model_name = derive_external_model_name(&candidate.file_path);
        let architecture = derive_external_architecture(&candidate.file_path);
        let model_type = derive_external_model_type(&candidate.file_path);
        let metadata = serde_json::json!({
            "source": EXTERNAL_MODEL_SOURCE,
            "external": true,
            "source_directory": candidate.source_directory,
            "path": candidate.file_path.to_string_lossy().to_string(),
        });

        let external_model = DownloadedModel::from_db(
            uuid::Uuid::new_v4().to_string(),
            model_name,
            model_id.clone(),
            candidate.file_path,
            candidate.file_size_bytes,
            model_type,
            architecture,
            Utc::now(),
            None,
            0,
            false,
            false,
            Some(metadata),
            ModelBackend::Local,
            false,
        );

        if let Err(e) = repository.save(&external_model).await {
            warn!(model_id = %model_id, error = %e, "Failed to save discovered external model");
        }
    }

    let existing_models = repository
        .list_all()
        .await
        .map_err(|e| format!("Failed to list models for external sync cleanup: {}", e))?;

    for model in existing_models {
        if !is_external_directory_model(&model) {
            continue;
        }

        let still_discovered = discovered_model_ids.contains(model.model_id());
        let file_exists = model.file_path().exists() && model.file_path().is_file();
        if still_discovered && file_exists {
            continue;
        }

        match repository.delete_if_not_active(model.id()).await {
            Ok(Some(_)) => {
                info!(
                    model_id = %model.model_id(),
                    "Removed stale external model record"
                );
            }
            Ok(None) => {
                debug!(
                    model_id = %model.model_id(),
                    "External model record already absent during cleanup"
                );
            }
            Err(AppError::InvalidInput(_)) => {
                warn!(
                    model_id = %model.model_id(),
                    "Skipped stale external model cleanup because model is active"
                );
            }
            Err(e) => {
                warn!(
                    model_id = %model.model_id(),
                    error = %e,
                    "Failed to cleanup stale external model record"
                );
            }
        }
    }

    Ok(())
}

/// Get all downloaded models with metadata
///
/// Returns list of all models that have been downloaded, with full metadata.
/// Called by gateway - async dispatch.
pub async fn get_models_with_metadata_impl(container: &Container) -> Result<String, String> {
    info!("Command: get_models_with_metadata - ENTRY");

    let repository = (*container.downloaded_model_repository()).clone();
    let security_arc = Arc::clone(container.security_context());
    let rate_limiters = security_arc.rate_limiters();

    rate_limiters
        .model_management
        .check_rate_limit("list_models")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

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
                .map_err(|e| format!("Failed to serialize models: {}", e))
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

            Err(format!("Failed to get downloaded models: {}", e))
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
) -> Result<bool, String> {
    let repository = (*container.downloaded_model_repository()).clone();
    let security_arc = Arc::clone(container.security_context());
    let rate_limiters = security_arc.rate_limiters();
    let input_validator = security_arc.input_validator();

    rate_limiters
        .model_management
        .check_rate_limit("model_management")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    let validated_model_id = input_validator
        .validate_model_name(model_id)
        .map_err(|e| format!("Invalid model ID: {}", e))?;

    let use_case = CheckIsDownloadedUseCase::new(repository);
    use_case
        .execute(&validated_model_id)
        .await
        .map_err(|e| format!("Failed to check if model is downloaded: {}", e))
}

/// Set the active chat model
///
/// Sets which downloaded model should be used for chat feature.
/// Only one model can be active at a time (enforced by database trigger).
/// Called by gateway - async dispatch.
pub async fn set_active_chat_model_impl(
    container: &Container,
    model_id: &str,
) -> Result<(), String> {
    let repository = (*container.downloaded_model_repository()).clone();
    let security_arc = Arc::clone(container.security_context());
    let rate_limiters = security_arc.rate_limiters();
    let input_validator = security_arc.input_validator();

    rate_limiters
        .model_management
        .check_rate_limit("model_management")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    let validated_model_id = input_validator
        .validate_model_name(model_id)
        .map_err(|e| format!("Invalid model ID: {}", e))?;

    let logger = get_audit_logger();
    let use_case = SetActiveChatModelUseCase::new(repository);

    match use_case.execute(&validated_model_id).await {
        Ok(()) => {
            info!(model_id = %validated_model_id, "Active chat model set");
            audit_success!(
                logger,
                AuditAction::ModelSetActive,
                "active_chat_model",
                "model_id" => validated_model_id.as_str()
            )
            .await
            .ok();

            // Invalidate LLM cache
            container.invalidate_llm_cache();
            info!("LLM cache invalidated - new model will be loaded on next access");
            Ok(())
        }
        Err(e) => {
            error!(error = %e, model_id = %validated_model_id, "Failed to set active chat model");
            audit_failure!(
                logger,
                AuditAction::ModelSetActive,
                "active_chat_model",
                format!("Failed: {}", e),
                "model_id" => validated_model_id.as_str()
            )
            .await
            .ok();
            Err(format!("Failed to set active chat model: {}", e))
        }
    }
}

/// Clear the active chat model (deactivate)
///
/// Deactivates the currently active chat model.
/// Called by gateway - async dispatch.
pub async fn clear_active_chat_model_impl(container: &Container) -> Result<(), String> {
    let repository = (*container.downloaded_model_repository()).clone();
    let security_arc = Arc::clone(container.security_context());
    let rate_limiters = security_arc.rate_limiters();

    rate_limiters
        .model_management
        .check_rate_limit("model_management")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    let logger = get_audit_logger();
    let use_case = ClearActiveChatModelUseCase::new(repository);

    match use_case.execute().await {
        Ok(()) => {
            info!("Active chat model cleared");
            audit_success!(
                logger,
                AuditAction::ModelSetActive,
                "active_chat_model",
                "action" => "cleared"
            )
            .await
            .ok();

            container.invalidate_llm_cache();
            info!("LLM cache invalidated after clearing active model");
            Ok(())
        }
        Err(e) => {
            error!(error = %e, "Failed to clear active chat model");
            audit_failure!(
                logger,
                AuditAction::ModelSetActive,
                "active_chat_model",
                format!("Failed to clear: {}", e)
            )
            .await
            .ok();
            Err(format!("Failed to clear active chat model: {}", e))
        }
    }
}

/// Clear the active embedding model (deactivate)
///
/// Deactivates the currently active embedding model.
/// Called by gateway - async dispatch.
pub async fn clear_active_embedding_model_impl(container: &Container) -> Result<(), String> {
    let repository = (*container.downloaded_model_repository()).clone();
    let security_arc = Arc::clone(container.security_context());
    let rate_limiters = security_arc.rate_limiters();

    rate_limiters
        .model_management
        .check_rate_limit("model_management")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    let logger = get_audit_logger();
    let use_case = ClearActiveEmbeddingModelUseCase::new(repository);

    match use_case.execute().await {
        Ok(()) => {
            info!("Active embedding model cleared");
            audit_success!(
                logger,
                AuditAction::ModelSetActive,
                "active_embedding_model",
                "action" => "cleared"
            )
            .await
            .ok();
            Ok(())
        }
        Err(e) => {
            error!(error = %e, "Failed to clear active embedding model");
            audit_failure!(
                logger,
                AuditAction::ModelSetActive,
                "active_embedding_model",
                format!("Failed to clear: {}", e)
            )
            .await
            .ok();
            Err(format!("Failed to clear active embedding model: {}", e))
        }
    }
}

/// Get the currently active chat model
///
/// Returns the model currently set as active for chat, or None if no model is active.
/// Called by gateway - async dispatch.
pub async fn get_active_chat_model_impl(container: &Container) -> Result<String, String> {
    let repository = (*container.downloaded_model_repository()).clone();
    let security_arc = Arc::clone(container.security_context());
    let rate_limiters = security_arc.rate_limiters();

    rate_limiters
        .model_management
        .check_rate_limit("model_management")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    let use_case = GetActiveChatModelUseCase::new(repository);

    match use_case.execute().await {
        Ok(maybe_model) => {
            let response: Option<DownloadedModelResponse> = maybe_model.map(|m| m.into());
            serde_json::to_string(&response)
                .map_err(|e| format!("Failed to serialize chat model: {}", e))
        }
        Err(e) => {
            error!(error = %e, "Failed to get active chat model");
            Err(format!("Failed to get active chat model: {}", e))
        }
    }
}

/// Delete a downloaded model and optionally its file
///
/// Removes the model record from database and optionally deletes the file from filesystem.
/// Called by gateway - async dispatch.
pub async fn delete_downloaded_model_and_file_impl(
    container: &Container,
    model_id: &str,
    delete_file: bool,
) -> Result<(), String> {
    let repository = (*container.downloaded_model_repository()).clone();
    let security_arc = Arc::clone(container.security_context());
    let rate_limiters = security_arc.rate_limiters();

    rate_limiters
        .model_management
        .check_rate_limit("model_management")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    if model_id.trim().is_empty() {
        return Err("Model ID cannot be empty".to_string());
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
            Err(format!("Failed to delete downloaded model: {}", e))
        }
    }
}

/// Get the currently active embedding model
///
/// Returns the model currently set as active for embeddings, or None if no model is active.
/// Called by gateway - async dispatch.
pub async fn get_active_embedding_model_impl(container: &Container) -> Result<String, String> {
    let repository = (*container.downloaded_model_repository()).clone();
    let security_arc = Arc::clone(container.security_context());
    let rate_limiters = security_arc.rate_limiters();

    rate_limiters
        .model_management
        .check_rate_limit("model_management")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    let use_case = GetActiveEmbeddingModelUseCase::new(repository);

    match use_case.execute().await {
        Ok(maybe_model) => {
            let response: Option<DownloadedModelResponse> = maybe_model.map(|m| m.into());
            serde_json::to_string(&response)
                .map_err(|e| format!("Failed to serialize embedding model: {}", e))
        }
        Err(e) => {
            error!(error = %e, "Failed to get active embedding model");
            Err(format!("Failed to get active embedding model: {}", e))
        }
    }
}

/// Warm up the currently active chat model.
///
/// This eagerly loads the LLM into memory so first chat response is faster.
pub async fn warm_up_active_chat_model_impl(container: &Container) -> Result<(), String> {
    let repository = (*container.downloaded_model_repository()).clone();
    let security_arc = Arc::clone(container.security_context());
    let rate_limiters = security_arc.rate_limiters();

    rate_limiters
        .model_management
        .check_rate_limit("model_management")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    let use_case = GetActiveChatModelUseCase::new(repository.clone());
    let active_model = use_case
        .execute()
        .await
        .map_err(|e| format!("Failed to get active chat model: {}", e))?
        .ok_or_else(|| "No active chat model is selected. Set one first.".to_string())?;

    let start = Instant::now();
    if let Err(error) = container.get_or_load_llm().await {
        if let AppError::ModelLoadFailed(load_error) = &error {
            warn!(
                model_id = %active_model.model_id(),
                model_name = %active_model.model_name(),
                "Active chat model failed to load during warmup; clearing active selection"
            );

            let clear_use_case = ClearActiveChatModelUseCase::new(repository);
            if let Err(clear_error) = clear_use_case.execute().await {
                error!(
                    model_id = %active_model.model_id(),
                    model_name = %active_model.model_name(),
                    clear_error = %clear_error,
                    "Failed to clear invalid active chat model after warmup failure"
                );
                return Err(format!(
                    "Failed to warm up active chat model: {}. Failed to clear invalid active model: {}",
                    load_error, clear_error
                ));
            }

            container.invalidate_llm_cache();
            warn!(
                model_id = %active_model.model_id(),
                model_name = %active_model.model_name(),
                "Invalid active chat model cleared after warmup failure"
            );

            return Err(format!(
                "Failed to warm up active chat model: {}. The model was deactivated because it could not be loaded. \
                 Please download or select a compatible model in Settings → Model Catalog.",
                load_error
            ));
        }

        return Err(format!("Failed to warm up active chat model: {}", error));
    }

    let elapsed_ms = start.elapsed().as_millis() as u64;

    info!(
        model_id = %active_model.model_id(),
        model_name = %active_model.model_name(),
        elapsed_ms,
        "Active chat model warmed up"
    );

    Ok(())
}

/// Set the active utility model.
///
/// Utility = HyDE expansion / router / intent classification — a chat-style
/// generation role. Only one model can be active at a time. The use-case
/// gates the local-only download check by backend, so Ollama-backed models
/// (which have no on-disk files) can be activated without false-positive
/// "not downloaded" errors. Called by gateway - async dispatch.
pub async fn set_active_utility_model_impl(
    container: &Container,
    model_id: &str,
) -> Result<(), String> {
    let repository = (*container.downloaded_model_repository()).clone();
    let security_arc = Arc::clone(container.security_context());
    let rate_limiters = security_arc.rate_limiters();
    let input_validator = security_arc.input_validator();

    rate_limiters
        .model_management
        .check_rate_limit("model_management")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    let validated_model_id = input_validator
        .validate_model_name(model_id)
        .map_err(|e| format!("Invalid model ID: {}", e))?;

    let logger = get_audit_logger();
    let use_case = SetActiveUtilityModelUseCase::new(repository);

    match use_case.execute(&validated_model_id).await {
        Ok(()) => {
            info!(model_id = %validated_model_id, "Active utility model set");
            audit_success!(
                logger,
                AuditAction::ModelSetActive,
                "active_utility_model",
                "model_id" => validated_model_id.as_str()
            )
            .await
            .ok();

            // Utility shares the LLM resolution path with chat, so invalidate the
            // LLM cache to force re-resolution on next HyDE/router invocation.
            container.invalidate_llm_cache();
            info!("LLM cache invalidated - utility role updated");
            Ok(())
        }
        Err(e) => {
            error!(error = %e, model_id = %validated_model_id, "Failed to set active utility model");
            audit_failure!(
                logger,
                AuditAction::ModelSetActive,
                "active_utility_model",
                format!("Failed: {}", e),
                "model_id" => validated_model_id.as_str()
            )
            .await
            .ok();
            Err(format!("Failed to set active utility model: {}", e))
        }
    }
}

/// Clear the active utility model — reverts HyDE/router back to the chat model.
pub async fn clear_active_utility_model_impl(container: &Container) -> Result<(), String> {
    let repository = (*container.downloaded_model_repository()).clone();
    let security_arc = Arc::clone(container.security_context());
    let rate_limiters = security_arc.rate_limiters();

    rate_limiters
        .model_management
        .check_rate_limit("model_management")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    let logger = get_audit_logger();
    let use_case = SetActiveUtilityModelUseCase::new(repository);

    match use_case.clear().await {
        Ok(()) => {
            info!("Active utility model cleared");
            audit_success!(
                logger,
                AuditAction::ModelSetActive,
                "active_utility_model",
                "action" => "cleared"
            )
            .await
            .ok();

            container.invalidate_llm_cache();
            info!("LLM cache invalidated after clearing utility model");
            Ok(())
        }
        Err(e) => {
            error!(error = %e, "Failed to clear active utility model");
            audit_failure!(
                logger,
                AuditAction::ModelSetActive,
                "active_utility_model",
                format!("Failed to clear: {}", e)
            )
            .await
            .ok();
            Err(format!("Failed to clear active utility model: {}", e))
        }
    }
}

/// Set the active embedding model
///
/// Sets which downloaded model should be used for embeddings.
/// Only one model can be active at a time (enforced by database trigger).
/// Called by gateway - async dispatch.
pub async fn set_active_embedding_model_impl(
    container: &Container,
    model_id: &str,
) -> Result<(), String> {
    let repository = (*container.downloaded_model_repository()).clone();
    let security_arc = Arc::clone(container.security_context());
    let rate_limiters = security_arc.rate_limiters();
    let input_validator = security_arc.input_validator();

    rate_limiters
        .model_management
        .check_rate_limit("model_management")
        .await
        .map_err(|e| format!("Rate limit exceeded: {}", e))?;

    let validated_model_id = input_validator
        .validate_model_name(model_id)
        .map_err(|e| format!("Invalid model ID: {}", e))?;

    let logger = get_audit_logger();
    let use_case = SetActiveEmbeddingModelUseCase::new(repository);

    match use_case.execute(&validated_model_id).await {
        Ok(()) => {
            info!(model_id = %validated_model_id, "Active embedding model set");
            audit_success!(
                logger,
                AuditAction::ModelSetActive,
                "active_embedding_model",
                "model_id" => validated_model_id.as_str()
            )
            .await
            .ok();

            container.invalidate_embedding_cache();
            info!("Embedding cache invalidated - new model will be loaded on next access");
            Ok(())
        }
        Err(e) => {
            error!(error = %e, model_id = %validated_model_id, "Failed to set active embedding model");
            audit_failure!(
                logger,
                AuditAction::ModelSetActive,
                "active_embedding_model",
                format!("Failed: {}", e),
                "model_id" => validated_model_id.as_str()
            )
            .await
            .ok();
            Err(format!("Failed to set active embedding model: {}", e))
        }
    }
}
