//! Model Plugin Commands - PROPERLY MIGRATED
//!
//! Direct delegation to business logic in model_management_commands.rs
//! NO GATEWAY WRAPPER - calls *_impl functions directly like search plugin does

use crate::interfaces::di::Container;
use lattice_core::shared::error::AppError;
use lattice_core::shared::ipc::{ApiError, ErrorCode};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::features::model_management::commands_extra::{
    clear_active_chat_model_impl, clear_active_embedding_model_impl,
    clear_active_utility_model_impl, delete_downloaded_model_and_file_impl,
    get_active_chat_model_impl, get_active_embedding_model_impl, get_models_with_metadata_impl,
    is_model_already_downloaded_impl, set_active_chat_model_impl, set_active_embedding_model_impl,
    set_active_utility_model_impl, warm_up_active_chat_model_impl,
    warm_up_active_utility_model_impl,
};
use crate::interfaces::commands::model_setup::{
    check_first_run_status_impl, download_default_embedding_model_impl, download_model_impl,
};

use lattice_core::domain::download::DownloadOperationState;

pub use crate::features::model_management::commands_extra::DownloadedModelResponse;

/// Download a model by ID
///
/// Thin wrapper over `download_model_impl`, the shared model-setup command.
/// Maps the response from DownloadModelResponseDto to DownloadModelResponse.
#[tauri::command]
#[specta::specta]
pub async fn download_model(
    model_id: String,
    container: State<'_, Container>,
) -> Result<DownloadModelResponse, ApiError> {
    match download_model_impl(model_id.clone(), &container).await {
        Ok(response) => {
            // Map DownloadModelResponseDto to DownloadModelResponse
            // The model id is also the stable download id.
            let status = match response.state {
                DownloadOperationState::DownloadStarted { .. } => "started",
                DownloadOperationState::AlreadyDownloaded { .. } => "already_downloaded",
                DownloadOperationState::NetworkError { .. } => "network_error",
                DownloadOperationState::OperationFailed { .. } => "failed",
            };

            Ok(DownloadModelResponse {
                download_id: model_id,
                status: status.to_string(),
            })
        }
        Err(e) => {
            // Map AppError to ApiError with appropriate error codes
            let (code, message) = match e {
                AppError::InvalidInput(msg) => (ErrorCode::InvalidInput, msg),
                AppError::NotFound(msg) => (ErrorCode::NotFound, msg),
                AppError::RateLimitExceeded(msg) => (ErrorCode::RateLimitExceeded, msg),
                AppError::Network(msg) => (ErrorCode::NetworkError, msg),
                _ => (ErrorCode::InternalError, e.to_string()),
            };

            Err(ApiError {
                code,
                message,
                details: None,
            })
        }
    }
}

/// Check whether first-run model setup is required.
///
/// Returns a JSON string for compatibility with existing first-run flow.
#[tauri::command]
#[specta::specta]
pub async fn check_first_run_status(container: State<'_, Container>) -> Result<String, ApiError> {
    check_first_run_status_impl(container.inner())
        .await
        .map_err(|e| ApiError {
            code: ErrorCode::InternalError,
            message: e,
            details: None,
        })
}

/// Start downloading the default embedding model used during first run.
///
/// Returns a JSON string for compatibility with existing first-run flow.
#[tauri::command]
#[specta::specta]
pub async fn download_default_embedding_model(
    container: State<'_, Container>,
) -> Result<String, ApiError> {
    download_default_embedding_model_impl(container.inner())
        .await
        .map_err(|e| ApiError {
            code: ErrorCode::InternalError,
            message: e,
            details: None,
        })
}

/// Delete a downloaded model
#[tauri::command]
#[specta::specta]
pub async fn delete_model(
    model_id: String,
    delete_file: bool,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    delete_downloaded_model_and_file_impl(container.inner(), &model_id, delete_file)
        .await
        .map_err(ApiError::from)
}

