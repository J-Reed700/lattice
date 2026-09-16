//! Model Plugin Commands - PROPERLY MIGRATED
//!
//! Direct delegation to business logic in model_management_commands.rs
//! NO GATEWAY WRAPPER - calls *_impl functions directly like search plugin does

use crate::interfaces::di::Container;
use crate::shared::api_result::{ApiError, ErrorCode};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::features::model_management::commands::get_all_recommended_models as get_all_recommended_models_impl;
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
use crate::shared::path_confinement::confine_to_root;
use std::path::Path;

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

/// Cancel an in-progress download
#[tauri::command]
#[specta::specta]
pub async fn cancel_download(
    download_id: String,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    let download_manager = container.download_manager().map_err(|e| ApiError {
        code: ErrorCode::InternalError,
        message: format!("Failed to get download manager: {}", e),
        details: None,
    })?;

    download_manager
        .cancel_download(&download_id)
        .await
        .map_err(|e| ApiError {
            code: ErrorCode::InternalError,
            message: format!("Failed to cancel download: {}", e),
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

/// Get download status for a specific download
#[tauri::command]
#[specta::specta]
pub async fn get_download_status(
    download_id: String,
    container: State<'_, Container>,
) -> Result<DownloadStatus, ApiError> {
    let download_manager = container.download_manager().map_err(|e| ApiError {
        code: ErrorCode::InternalError,
        message: format!("Failed to get download manager: {}", e),
        details: None,
    })?;

    let session = download_manager
        .get_download_status(&download_id)
        .await
        .map_err(|e| ApiError {
            code: ErrorCode::InternalError,
            message: format!("Failed to get download status: {}", e),
            details: None,
        })?;

    match session {
        Some(session) => {
            let progress = session.progress().percentage().unwrap_or(0.0) as f32;
            Ok(DownloadStatus {
                id: download_id,
                status: format!("{:?}", session.state()),
                progress,
                error: session.error_message().map(|s| s.to_string()),
            })
        }
        None => Err(ApiError {
            code: ErrorCode::NotFound,
            message: format!("Download not found: {}", download_id),
            details: None,
        }),
    }
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

/// Set active inference model (chat model)
#[tauri::command]
#[specta::specta]
pub async fn set_active_inference_model(
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

/// Validate model compatibility with system
#[tauri::command]
#[specta::specta]
pub async fn validate_model_compatibility(
    model_id: String,
    container: State<'_, Container>,
) -> Result<CompatibilityReport, ApiError> {
    let recommendations = get_all_recommended_models_impl(container)
        .await
        .map_err(|e| ApiError {
            code: ErrorCode::InternalError,
            message: e.to_string(),
            details: None,
        })?;

    match recommendations
        .into_iter()
        .find(|rec| rec.model.id == model_id || rec.model.model_id.as_deref() == Some(&model_id))
    {
        Some(rec) => {
            let warnings = rec.compatibility.recommendations;
            let errors = rec.compatibility.blockers;
            Ok(CompatibilityReport {
                compatible: errors.is_empty(),
                warnings,
                errors,
            })
        }
        None => Ok(CompatibilityReport {
            compatible: false,
            warnings: vec![],
            errors: vec![format!("Model not found: {}", model_id)],
        }),
    }
}

/// Get model information by ID
#[tauri::command]
#[specta::specta]
pub async fn get_model_info(
    model_id: String,
    container: State<'_, Container>,
) -> Result<Option<DownloadedModelResponse>, ApiError> {
    let json_str = get_models_with_metadata_impl(container.inner())
        .await
        .map_err(|e| ApiError {
            code: ErrorCode::InternalError,
            message: e,
            details: None,
        })?;

    let models: Vec<DownloadedModelResponse> =
        serde_json::from_str(&json_str).map_err(|e| ApiError {
            code: ErrorCode::SerializationError,
            message: format!("Failed to parse models: {}", e),
            details: None,
        })?;

    Ok(models.into_iter().find(|m| m.model_id == model_id))
}

/// Export model metadata to file
#[tauri::command]
#[specta::specta]
pub async fn export_model(
    model_id: String,
    export_path: String,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    let json_str = get_models_with_metadata_impl(container.inner())
        .await
        .map_err(|e| ApiError {
            code: ErrorCode::InternalError,
            message: e,
            details: None,
        })?;

    let models: Vec<DownloadedModelResponse> =
        serde_json::from_str(&json_str).map_err(|e| ApiError {
            code: ErrorCode::InvalidInput,
            message: format!("Failed to parse models: {}", e),
            details: None,
        })?;

    let model = models
        .into_iter()
        .find(|candidate| candidate.model_id == model_id)
        .ok_or(ApiError {
            code: ErrorCode::NotFound,
            message: format!("Model not found: {}", model_id),
            details: None,
        })?;

    let payload = serde_json::to_string(&model).map_err(|e| ApiError {
        code: ErrorCode::InvalidInput,
        message: format!("Failed to serialize model: {}", e),
        details: None,
    })?;

    // Confine the destination. This previously called `std::fs::write` on a
    // fully caller-supplied path with no validation whatsoever, which let a
    // compromised renderer clobber any user-writable file — for example
    // `~/.ssh/authorized_keys`.
    let exports_root = container.exports_path();
    std::fs::create_dir_all(&exports_root).map_err(|e| ApiError {
        code: ErrorCode::InternalError,
        message: format!("Failed to create exports directory: {}", e),
        details: None,
    })?;

    let confined =
        confine_to_root(&exports_root, Path::new(&export_path)).map_err(|e| ApiError {
            code: ErrorCode::InvalidInput,
            message: format!(
                "Models can only be exported into {}: {}",
                exports_root.display(),
                e
            ),
            details: None,
        })?;

    std::fs::write(&confined, payload).map_err(|e| ApiError {
        code: ErrorCode::InvalidInput,
        message: format!("Failed to write export file: {}", e),
        details: None,
    })?;

    Ok(())
}

/// Import model from file
#[tauri::command]
#[specta::specta]
pub async fn import_model(
    import_path: String,
    _container: State<'_, Container>,
) -> Result<DownloadedModelResponse, ApiError> {
    let payload = std::fs::read_to_string(&import_path).map_err(|e| ApiError {
        code: ErrorCode::InvalidInput,
        message: format!("Failed to read import file: {}", e),
        details: None,
    })?;

    serde_json::from_str(&payload).map_err(|e| ApiError {
        code: ErrorCode::InvalidInput,
        message: format!("Failed to parse import data: {}", e),
        details: None,
    })
}

/// Refresh model cache (invalidates LLM and embedding caches)
#[tauri::command]
#[specta::specta]
pub async fn refresh_model_cache(container: State<'_, Container>) -> Result<(), ApiError> {
    container.invalidate_llm_cache();
    container.invalidate_embedding_cache();
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct DownloadModelResponse {
    pub download_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct DownloadStatus {
    pub id: String,
    pub status: String,
    pub progress: f32,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ActiveModels {
    pub chat_model: Option<DownloadedModelResponse>,
    pub embedding_model: Option<DownloadedModelResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct CompatibilityReport {
    pub compatible: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
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
