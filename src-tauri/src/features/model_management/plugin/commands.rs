//! Model Plugin Commands - PROPERLY MIGRATED
//!
//! Direct delegation to business logic in model_management_commands.rs
//! NO GATEWAY WRAPPER - calls *_impl functions directly like search plugin does

use crate::interfaces::di::Container;
use crate::shared::ipc::{ApiError, ErrorCode};
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
    check_first_run_status_impl, download_default_embedding_model_impl,
};

use crate::domain::download::DownloadOperationState;
use crate::features::llm::commands::download_model as download_model_impl;

pub use crate::features::model_management::commands_extra::DownloadedModelResponse;

/// Download a model by ID
///
/// Thin wrapper that delegates to the existing download_model implementation in llm.rs.
/// Maps the response from DownloadModelResponseDto to DownloadModelResponse.
#[tauri::command]
#[specta::specta]
pub async fn download_model(
    model_id: String,
    container: State<'_, Container>,
) -> Result<DownloadModelResponse, ApiError> {
    // Delegate to existing implementation in llm.rs
    match download_model_impl(model_id.clone(), container).await {
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
                crate::shared::error::AppError::InvalidInput(msg) => (ErrorCode::InvalidInput, msg),
                crate::shared::error::AppError::NotFound(msg) => (ErrorCode::NotFound, msg),
                crate::shared::error::AppError::RateLimitExceeded(msg) => {
                    (ErrorCode::RateLimitExceeded, msg)
                }
                crate::shared::error::AppError::Network(msg) => (ErrorCode::NetworkError, msg),
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
        .map_err(|e| ApiError {
            code: ErrorCode::InternalError,
            message: e,
            details: None,
        })
}

/// List all downloaded models
#[tauri::command]
#[specta::specta]
pub async fn list_downloaded_models(
    container: State<'_, Container>,
) -> Result<Vec<DownloadedModelResponse>, ApiError> {
    let json_str = get_models_with_metadata_impl(container.inner())
        .await
        .map_err(|e| ApiError {
            code: ErrorCode::InternalError,
            message: e,
            details: None,
        })?;

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
        .map_err(|e| ApiError {
            code: ErrorCode::InternalError,
            message: e,
            details: None,
        })
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
        .map_err(|e| ApiError {
            code: ErrorCode::InternalError,
            message: e,
            details: None,
        })
}

#[tauri::command]
#[specta::specta]
pub async fn set_active_chat_model(
    model_id: String,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    set_active_chat_model_impl(container.inner(), &model_id)
        .await
        .map_err(|e| ApiError {
            code: ErrorCode::InternalError,
            message: e,
            details: None,
        })
}

#[tauri::command]
#[specta::specta]
pub async fn get_active_chat_model(
    container: State<'_, Container>,
) -> Result<Option<DownloadedModelResponse>, ApiError> {
    let json_str = get_active_chat_model_impl(container.inner())
        .await
        .map_err(|e| ApiError {
            code: ErrorCode::InternalError,
            message: e,
            details: None,
        })?;

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
        .map_err(|e| ApiError {
            code: ErrorCode::InternalError,
            message: e,
            details: None,
        })?;

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
        .map_err(|e| ApiError {
            code: ErrorCode::InternalError,
            message: e,
            details: None,
        })
}

#[tauri::command]
#[specta::specta]
pub async fn clear_active_embedding_model(container: State<'_, Container>) -> Result<(), ApiError> {
    clear_active_embedding_model_impl(container.inner())
        .await
        .map_err(|e| ApiError {
            code: ErrorCode::InternalError,
            message: e,
            details: None,
        })
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
        .map_err(|e| ApiError {
            code: ErrorCode::InternalError,
            message: e,
            details: None,
        })
}

/// Clear the active utility model — reverts HyDE/router back to the chat model.
#[tauri::command]
#[specta::specta]
pub async fn clear_active_utility_model(container: State<'_, Container>) -> Result<(), ApiError> {
    clear_active_utility_model_impl(container.inner())
        .await
        .map_err(|e| ApiError {
            code: ErrorCode::InternalError,
            message: e,
            details: None,
        })
}

#[tauri::command]
#[specta::specta]
pub async fn warm_up_active_chat_model(container: State<'_, Container>) -> Result<(), ApiError> {
    warm_up_active_chat_model_impl(container.inner())
        .await
        .map_err(|e| ApiError {
            code: ErrorCode::InternalError,
            message: e,
            details: None,
        })
}

#[tauri::command]
#[specta::specta]
pub async fn warm_up_active_utility_model(container: State<'_, Container>) -> Result<(), ApiError> {
    warm_up_active_utility_model_impl(container.inner())
        .await
        .map_err(|e| ApiError {
            code: ErrorCode::InternalError,
            message: e,
            details: None,
        })
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