/// List all downloaded models
#[tauri::command]
#[specta::specta]
pub async fn list_downloaded_models(
    container: State<'_, Container>,
) -> Result<Vec<DownloadedModelResponse>, ApiError> {
    let json_str = get_models_with_metadata_impl(container.inner())
        .await
        .map_err(ApiError::from)?;

    serde_json::from_str(&json_str).map_err(|e| ApiError {
        code: ErrorCode::SerializationError,
        message: format!("Failed to parse models: {}", e),
        details: None,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn is_model_already_downloaded(
    model_id: String,
    container: State<'_, Container>,
) -> Result<bool, ApiError> {
    is_model_already_downloaded_impl(container.inner(), &model_id)
        .await
        .map_err(ApiError::from)
}

/// Set active embedding model
#[tauri::command]
#[specta::specta]
pub async fn set_active_embedding_model(
    model_id: String,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    set_active_embedding_model_impl(container.inner(), &model_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn set_active_chat_model(
    model_id: String,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    set_active_chat_model_impl(container.inner(), &model_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_active_chat_model(
    container: State<'_, Container>,
) -> Result<Option<DownloadedModelResponse>, ApiError> {
    let json_str = get_active_chat_model_impl(container.inner())
        .await
        .map_err(ApiError::from)?;

    serde_json::from_str(&json_str).map_err(|e| ApiError {
        code: ErrorCode::SerializationError,
        message: format!("Failed to parse active chat model: {}", e),
        details: None,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn get_active_embedding_model(
    container: State<'_, Container>,
) -> Result<Option<DownloadedModelResponse>, ApiError> {
    let json_str = get_active_embedding_model_impl(container.inner())
        .await
        .map_err(ApiError::from)?;

    serde_json::from_str(&json_str).map_err(|e| ApiError {
        code: ErrorCode::SerializationError,
        message: format!("Failed to parse active embedding model: {}", e),
        details: None,
    })
}

/// Get active models (both chat and embedding)
#[tauri::command]
#[specta::specta]
pub async fn get_active_models(container: State<'_, Container>) -> Result<ActiveModels, ApiError> {
    let chat_json = get_active_chat_model_impl(container.inner()).await.ok();
    let embedding_json = get_active_embedding_model_impl(container.inner())
        .await
        .ok();

    let chat_model = chat_json.and_then(|json| serde_json::from_str(&json).ok());

    let embedding_model = embedding_json.and_then(|json| serde_json::from_str(&json).ok());

    Ok(ActiveModels {
        chat_model,
        embedding_model,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn clear_active_chat_model(container: State<'_, Container>) -> Result<(), ApiError> {
    clear_active_chat_model_impl(container.inner())
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn clear_active_embedding_model(container: State<'_, Container>) -> Result<(), ApiError> {
    clear_active_embedding_model_impl(container.inner())
        .await
        .map_err(ApiError::from)
}

/// Set the active utility model (HyDE / router / intent classification).
#[tauri::command]
#[specta::specta]
pub async fn set_active_utility_model(
    model_id: String,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    set_active_utility_model_impl(container.inner(), &model_id)
        .await
        .map_err(ApiError::from)
}

/// Clear the active utility model — reverts HyDE/router back to the chat model.
#[tauri::command]
#[specta::specta]
pub async fn clear_active_utility_model(container: State<'_, Container>) -> Result<(), ApiError> {
    clear_active_utility_model_impl(container.inner())
        .await
        .map_err(ApiError::from)
}

/// Map a warm-up failure for IPC.
///
/// A model that is missing or would not load is the failure the user acts on
/// from Settings, and its sentence says what happened to their selection, so
/// it crosses as the message rather than behind "AI model not available".
fn warm_up_api_error(error: AppError) -> ApiError {
    let code = match &error {
        AppError::ModelLoadFailed(_) => ErrorCode::ModelLoadError,
        AppError::AiModelsNotInstalled(_) => ErrorCode::ModelNotLoaded,
        _ => return ApiError::from(error),
    };
    ApiError {
        code,
        message: error.to_string(),
        details: None,
    }
}

#[tauri::command]
#[specta::specta]
pub async fn warm_up_active_chat_model(container: State<'_, Container>) -> Result<(), ApiError> {
    warm_up_active_chat_model_impl(container.inner())
        .await
        .map_err(warm_up_api_error)
}

#[tauri::command]
#[specta::specta]
pub async fn warm_up_active_utility_model(container: State<'_, Container>) -> Result<(), ApiError> {
    warm_up_active_utility_model_impl(container.inner())
        .await
        .map_err(warm_up_api_error)
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct DownloadModelResponse {
    pub download_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ActiveModels {
    pub chat_model: Option<DownloadedModelResponse>,
    pub embedding_model: Option<DownloadedModelResponse>,
}

/// Absolute path of the local models folder, created on first call.
///
/// Shown in Settings › Models so the user knows where model files land.
#[tauri::command]
#[specta::specta]
pub async fn get_model_download_path<R: tauri::Runtime>(
    app_handle: tauri::AppHandle<R>,
) -> Result<String, ApiError> {
    use tauri::Manager;

    let internal = |message: String| ApiError {
        code: ErrorCode::InternalError,
        message: "Couldn't read the models folder".to_string(),
        details: Some(message),
    };

    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| internal(e.to_string()))?;
    let models_dir = app_data_dir.join("models");
    tokio::fs::create_dir_all(&models_dir)
        .await
        .map_err(|e| internal(e.to_string()))?;
    let canonical = tokio::fs::canonicalize(&models_dir)
        .await
        .map_err(|e| internal(e.to_string()))?;
    Ok(canonical.to_string_lossy().into_owned())
}
