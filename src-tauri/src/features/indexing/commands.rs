//! Indexing Command Handlers (DDD Architecture)
//!
//! Thin controllers for document indexing operations following Domain-Driven Design.
//! These commands apply cross-cutting concerns (rate limiting, validation, audit logging)
//! and delegate business logic to dedicated use cases for testability and separation of concerns.

use crate::features::indexing::dto::{
    IndexDirectoryRequestDto, IndexDirectoryResponseDto, IndexFileRequestDto, IndexFileResponseDto,
    IndexingStatsDto,
};
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};
use crate::shared::ipc::{ApiResult, ErrorCode};

async fn ensure_embedding_ready(container: &Container) -> Result<()> {
    let embedding_service = container.get_or_load_embedding().await.map_err(|e| {
        AppError::ServiceNotAvailable(format!("Could not load the active embedding model: {}", e))
    })?;

    let is_ready = embedding_service.is_ready().await.map_err(|e| {
        AppError::ServiceNotAvailable(format!("Failed to verify embedding model readiness: {}", e))
    })?;

    if !is_ready {
        return Err(AppError::ServiceNotAvailable(
            "AI embedding model is not ready yet. Activate an embedding model in Settings -> Models and try again."
                .to_string(),
        ));
    }

    Ok(())
}

fn normalize_space_id(space_id: Option<&str>) -> Option<String> {
    space_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

async fn ensure_space_exists(container: &Container, space_id: &str) -> Result<()> {
    let exists = container
        .document_scope()
        .space_exists(space_id)
        .await
        .map_err(|e| e.with_context(&format!("Failed to verify target space '{}'", space_id)))?;
    if !exists {
        return Err(AppError::NotFound(format!("Space not found: {}", space_id)));
    }
    Ok(())
}

async fn assign_document_memberships(
    container: &Container,
    document_ids: &[String],
    space_id: &str,
) -> Result<()> {
    container
        .document_scope()
        .assign_documents(document_ids, space_id)
        .await
        .map_err(|e| {
            e.with_context(&format!(
                "Failed to assign documents to space '{}'",
                space_id
            ))
        })
}

/// Core implementation - Indexes a single file (DDD use case)
///
/// # Arguments
///
/// * `container` - Service container with use cases and security context
/// * `request` - Index request DTO with file path and options
///
/// # Returns
///
/// * `ApiResult::success(IndexFileResponseDto)` - Response with document ID and indexing metadata
/// * `ApiResult::error(...)` - If validation fails or indexing fails
pub async fn index_file_impl(
    container: &Container,
    request: IndexFileRequestDto,
) -> ApiResult<IndexFileResponseDto> {
    tracing::info!(file_path = %request.path, "Command: index_file - ENTRY");
    let requested_space_id = normalize_space_id(request.space_id.as_deref());

    // 1. Input validation (path safety)
    if let Err(e) = container.file_access_config().validate_path(&request.path) {
        return ApiResult::error(
            ErrorCode::InvalidInput,
            format!("Path validation failed: {}", e),
        );
    }

    // 2. Ensure embedding service is loaded before indexing starts.
    // Indexing use cases read from the shared embedding cache.
    if let Err(e) = ensure_embedding_ready(container).await {
        return ApiResult::error(ErrorCode::ServiceNotAvailable, e.to_string());
    }

    if let Some(space_id) = requested_space_id.as_deref() {
        if let Err(e) = ensure_space_exists(container, space_id).await {
            return ApiResult::error(ErrorCode::InvalidInput, e.to_string());
        }
    }

    // 3. Update indexing state for progress tracking
    let state = container.indexing.indexing_state();
    state.start_scanning();
    state.set_total_files(1);
    state.set_current_file(request.path.clone());

    // 4. Execute use case
    let use_case = container.index_file_use_case();
    let result = use_case.execute(request).await;

    // 5. Handle result and update state
    match result {
        Ok(response) => {
            if let Some(space_id) = requested_space_id.as_deref() {
                if let Err(e) = assign_document_memberships(
                    container,
                    std::slice::from_ref(&response.document_id),
                    space_id,
                )
                .await
                {
                    state.file_failed();
                    state.error(e.to_string());
                    tracing::error!(error = %e, space_id = %space_id, document_id = %response.document_id, "Command: index_file - SPACE_ASSIGNMENT_ERROR");
                    return ApiResult::error(ErrorCode::ProcessingError, e.to_string());
                }
            }

            state.file_processed();
            state.complete();

            // 6. Audit logging
            let logger = crate::audit::get_audit_logger();
            crate::audit_success!(
                logger,
                crate::audit::AuditAction::FileIndexed,
                response.document_id.as_str(),
                "path" => response.file_path.as_str()
            )
            .await
            .ok();

            tracing::info!(
                document_id = %response.document_id,
                "Command: index_file - EXIT"
            );
            ApiResult::success(response)
        }
        Err(e) => {
            state.file_failed();
            state.error(e.to_string());
            tracing::error!(error = %e, "Command: index_file - ERROR");
            ApiResult::error(ErrorCode::ProcessingError, e.to_string())
        }
    }
}

/// Core implementation - Indexes a directory recursively (DDD use case)
///
/// Indexes all supported files in a directory and optionally all subdirectories.
/// Processes files in batches with progress tracking.
///
/// # Arguments
///
/// * `container` - Service container with use cases and security context
/// * `request` - Index request DTO with directory path and recursion flag
///
/// # Returns
///
/// * `ApiResult::success(IndexDirectoryResponseDto)` - Response with indexed file count and stats
/// * `ApiResult::error(...)` - If validation fails or indexing fails
pub async fn index_directory_impl(
    container: &Container,
    request: IndexDirectoryRequestDto,
) -> ApiResult<IndexDirectoryResponseDto> {
    let requested_space_id = normalize_space_id(request.space_id.as_deref());

    // 1. Input validation
    if let Err(e) = container.file_access_config().validate_path(&request.path) {
        return ApiResult::error(
            ErrorCode::InvalidInput,
            format!("Path validation failed: {}", e),
        );
    }

    // 2. Ensure embedding service is loaded before indexing starts.
    if let Err(e) = ensure_embedding_ready(container).await {
        return ApiResult::error(ErrorCode::ServiceNotAvailable, e.to_string());
    }

    if let Some(space_id) = requested_space_id.as_deref() {
        if let Err(e) = ensure_space_exists(container, space_id).await {
            return ApiResult::error(ErrorCode::InvalidInput, e.to_string());
        }
    }

    // 3. Execute use case
    let use_case = container.index_directory_use_case();
    match use_case.execute(request).await {
        Ok(response) => {
            if let Some(space_id) = requested_space_id.as_deref() {
                if let Err(e) =
                    assign_document_memberships(container, &response.document_ids, space_id).await
                {
                    tracing::error!(error = %e, space_id = %space_id, "Command: index_directory - SPACE_ASSIGNMENT_ERROR");
                    return ApiResult::error(ErrorCode::ProcessingError, e.to_string());
                }
            }

            // 4. Audit
            let logger = crate::audit::get_audit_logger();
            crate::audit_success!(
                logger,
                crate::audit::AuditAction::FileIndexed,
                "directory",
                "files_indexed" => response.files_indexed.to_string().as_str()
            )
            .await
            .ok();

            ApiResult::success(response)
        }
        Err(e) => ApiResult::error(ErrorCode::ProcessingError, e.to_string()),
    }
}

/// Reindexes an existing document with fresh embeddings (DDD use case)
///
/// Removes old index data and re-indexes the document. Useful when file content
/// has changed or to refresh embeddings with updated models.
pub async fn reindex_document_impl(
    container: &Container,
    document_id: String,
) -> ApiResult<IndexFileResponseDto> {
    tracing::info!(document_id = %document_id, "Command: reindex_document - ENTRY");

    // 1. Ensure embedding service is loaded before reindexing starts.
    if let Err(e) = ensure_embedding_ready(container).await {
        return ApiResult::error(ErrorCode::ServiceNotAvailable, e.to_string());
    }

    // 2. Execute use case
    let use_case = container.reindex_document_use_case();
    match use_case.execute(document_id.clone()).await {
        Ok(response) => {
            // 3. Audit
            let logger = crate::audit::get_audit_logger();
            crate::audit_success!(
                logger,
                crate::audit::AuditAction::DocumentReindexed,
                response.document_id.as_str()
            )
            .await
            .ok();

            tracing::info!(
                document_id = %response.document_id,
                "Command: reindex_document - EXIT"
            );
            ApiResult::success(response)
        }
        Err(e) => {
            tracing::error!(document_id = %document_id, error = %e, "Command: reindex_document - ERROR");
            ApiResult::error(ErrorCode::ProcessingError, e.to_string())
        }
    }
}

/// Core implementation - Deletes a document and all associated data (DDD use case)
///
/// # Arguments
///
/// * `container` - Service container with use cases and security context
/// * `document_id` - Unique identifier of the document to delete
///
/// # Returns
///
/// * `ApiResult::success(())` - Document successfully deleted
/// * `ApiResult::error(...)` - If deletion fails
pub async fn delete_document_impl(container: &Container, document_id: String) -> ApiResult<()> {
    tracing::info!(document_id = %document_id, "Command: delete_document - ENTRY");

    // 1. Execute use case
    let use_case = container.delete_document_use_case();
    match use_case.execute(document_id.clone()).await {
        Ok(_) => {
            // 2. Audit
            let logger = crate::audit::get_audit_logger();
            crate::audit_success!(
                logger,
                crate::audit::AuditAction::FileDeleted,
                document_id.as_str()
            )
            .await
            .ok();

            tracing::info!(document_id = %document_id, "Command: delete_document - EXIT");
            ApiResult::success(())
        }
        Err(e) => {
            tracing::error!(document_id = %document_id, error = %e, "Command: delete_document - ERROR");
            ApiResult::error(ErrorCode::ProcessingError, e.to_string())
        }
    }
}

/// Core implementation - Get indexing statistics
///
/// # Arguments
///
/// * `container` - Service container with document repository
///
/// # Returns
///
/// * `ApiResult::success(IndexingStatsDto)` - Statistics with document and chunk counts
/// * `ApiResult::error(...)` - If database query fails
pub async fn get_indexing_stats_impl(container: &Container) -> ApiResult<IndexingStatsDto> {
    let repo = container.document_repository();

    // Query actual counts from database
    let indexed_documents = match repo.count_documents().await {
        Ok(count) => count,
        Err(e) => return ApiResult::error(ErrorCode::ProcessingError, e.to_string()),
    };

    let total_chunks = match repo.count_chunks().await {
        Ok(count) => count,
        Err(e) => return ApiResult::error(ErrorCode::ProcessingError, e.to_string()),
    };

    ApiResult::success(IndexingStatsDto {
        indexed_documents,
        total_chunks,
    })
}

/// Core implementation - Renames a document's display name
///
/// # Arguments
///
/// * `container` - Service container with use cases and security context
/// * `document_id` - Document ID to rename
/// * `new_name` - New display name for the document
///
/// # Returns
///
/// * `ApiResult::success(RenameDocumentResponseDto)` - Response with new name
/// * `ApiResult::error(...)` - If rename fails
pub async fn rename_document_impl(
    container: &Container,
    document_id: String,
    new_name: String,
) -> ApiResult<crate::features::indexing::use_cases::rename_document::RenameDocumentResponseDto> {
    // 1. Execute use case (validation happens inside use case)
    let use_case = container.rename_document_use_case();
    match use_case
        .execute(document_id.clone(), new_name.clone())
        .await
    {
        Ok(response) => {
            // 2. Audit logging
            let logger = crate::audit::get_audit_logger();
            crate::audit_success!(
                logger,
                crate::audit::AuditAction::DocumentUpdated,
                document_id.as_str(),
                "new_name" => response.new_name.as_str()
            )
            .await
            .ok();

            ApiResult::success(response)
        }
        Err(e) => ApiResult::error(ErrorCode::ProcessingError, e.to_string()),
    }
}

/// IndexProgress structure for progress reporting
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, Default)]
pub struct IndexProgress {
    pub is_indexing: bool,
    pub current_file: Option<String>,
    pub files_processed: u64,
    pub total_files: Option<u64>,
    pub percent_complete: Option<f32>,
    /// Number of files that failed in this run.
    pub failed: u64,
    /// "idle" | "scanning" | "processing" | "complete" | "error" | "cancelled".
    /// Serialised lowercase to match `IndexStatus` and the `indexing://progress`
    /// event payload, so the frontend has one vocabulary.
    pub status: String,
    /// True while a run is paused; drives Pause vs Resume in the UI.
    pub paused: bool,
    /// Newest first, capped at `MAX_TRACKED_FAILURES`, cleared on each new run.
    pub failures: Vec<crate::features::indexing::engine::state::IndexingFailure>,
}

