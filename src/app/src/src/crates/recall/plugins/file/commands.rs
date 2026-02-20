//! File Plugin Commands (Phase 2: Wired to Implementation)
//!
//! Connects plugin stubs to actual implementations in `interfaces::commands`.

use crate::application::dtos::file_dto::UpdateFileMetadataRequestDto;
use crate::application::dtos::indexing_dto::{
    ChunkingStrategyDto, IndexDirectoryRequestDto, IndexFileRequestDto, IndexFileResponseDto,
    IndexingStatsDto,
};
use crate::application::ports::DocumentRepositoryPort;
use crate::application::use_cases::indexing::rename_document::RenameDocumentResponseDto;
use crate::interfaces::commands::{document_list, file as file_commands, indexing_commands};
use crate::interfaces::di::Container;
use crate::shared::api_result::{ApiError, ErrorCode};
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct IndexingStatus {
    pub active: bool,
    pub progress: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct MetadataUpdate {
    pub tags: Option<Vec<String>>,
    pub custom_fields: Option<std::collections::HashMap<String, String>>,
}

fn unwrap_api_result<T>(result: crate::shared::api_result::ApiResult<T>) -> Result<T, ApiError> {
    match result {
        crate::shared::api_result::ApiResult::Success { data, .. } => Ok(data),
        crate::shared::api_result::ApiResult::Error { error, .. } => Err(ApiError {
            code: error.code,
            message: error.message,
            details: error.details,
        }),
    }
}

async fn resolve_indexing_defaults(container: &Container) -> (usize, Option<Vec<String>>) {
    const DEFAULT_CHUNK_TOKENS: usize = 800;
    const MIN_CHUNK_TOKENS: usize = 64;
    const MAX_CHUNK_TOKENS: usize = 4096;

    match container.get_settings_use_case().execute().await {
        Ok(settings) => {
            let chunk_tokens =
                (settings.indexing.chunk_size as usize).clamp(MIN_CHUNK_TOKENS, MAX_CHUNK_TOKENS);
            let include_extensions = if settings.indexing.file_types.is_empty() {
                None
            } else {
                Some(settings.indexing.file_types)
            };
            (chunk_tokens, include_extensions)
        }
        Err(error) => {
            tracing::warn!(
                error = %error,
                "Failed to resolve indexing settings for file plugin; using defaults"
            );
            (DEFAULT_CHUNK_TOKENS, None)
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn index_file(
    path: String,
    space_id: Option<String>,
    container: State<'_, Container>,
) -> Result<IndexFileResponseDto, ApiError> {
    let (chunk_tokens, _) = resolve_indexing_defaults(&container).await;

    let request = IndexFileRequestDto {
        path,
        chunking_strategy: ChunkingStrategyDto::Semantic {
            max_tokens: chunk_tokens,
        },
        tags: None,
        metadata: None,
        space_id,
    };

    let result = indexing_commands::index_file_impl(container.inner(), request).await;
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
    let (chunk_tokens, include_extensions) = resolve_indexing_defaults(&container).await;

    let request = IndexDirectoryRequestDto {
        path,
        recursive,
        chunking_strategy: ChunkingStrategyDto::Semantic {
            max_tokens: chunk_tokens,
        },
        include_extensions,
        space_id,
    };

    let result = indexing_commands::index_directory_impl(container.inner(), request).await;
    unwrap_api_result(result)?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_file_content(
    path: String,
    container: State<'_, Container>,
) -> Result<String, ApiError> {
    file_commands::read_file_content_impl(&container, path)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn update_file_metadata(
    path: String,
    metadata: MetadataUpdate,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    if metadata.custom_fields.is_some() {
        return Err(ApiError {
            code: ErrorCode::InvalidInput,
            message: "custom_fields are not supported for file metadata updates".to_string(),
            details: None,
        });
    }

    let repo = container.search.document_repo();
    let maybe_id = repo.find_id_by_path(&path).await.map_err(|e| ApiError {
        code: ErrorCode::InternalError,
        message: e.to_string(),
        details: None,
    })?;

    let document_id = maybe_id.ok_or_else(|| ApiError {
        code: ErrorCode::NotFound,
        message: format!("Document not found for path: {}", path),
        details: None,
    })?;

    let use_case = container.update_file_metadata_use_case();
    let request = UpdateFileMetadataRequestDto {
        document_id,
        tags: metadata.tags,
    };

    use_case.execute(request).await.map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_file_index(
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

        // Check if successful
        if result.is_ok() {
            Ok(())
        } else {
            // Extract error message from ApiResult
            match result {
                crate::shared::api_result::ApiResult::Error { error, .. } => Err(ApiError {
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
pub async fn remove_indexed_file(
    path: String,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    delete_file_index(path, container).await
}

#[tauri::command]
#[specta::specta]
pub async fn list_indexed_files(
    limit: Option<usize>,
    container: State<'_, Container>,
) -> Result<Vec<String>, ApiError> {
    let effective_limit = limit.unwrap_or(10000);
    let documents = document_list::list_all_documents(container, effective_limit)
        .await
        .map_err(ApiError::from)?;

    Ok(documents.into_iter().map(|doc| doc.file_path).collect())
}

#[tauri::command]
#[specta::specta]
pub async fn list_all_documents(
    limit: usize,
    container: State<'_, Container>,
) -> Result<Vec<document_list::DocumentMetadataDto>, ApiError> {
    document_list::list_all_documents(container, limit)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_indexing_status(
    container: State<'_, Container>,
) -> Result<IndexingStatus, ApiError> {
    let result = indexing_commands::get_index_progress_impl(&container).await;

    match result {
        crate::shared::api_result::ApiResult::Success { data: progress, .. } => {
            Ok(IndexingStatus {
                active: progress.is_indexing,
                progress: progress.percent_complete.unwrap_or(0.0),
            })
        }
        crate::shared::api_result::ApiResult::Error { error, .. } => Err(ApiError {
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
        crate::shared::api_result::ApiResult::Success { .. } => Ok(()),
        crate::shared::api_result::ApiResult::Error { error, .. } => Err(ApiError {
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
        crate::shared::api_result::ApiResult::Success { .. } => Ok(()),
        crate::shared::api_result::ApiResult::Error { error, .. } => Err(ApiError {
            code: ErrorCode::InternalError,
            message: error.message,
            details: error.details,
        }),
    }
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
            crate::shared::api_result::ApiResult::Success { .. } => Ok(()),
            crate::shared::api_result::ApiResult::Error { error, .. } => Err(ApiError {
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

#[tauri::command]
#[specta::specta]
pub async fn validate_file_path(
    path: String,
    container: State<'_, Container>,
) -> Result<bool, ApiError> {
    match container.file_access_config().validate_path(&path) {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn get_file_path_by_id(
    file_id: String,
    container: State<'_, Container>,
) -> Result<String, ApiError> {
    file_commands::get_file_path_by_id(file_id, container)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_indexed_folders(
    container: State<'_, Container>,
) -> Result<Vec<file_commands::IndexedFolder>, ApiError> {
    file_commands::get_indexed_folders(container)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_indexing_activities(
    limit: usize,
    container: State<'_, Container>,
) -> Result<Vec<file_commands::IndexingActivity>, ApiError> {
    file_commands::get_indexing_activities(container, limit)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_recent_documents(
    limit: usize,
    container: State<'_, Container>,
) -> Result<Vec<crate::interfaces::commands::recent_documents::RecentDocument>, ApiError> {
    crate::interfaces::commands::recent_documents::get_recent_documents(limit, container)
        .await
        .map_err(ApiError::from)
}
