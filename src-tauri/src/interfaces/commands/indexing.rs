//! Indexing commands.
//!
//! The file plugin's indexing commands: thin adapters from IPC onto the
//! indexing feature's command implementations. The file plugin registers
//! them, so their invoke names keep the `plugin:file|` prefix.

use crate::features::file::plugin::commands::IndexingStatus;
use crate::features::indexing::commands as indexing_commands;
use crate::features::indexing::dto::{IndexFileResponseDto, IndexingStatsDto};
use crate::features::indexing::use_cases::rename_document::RenameDocumentResponseDto;
use crate::interfaces::di::Container;
use lattice_core::shared::ipc::{ApiError, ErrorCode};
use tauri::State;

fn unwrap_api_result<T>(result: lattice_core::shared::ipc::ApiResult<T>) -> Result<T, ApiError> {
    match result {
        lattice_core::shared::ipc::ApiResult::Success { data, .. } => Ok(data),
        lattice_core::shared::ipc::ApiResult::Error { error, .. } => Err(ApiError {
            code: error.code,
            message: error.message,
            details: error.details,
        }),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn index_file(
    path: String,
    space_id: Option<String>,
    container: State<'_, Container>,
) -> Result<IndexFileResponseDto, ApiError> {
    let result =
        indexing_commands::index_file_with_settings_impl(container.inner(), path, space_id).await;
    unwrap_api_result(result)
}

#[tauri::command]
#[specta::specta]
pub async fn index_directory(
    path: String,
    recursive: bool,
    space_id: Option<String>,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    let result = indexing_commands::index_directory_with_settings_impl(
        container.inner(),
        path,
        recursive,
        space_id,
    )
    .await;
    unwrap_api_result(result)?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn remove_indexed_file(
    path: String,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    // 1. Find document ID by path
    let repo = container.search.document_repo();
    let maybe_id = repo.find_id_by_path(&path).await.map_err(|e| ApiError {
        code: ErrorCode::InternalError,
        message: e.to_string(),
        details: None,
    })?;

    if let Some(doc_id) = maybe_id {
        // 2. Delete using implementation (now returns ApiResult)
        let result = indexing_commands::delete_document_impl(&container, doc_id).await;

        if result.is_ok() {
            Ok(())
        } else {
            match result {
                lattice_core::shared::ipc::ApiResult::Error { error, .. } => Err(ApiError {
                    code: ErrorCode::InternalError,
                    message: error.message,
                    details: error.details,
                }),
                _ => Ok(()), // Should not happen since we checked is_ok()
            }
        }
    } else {
        // Idempotent success
        Ok(())
    }
}

#[tauri::command]
#[specta::specta]
pub async fn get_indexing_status(
    container: State<'_, Container>,
) -> Result<IndexingStatus, ApiError> {
    let result = indexing_commands::get_index_progress_impl(&container).await;

    match result {
        lattice_core::shared::ipc::ApiResult::Success { data: progress, .. } => {
            Ok(IndexingStatus {
                active: progress.is_indexing,
                progress: progress.percent_complete.unwrap_or(0.0),
            })
        }
        lattice_core::shared::ipc::ApiResult::Error { error, .. } => Err(ApiError {
            code: ErrorCode::InternalError,
            message: error.message,
            details: error.details,
        }),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn get_indexing_stats(
    container: State<'_, Container>,
) -> Result<IndexingStatsDto, ApiError> {
    let result = indexing_commands::get_indexing_stats_impl(container.inner()).await;
    unwrap_api_result(result)
}

#[tauri::command]
#[specta::specta]
pub async fn get_index_progress(
    container: State<'_, Container>,
) -> Result<indexing_commands::IndexProgress, ApiError> {
    let result = indexing_commands::get_index_progress_impl(container.inner()).await;
    unwrap_api_result(result)
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_indexing(container: State<'_, Container>) -> Result<(), ApiError> {
    let result = indexing_commands::cancel_indexing_impl(container.inner()).await;
    unwrap_api_result(result)
}

#[tauri::command]
#[specta::specta]
pub async fn pause_indexing(container: State<'_, Container>) -> Result<(), ApiError> {
    let result = indexing_commands::pause_indexing_impl(&container).await;
    match result {
        lattice_core::shared::ipc::ApiResult::Success { .. } => Ok(()),
        lattice_core::shared::ipc::ApiResult::Error { error, .. } => Err(ApiError {
            code: ErrorCode::InternalError,
            message: error.message,
            details: error.details,
        }),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn resume_indexing(container: State<'_, Container>) -> Result<(), ApiError> {
    let result = indexing_commands::resume_indexing_impl(&container).await;
    match result {
        lattice_core::shared::ipc::ApiResult::Success { .. } => Ok(()),
        lattice_core::shared::ipc::ApiResult::Error { error, .. } => Err(ApiError {
            code: ErrorCode::InternalError,
            message: error.message,
            details: error.details,
        }),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn clear_indexing_failure(
    path: String,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    let result = indexing_commands::clear_indexing_failure_impl(&container, path).await;
    unwrap_api_result(result)
}

#[tauri::command]
#[specta::specta]
pub async fn reindex_file(path: String, container: State<'_, Container>) -> Result<(), ApiError> {
    // 1. Find document ID by path
    let repo = container.search.document_repo();
    let maybe_id = repo.find_id_by_path(&path).await.map_err(|e| ApiError {
        code: ErrorCode::InternalError,
        message: e.to_string(),
        details: None,
    })?;

    if let Some(doc_id) = maybe_id {
        // 2. Reindex using implementation (now returns ApiResult)
        let result = indexing_commands::reindex_document_impl(&container, doc_id).await;

        match result {
            lattice_core::shared::ipc::ApiResult::Success { .. } => Ok(()),
            lattice_core::shared::ipc::ApiResult::Error { error, .. } => Err(ApiError {
                code: ErrorCode::InternalError,
                message: error.message,
                details: error.details,
            }),
        }
    } else {
        Err(ApiError {
            code: ErrorCode::NotFound,
            message: format!("Document not found for path: {}", path),
            details: None,
        })
    }
}

#[tauri::command]
#[specta::specta]
pub async fn delete_document(
    document_id: String,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    let result = indexing_commands::delete_document_impl(&container, document_id).await;
    unwrap_api_result(result)
}

#[tauri::command]
#[specta::specta]
pub async fn rename_document(
    document_id: String,
    new_name: String,
    container: State<'_, Container>,
) -> Result<RenameDocumentResponseDto, ApiError> {
    let result = indexing_commands::rename_document_impl(&container, document_id, new_name).await;
    unwrap_api_result(result)
}