/// Core implementation - Retrieves current indexing progress
///
/// Returns real-time progress from the indexing service.
///
/// # Arguments
///
/// * `container` - Service container with indexing module
///
/// # Returns
///
/// * `ApiResult::success(IndexProgress)` - Current indexing status
pub async fn get_index_progress_impl(container: &Container) -> ApiResult<IndexProgress> {
    let state = container.indexing.indexing_state();
    let snapshot = state.get_snapshot();

    // Map infrastructure model to DTO
    ApiResult::success(IndexProgress {
        is_indexing: snapshot.is_active(),
        current_file: snapshot.current_file,
        files_processed: snapshot.processed as u64,
        total_files: Some(snapshot.total_files as u64),
        percent_complete: Some(snapshot.percentage),
        failed: snapshot.failed as u64,
        // `IndexStatus` serialises lowercase; round-tripping through
        // `serde_json::Value` gives exactly the word the event payload uses.
        status: serde_json::to_value(&snapshot.status)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "idle".to_string()),
        paused: state.is_paused(),
        failures: state.failures(),
    })
}

/// Core implementation - Cancels an in-progress indexing operation
///
/// Signals the running indexing job to stop. The job will check for cancellation
/// flag periodically and stop processing new files.
pub async fn cancel_indexing_impl(container: &Container) -> ApiResult<()> {
    // Signal cancellation
    container.indexing.indexing_state().cancel();
    ApiResult::success(())
}

/// Sets the pause flag on `IndexingState`. The indexing loop checks the flag
/// between files, so an in-flight file finishes before the run stops.
pub async fn pause_indexing_impl(container: &Container) -> ApiResult<()> {
    container.indexing.indexing_state().pause();
    ApiResult::success(())
}

/// Mirror of `pause_indexing_impl`: clears the pause flag and wakes the run.
pub async fn resume_indexing_impl(container: &Container) -> ApiResult<()> {
    container.indexing.indexing_state().resume();
    ApiResult::success(())
}

/// Drop one entry from the run's failure list.
///
/// The list is the in-flight state of the current run, owned by
/// `IndexingState` and read by `get_index_progress`. Dismissing a failed file
/// in one view has to reach that state or the row reappears the moment another
/// view reads the snapshot. Unknown paths are a no-op, not an error — the entry
/// may already have aged out past `MAX_TRACKED_FAILURES` or been cleared by a
/// new run's `reset()`.
pub async fn clear_indexing_failure_impl(container: &Container, path: String) -> ApiResult<()> {
    container.indexing.indexing_state().clear_failure(&path);
    ApiResult::success(())
}
