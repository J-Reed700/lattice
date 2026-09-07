//! # Start Batch File Import Use Case
//!
//! Creates a batch file import job and spawns background processing.
//!
//! This use case orchestrates:
//! 1. Batch size validation (1-100 files)
//! 2. File path validation
//! 3. Batch job creation
//! 4. Batch item creation
//! 5. Background processing spawn
//!
//! ## Example
//!
//! ```rust,no_run
//! use lattice::application::use_cases::batch::StartBatchFileImportUseCase;
//! use lattice::application::dtos::batch_dto::StartBatchFileImportRequestDto;
//!
//! # async fn example(use_case: StartBatchFileImportUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let request = StartBatchFileImportRequestDto {
//!     file_paths: vec![
//!         "/path/to/file1.txt".to_string(),
//!         "/path/to/file2.pdf".to_string(),
//!     ],
//! };
//!
//! let response = use_case.execute(request).await?;
//! println!("Batch job started: {}", response.job_id);
//! # Ok(())
//! # }
//! ```

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use tracing::{info, instrument};
use uuid::Uuid;

use crate::application::ports::BatchJobRepositoryPort;
use crate::domain::repositories::UnitOfWorkFactory;
use crate::features::batch::dto::{
    StartBatchFileImportRequestDto, StartBatchFileImportResponseDto,
};
use crate::features::indexing::dto::{ChunkingStrategyDto, IndexFileRequestDto};
use crate::features::indexing::use_cases::index_file::PrepareForIndexingOutcome;
use crate::features::indexing::use_cases::IndexFileUseCase;
use crate::shared::domain_types::ValidatedFilePath;
use crate::shared::error::{AppError, Result};

const MIN_BATCH_SIZE: usize = 1;
const MAX_BATCH_SIZE: usize = 100;

/// Start batch file import use case.
///
/// Coordinates batch file import, including:
/// - Batch size validation
/// - File path validation
/// - Job creation
/// - Background processing
///
/// ## Dependencies
///
/// - `BatchJobRepositoryPort`: Persists batch job and items
/// - `IndexFileUseCase`: Processes individual files
/// - `UnitOfWorkFactory`: Creates transactions for atomic batch processing
pub struct StartBatchFileImportUseCase {
    batch_repo: Arc<dyn BatchJobRepositoryPort>,
    index_file_use_case: Arc<IndexFileUseCase>,
    uow_factory: Arc<dyn UnitOfWorkFactory>,
}

impl StartBatchFileImportUseCase {
    /// Create a new start batch file import use case.
    ///
    /// # Arguments
    ///
    /// * `batch_repo` - Repository for batch job persistence
    /// * `index_file_use_case` - Use case for indexing individual files
    /// * `uow_factory` - Factory for creating Unit of Work transactions
    pub fn new(
        batch_repo: Arc<dyn BatchJobRepositoryPort>,
        index_file_use_case: Arc<IndexFileUseCase>,
        uow_factory: Arc<dyn UnitOfWorkFactory>,
    ) -> Self {
        Self {
            batch_repo,
            index_file_use_case,
            uow_factory,
        }
    }

    /// Execute batch file import creation.
    ///
    /// # Arguments
    ///
    /// * `request` - Request containing file paths to import
    ///
    /// # Returns
    ///
    /// Response with job ID for status tracking
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Batch is empty
    /// - Batch exceeds maximum size (100 files)
    /// - Any file path is invalid
    /// - Job creation fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use lattice::application::use_cases::batch::StartBatchFileImportUseCase;
    /// # use lattice::application::dtos::batch_dto::StartBatchFileImportRequestDto;
    /// # async fn example(use_case: StartBatchFileImportUseCase) -> Result<(), Box<dyn std::error::Error>> {
    /// let request = StartBatchFileImportRequestDto {
    ///     file_paths: vec!["/docs/file1.txt".to_string()],
    /// };
    ///
    /// let response = use_case.execute(request).await?;
    /// println!("Job ID: {}", response.job_id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute(
        &self,
        request: StartBatchFileImportRequestDto,
    ) -> Result<StartBatchFileImportResponseDto> {
        // 1. Validate batch size
        let batch_size = request.file_paths.len();

        if batch_size < MIN_BATCH_SIZE {
            return Err(AppError::InvalidInput(
                "Batch must contain at least 1 file".to_string(),
            ));
        }

        if batch_size > MAX_BATCH_SIZE {
            return Err(AppError::InvalidInput(format!(
                "Batch size {} exceeds maximum of {}",
                batch_size, MAX_BATCH_SIZE
            )));
        }

        // 2. Validate each file path
        let mut validated_paths = Vec::new();
        for path_str in &request.file_paths {
            let path = PathBuf::from(path_str);
            let validated = ValidatedFilePath::new(path)?;
            validated_paths.push(validated);
        }

        // 3. Generate job ID
        let job_id = Uuid::new_v4().to_string();

        // 4. Create batch job record
        self.batch_repo
            .create_batch_job(
                &job_id,
                "file_import",
                batch_size as i64,
                None, // No additional options for now
            )
            .await?;

        // 5. Create batch items (one per file)
        // Note: BatchJobRepositoryPort expects URLs, but we use file paths
        self.batch_repo
            .create_batch_items(&job_id, request.file_paths.clone())
            .await?;

        // 6. Spawn background processing task
        let batch_repo = Arc::clone(&self.batch_repo);
        let index_file_use_case = Arc::clone(&self.index_file_use_case);
        let uow_factory = Arc::clone(&self.uow_factory);
        let job_id_clone = job_id.clone();
        let file_paths = request.file_paths.clone();

        tokio::spawn(async move {
            if let Ok(status) = batch_repo.get_batch_job(&job_id_clone).await {
                if status.status == "cancelled" {
                    return;
                }
            } else {
                return;
            }

            // Update job status to "running"
            if let Err(e) = batch_repo
                .update_job_status(
                    &job_id_clone,
                    "running",
                    Some(chrono::Utc::now().to_rfc3339()),
                    None,
                )
                .await
            {
                info!(
                    job_id = %job_id_clone,
                    error = %e,
                    "Failed to update batch job status to running"
                );
                return;
            }

            // Get pending items
            let pending_items = match batch_repo.get_pending_items(&job_id_clone).await {
                Ok(items) => items,
                Err(e) => {
                    if let Err(update_err) = batch_repo
                        .update_job_status(
                            &job_id_clone,
                            "failed",
                            None,
                            Some(chrono::Utc::now().to_rfc3339()),
                        )
                        .await
                    {
                        info!(
                            job_id = %job_id_clone,
                            error = %update_err,
                            "Failed to update batch job status after pending item error"
                        );
                    }
                    info!(
                        job_id = %job_id_clone,
                        error = %e,
                        "Failed to load pending batch items"
                    );
                    return;
                }
            };

            let mut completed = 0i64;
            let mut failed = 0i64;
            let total = pending_items.len() as i64;

            let batch_start = Instant::now();
            info!(
                job_id = %job_id_clone,
                total_files = total,
                "Starting batch file import with Prepare-Then-Commit pattern"
            );

            // ============================================================================
            // PHASE 1: PREPARE (No Transaction - Heavy CPU/IO Work)
            // ============================================================================
            // Extract content and generate embeddings for ALL files BEFORE opening DB transaction.
            // This prevents holding SQLite write lock during slow operations.

            info!(
                job_id = %job_id_clone,
                "Phase 1: Preparing files (extraction + embeddings, no transaction)"
            );

            let preparation_start = Instant::now();
            let mut prepared_files = Vec::new();
            let mut cleanup_candidates: Vec<(String, bool)> = Vec::new();

            for (index, item) in pending_items.iter().enumerate() {
                let job_status = batch_repo.get_batch_job(&job_id_clone).await;
                if let Ok(status) = job_status {
                    if status.status == "cancelled" {
                        info!(
                            job_id = %job_id_clone,
                            prepared_files = cleanup_candidates.len(),
                            "Batch job cancelled during preparation"
                        );
                        for (library_path, imported_new) in &cleanup_candidates {
                            if !*imported_new {
                                continue;
                            }
                            if let Err(cleanup_err) =
                                index_file_use_case.cleanup_library_file(library_path).await
                            {
                                info!(
                                    job_id = %job_id_clone,
                                    file_path = %library_path,
                                    error = %cleanup_err,
                                    "Failed to cleanup imported file after cancellation"
                                );
                            }
                        }
                        return;
                    }
                } else {
                    info!(
                        job_id = %job_id_clone,
                        "Failed to load batch job status during preparation"
                    );
                    return;
                }

                let file_start = Instant::now();
                let index_request = IndexFileRequestDto {
                    path: item.url.clone(),
                    chunking_strategy: ChunkingStrategyDto::Semantic { max_tokens: 800 },
                    tags: None,
                    metadata: None,
                    space_id: None,
                };

                // Prepare file (extraction + embeddings) WITHOUT transaction
                match index_file_use_case
                    .prepare_for_indexing(index_request)
                    .await
                {
                    Ok(PrepareForIndexingOutcome::Prepared(prepared)) => {
                        let (aggregate, embedding_entries, library_path, imported_new) = *prepared;
                        let file_duration = file_start.elapsed();
                        let chunks_created = aggregate.chunks().len();
                        let document_id = aggregate.document().id().to_string();

                        info!(
                            job_id = %job_id_clone,
                            file_index = index + 1,
                            total_files = total,
                            file_path = %item.url,
                            document_id = %document_id,
                            file_duration_ms = file_duration.as_millis(),
                            chunks_created = chunks_created,
                            "File prepared successfully (extraction + embeddings)"
                        );

                        cleanup_candidates.push((library_path.clone(), imported_new));
                        prepared_files.push((
                            item.id.clone(),
                            aggregate,
                            embedding_entries,
                            library_path,
                            imported_new,
                        ));
                    }
                    Ok(PrepareForIndexingOutcome::Duplicate { document_id }) => {
                        info!(
                            job_id = %job_id_clone,
                            file_index = index + 1,
                            total_files = total,
                            file_path = %item.url,
                            document_id = %document_id,
                            "File already indexed (duplicate detected)"
                        );

                        if let Err(e) = batch_repo
                            .update_item_status(&item.id, "completed", Some(&document_id), None)
                            .await
                        {
                            info!(
                                job_id = %job_id_clone,
                                item_id = %item.id,
                                error = %e,
                                "Failed to update batch item status to completed"
                            );
                        }

                        completed += 1;
                        let progress = (completed + failed) as f64 / total as f64;
                        if let Err(e) = batch_repo
                            .update_progress(&job_id_clone, completed, failed, progress)
                            .await
                        {
                            info!(
                                job_id = %job_id_clone,
                                error = %e,
                                "Failed to update batch progress after duplicate"
                            );
                        }
                    }
                    Err(e) => {
                        failed += 1;
                        let file_duration = file_start.elapsed();

                        info!(
                            job_id = %job_id_clone,
                            file_index = index + 1,
                            total_files = total,
                            file_path = %item.url,
                            file_duration_ms = file_duration.as_millis(),
                            error = %e,
                            "File preparation failed (extraction or embeddings)"
                        );

                        if let Err(update_err) = batch_repo
                            .update_item_status(&item.id, "failed", None, Some(&e.to_string()))
                            .await
                        {
                            info!(
                                job_id = %job_id_clone,
                                item_id = %item.id,
                                error = %update_err,
                                "Failed to update batch item status to failed"
                            );
                        }

                        let progress = (completed + failed) as f64 / total as f64;
                        if let Err(update_err) = batch_repo
                            .update_progress(&job_id_clone, completed, failed, progress)
                            .await
                        {
                            info!(
                                job_id = %job_id_clone,
                                error = %update_err,
                                "Failed to update batch progress after preparation failure"
                            );
                        }
                    }
                }
            }

            let preparation_duration = preparation_start.elapsed();
            info!(
                job_id = %job_id_clone,
                prepared_count = prepared_files.len(),
                failed_count = failed,
                preparation_duration_ms = preparation_duration.as_millis(),
                "Phase 1 complete: All files prepared"
            );

            let job_status = batch_repo.get_batch_job(&job_id_clone).await;
            if let Ok(status) = job_status {
                if status.status == "cancelled" {
                    info!(
                        job_id = %job_id_clone,
                        prepared_files = cleanup_candidates.len(),
                        "Batch job cancelled before commit"
                    );
                    for (library_path, imported_new) in &cleanup_candidates {
                        if !*imported_new {
                            continue;
                        }
                        if let Err(cleanup_err) =
                            index_file_use_case.cleanup_library_file(library_path).await
                        {
                            info!(
                                job_id = %job_id_clone,
                                file_path = %library_path,
                                error = %cleanup_err,
                                "Failed to cleanup imported file after cancellation"
                            );
                        }
                    }
                    return;
                }
            } else {
                info!(
                    job_id = %job_id_clone,
                    "Failed to load batch job status before commit"
                );
                return;
            }

            // ============================================================================
            // PHASE 2: COMMIT (Fast DB Transaction)
            // ============================================================================
            // Now save all prepared aggregates in a SINGLE fast transaction.
            // This is where we hold the SQLite write lock - only for database operations.

            if !prepared_files.is_empty() {
                info!(
                    job_id = %job_id_clone,
                    files_to_save = prepared_files.len(),
                    "Phase 2: Saving prepared files (fast transaction)"
                );

                let commit_start = Instant::now();

                // Create UoW and save all prepared files
                let mut uow = match uow_factory.create().await {
                    Ok(uow) => uow,
                    Err(e) => {
                        info!(
                            job_id = %job_id_clone,
                            error = %e,
                            "Failed to create UoW for batch commit"
                        );

                        // Mark all prepared files as failed
                        for (item_id, _, _, _, _) in prepared_files {
                            if let Err(update_err) = batch_repo
                                .update_item_status(
                                    &item_id,
                                    "failed",
                                    None,
                                    Some("Failed to create database transaction"),
                                )
                                .await
                            {
                                info!(
                                    job_id = %job_id_clone,
                                    item_id = %item_id,
                                    error = %update_err,
                                    "Failed to mark batch item failed after UoW error"
                                );
                            }
                        }

                        for (library_path, imported_new) in &cleanup_candidates {
                            if !*imported_new {
                                continue;
                            }
                            if let Err(cleanup_err) =
                                index_file_use_case.cleanup_library_file(library_path).await
                            {
                                info!(
                                    job_id = %job_id_clone,
                                    file_path = %library_path,
                                    error = %cleanup_err,
                                    "Failed to cleanup imported file after UoW creation failure"
                                );
                            }
                        }

                        if let Err(update_err) = batch_repo
                            .update_job_status(
                                &job_id_clone,
                                "failed",
                                None,
                                Some(chrono::Utc::now().to_rfc3339()),
                            )
                            .await
                        {
                            info!(
                                job_id = %job_id_clone,
                                error = %update_err,
                                "Failed to update batch job status after UoW error"
                            );
                        }

                        return;
                    }
                };

                // Collect item IDs before scope consumes prepared_files
                let all_item_ids: Vec<String> = prepared_files
                    .iter()
                    .map(|(item_id, _, _, _, _)| item_id.clone())
                    .collect();

                // PATTERN: Inner scope ensures repositories are dropped before commit/rollback
                // This satisfies Rust's borrow checker - repositories borrow UoW immutably,
                // but commit/rollback need mutable access. Scope ensures borrows end first.
                //
                // CRITICAL: Do NOT call batch_repo inside this scope! That would cause
                // SQLITE_BUSY deadlocks (batch_repo uses separate connection, uow holds write lock).
                let (db_error_occurred, save_results, failed_items) = {
                    // Get repositories from UoW
                    let document_repo = match uow.document_repository() {
                        Ok(repo) => repo,
                        Err(e) => {
                            info!(
                                job_id = %job_id_clone,
                                error = %e,
                                "Failed to get document repository from UoW"
                            );
                            // Cannot rollback here - repo borrow exists in Ok branch
                            // Will handle via outer error propagation
                            return;
                        }
                    };

                    let embedding_repo = match uow.embedding_repository() {
                        Ok(repo) => repo,
                        Err(e) => {
                            info!(
                                job_id = %job_id_clone,
                                error = %e,
                                "Failed to get embedding repository from UoW"
                            );
                            return;
                        }
                    };

                    // Save all aggregates and embeddings
                    let mut db_error_occurred = false;
                    let mut save_results = Vec::new();
                    let mut failed_items = Vec::new();

                    for (item_id, aggregate, embedding_entries, _library_path, _imported_new) in
                        prepared_files
                    {
                        let document_id = aggregate.document().id().to_string();
                        let chunks_created = aggregate.chunks().len();

                        // Save document aggregate
                        if let Err(e) = document_repo.save(&aggregate).await {
                            info!(
                                job_id = %job_id_clone,
                                document_id = %document_id,
                                error = %e,
                                "Failed to save document aggregate"
                            );

                            // Don't call batch_repo here (SQLITE_BUSY risk)
                            // Store failure for later processing outside transaction
                            failed_items.push((item_id, format!("DB save failed: {}", e)));
                            db_error_occurred = true;
                            break; // CRITICAL: Stop on first DB error
                        }

                        // Save embeddings batch
                        if let Err(e) = embedding_repo.save_batch(embedding_entries).await {
                            info!(
                                job_id = %job_id_clone,
                                document_id = %document_id,
                                error = %e,
                                "Failed to save embeddings"
                            );

                            // Don't call batch_repo here (SQLITE_BUSY risk)
                            failed_items.push((item_id, format!("Embedding save failed: {}", e)));
                            db_error_occurred = true;
                            break; // CRITICAL: Stop on first DB error
                        }

                        // Store successful save result
                        save_results.push((item_id, document_id, chunks_created));
                    }

                    // Repositories are dropped HERE when scope exits
                    (db_error_occurred, save_results, failed_items)
                };

                // NOW we can safely commit/rollback - no repository borrows exist
                // Explicit state tracking to prevent ghost record anomalies
                let commit_duration = commit_start.elapsed();

                if db_error_occurred {
                    // ROLLBACK PATH: Database error occurred
                    let root_cause = failed_items
                        .first()
                        .map(|(_, msg)| msg.clone())
                        .unwrap_or_else(|| "unknown database error".to_string());
                    info!(
                        job_id = %job_id_clone,
                        root_cause = %root_cause,
                        "Database error occurred, rolling back transaction"
                    );

                    let rollback_result = uow.rollback().await;

                    if let Err(e) = rollback_result {
                        info!(
                            job_id = %job_id_clone,
                            error = %e,
                            commit_duration_ms = commit_duration.as_millis(),
                            "Rollback failed"
                        );
                    } else {
                        info!(
                            job_id = %job_id_clone,
                            commit_duration_ms = commit_duration.as_millis(),
                            "Transaction rolled back successfully"
                        );
                    }

                    // CRITICAL: Mark ALL prepared files as failed (rollback = nothing was saved)
                    for item_id in &all_item_ids {
                        failed += 1;

                        if let Err(e) = batch_repo
                            .update_item_status(
                                item_id,
                                "failed",
                                None,
                                Some(&format!(
                                    "Batch transaction rolled back due to database error: {}",
                                    root_cause
                                )),
                            )
                            .await
                        {
                            info!(
                                job_id = %job_id_clone,
                                item_id = %item_id,
                                error = %e,
                                "Failed to update batch item status after rollback"
                            );
                        }

                        let progress = (completed + failed) as f64 / total as f64;
                        if let Err(e) = batch_repo
                            .update_progress(&job_id_clone, completed, failed, progress)
                            .await
                        {
                            info!(
                                job_id = %job_id_clone,
                                error = %e,
                                "Failed to update batch progress after rollback"
                            );
                        }
                    }

                    for (library_path, imported_new) in &cleanup_candidates {
                        if !*imported_new {
                            continue;
                        }
                        if let Err(cleanup_err) =
                            index_file_use_case.cleanup_library_file(library_path).await
                        {
                            info!(
                                job_id = %job_id_clone,
                                file_path = %library_path,
                                error = %cleanup_err,
                                "Failed to cleanup imported file after rollback"
                            );
                        }
                    }
                } else {
                    // COMMIT PATH: All database saves succeeded
                    info!(
                        job_id = %job_id_clone,
                        files_to_commit = save_results.len(),
                        "All files saved successfully, committing transaction"
                    );

                    let commit_result = uow.commit().await;

                    if let Err(e) = commit_result {
                        // COMMIT FAILED: Treat as rollback
                        info!(
                            job_id = %job_id_clone,
                            error = %e,
                            commit_duration_ms = commit_duration.as_millis(),
                            "Commit failed, transaction rolled back"
                        );

                        // Mark ALL prepared files as failed
                        for item_id in &all_item_ids {
                            failed += 1;

                            if let Err(update_err) = batch_repo
                                .update_item_status(
                                    item_id,
                                    "failed",
                                    None,
                                    Some(&format!("Commit failed: {}", e)),
                                )
                                .await
                            {
                                info!(
                                    job_id = %job_id_clone,
                                    item_id = %item_id,
                                    error = %update_err,
                                    "Failed to update batch item status after commit failure"
                                );
                            }

                            for (library_path, imported_new) in &cleanup_candidates {
                                if !*imported_new {
                                    continue;
                                }
                                if let Err(cleanup_err) =
                                    index_file_use_case.cleanup_library_file(library_path).await
                                {
                                    info!(
                                        job_id = %job_id_clone,
                                        file_path = %library_path,
                                        error = %cleanup_err,
                                        "Failed to cleanup imported file after commit failure"
                                    );
                                }
                            }

                            let progress = (completed + failed) as f64 / total as f64;
                            if let Err(update_err) = batch_repo
                                .update_progress(&job_id_clone, completed, failed, progress)
                                .await
                            {
                                info!(
                                    job_id = %job_id_clone,
                                    error = %update_err,
                                    "Failed to update batch progress after commit failure"
                                );
                            }
                        }
                    } else {
                        // COMMIT SUCCEEDED: Mark files as completed
                        info!(
                            job_id = %job_id_clone,
                            commit_duration_ms = commit_duration.as_millis(),
                            "Phase 2 complete: Transaction committed successfully"
                        );

                        // Update batch job status for successfully saved files
                        for (item_id, document_id, chunks_created) in save_results {
                            completed += 1;

                            info!(
                                job_id = %job_id_clone,
                                document_id = %document_id,
                                chunks_created = chunks_created,
                                "Document saved to database"
                            );

                            if let Err(update_err) = batch_repo
                                .update_item_status(&item_id, "completed", Some(&document_id), None)
                                .await
                            {
                                info!(
                                    job_id = %job_id_clone,
                                    item_id = %item_id,
                                    error = %update_err,
                                    "Failed to update batch item status to completed"
                                );
                            }

                            // Update progress
                            let progress = (completed + failed) as f64 / total as f64;
                            if let Err(update_err) = batch_repo
                                .update_progress(&job_id_clone, completed, failed, progress)
                                .await
                            {
                                info!(
                                    job_id = %job_id_clone,
                                    error = %update_err,
                                    "Failed to update batch progress after completion"
                                );
                            }
                        }
                    }
                }
            }

            // Mark job as completed
            if let Ok(status) = batch_repo.get_batch_job(&job_id_clone).await {
                if status.status == "cancelled" {
                    return;
                }
            }

            let final_status = if failed == total {
                "failed"
            } else {
                "completed"
            };

            let batch_duration = batch_start.elapsed();
            let files_per_second = if batch_duration.as_secs() > 0 {
                total as f64 / batch_duration.as_secs_f64()
            } else {
                total as f64
            };

            info!(
                job_id = %job_id_clone,
                total_files = total,
                completed = completed,
                failed = failed,
                batch_duration_ms = batch_duration.as_millis(),
                batch_duration_sec = batch_duration.as_secs_f64(),
                files_per_second = format!("{:.2}", files_per_second),
                final_status = final_status,
                "Batch file import completed"
            );

            if let Err(e) = batch_repo
                .update_job_status(
                    &job_id_clone,
                    final_status,
                    None,
                    Some(chrono::Utc::now().to_rfc3339()),
                )
                .await
            {
                info!(
                    job_id = %job_id_clone,
                    error = %e,
                    "Failed to update final batch job status"
                );
            }
        });

        // 7. Return job ID immediately
        Ok(StartBatchFileImportResponseDto { job_id })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use crate::application::ports::batch_job_repository_port::{
        BatchJobItem, BatchJobStatus as PortBatchJobStatus, BatchJobSummary,
    };
    use crate::features::indexing::dto::IndexFileResponseDto;

    // Mock UnitOfWorkFactory
    struct MockUoWFactory;

    #[async_trait]
    impl crate::domain::repositories::UnitOfWorkFactory for MockUoWFactory {
        async fn create(&self) -> Result<Box<dyn crate::domain::repositories::UnitOfWork + Send>> {
            use crate::domain::repositories::mocks::MockUnitOfWork;
            let mut mock = MockUnitOfWork::new();

            // Setup basic expectations
            mock.expect_commit().returning(|| Ok(()));
            mock.expect_rollback().returning(|| Ok(()));

            Ok(Box::new(mock))
        }
    }

    // Mock batch repository
    struct MockBatchRepo {
        jobs: Arc<Mutex<HashMap<String, String>>>, // job_id -> status
        items: Arc<Mutex<HashMap<String, Vec<String>>>>, // job_id -> file_paths
    }

    fn temp_path(name: &str) -> String {
        std::env::temp_dir()
            .join(name)
            .to_string_lossy()
            .to_string()
    }

    impl MockBatchRepo {
        fn new() -> Self {
            Self {
                jobs: Arc::new(Mutex::new(HashMap::new())),
                items: Arc::new(Mutex::new(HashMap::new())),
            }
        }
    }

    #[async_trait]
    impl BatchJobRepositoryPort for MockBatchRepo {
        async fn create_batch_job(
            &self,
            job_id: &str,
            _job_type: &str,
            _total_items: i64,
            _options: Option<&str>,
        ) -> Result<()> {
            self.jobs
                .lock()
                .unwrap()
                .insert(job_id.to_string(), "pending".to_string());
            Ok(())
        }

        async fn create_batch_items(&self, job_id: &str, urls: Vec<String>) -> Result<()> {
            self.items.lock().unwrap().insert(job_id.to_string(), urls);
            Ok(())
        }

        async fn update_job_status(
            &self,
            job_id: &str,
            status: &str,
            _started_at: Option<String>,
            _completed_at: Option<String>,
        ) -> Result<()> {
            if let Some(job_status) = self.jobs.lock().unwrap().get_mut(job_id) {
                *job_status = status.to_string();
            }
            Ok(())
        }

        async fn update_progress(
            &self,
            _job_id: &str,
            _completed: i64,
            _failed: i64,
            _progress: f64,
        ) -> Result<()> {
            Ok(())
        }

        async fn update_item_status(
            &self,
            _item_id: &str,
            _status: &str,
            _document_id: Option<&str>,
            _error_message: Option<&str>,
        ) -> Result<()> {
            Ok(())
        }

        async fn get_batch_job(&self, _job_id: &str) -> Result<PortBatchJobStatus> {
            Ok(PortBatchJobStatus {
                id: "test".to_string(),
                job_type: "file_import".to_string(),
                status: "pending".to_string(),
                total_items: 0,
                completed_items: 0,
                failed_items: 0,
                progress: 0.0,
                created_at: "".to_string(),
                started_at: None,
                completed_at: None,
                error_message: None,
                items: vec![],
            })
        }

        async fn get_pending_items(&self, job_id: &str) -> Result<Vec<BatchJobItem>> {
            let items = self.items.lock().unwrap();
            let urls = items.get(job_id).cloned().unwrap_or_default();

            Ok(urls
                .into_iter()
                .enumerate()
                .map(|(i, url)| BatchJobItem {
                    id: format!("{}-{}", job_id, i),
                    url,
                })
                .collect())
        }

        async fn cancel_pending_items(&self, _job_id: &str) -> Result<usize> {
            Ok(0)
        }

        async fn list_batch_jobs(
            &self,
            _limit: Option<i64>,
            _offset: Option<i64>,
        ) -> Result<Vec<BatchJobSummary>> {
            Ok(vec![])
        }

        async fn delete_batch_job(&self, _job_id: &str) -> Result<()> {
            Ok(())
        }
    }

    // Mock index file use case
    fn create_mock_index_file_use_case() -> IndexFileUseCase {
        use crate::application::ports::{
            EmbeddingPort, EmbeddingRepositoryPort, FileStoragePort, RepositoryPort,
        };
        use crate::domain::entities::document::Document;
        use crate::domain::repositories::UnitOfWorkFactory;
        use crate::features::embedding::entity::Embedding;
        use std::path::Path;

        struct MockFileStorage;
        #[async_trait]
        impl FileStoragePort for MockFileStorage {
            async fn read_file(&self, _path: &Path) -> Result<String> {
                Ok("test content".to_string())
            }
            async fn write_file(&self, _path: &Path, _content: &str) -> Result<()> {
                Ok(())
            }
            async fn delete_file(&self, _path: &Path) -> Result<()> {
                Ok(())
            }
            async fn read_file_bytes(&self, _path: &Path) -> Result<Vec<u8>> {
                Ok(vec![])
            }
            async fn write_file_bytes(&self, _path: &Path, _content: &[u8]) -> Result<()> {
                Ok(())
            }
            async fn compute_hash(&self, _path: &Path) -> Result<String> {
                Ok("hash".to_string())
            }
            async fn exists(&self, _path: &Path) -> bool {
                true
            }
            async fn metadata(
                &self,
                _path: &Path,
            ) -> Result<crate::application::ports::file_storage_port::FileMetadata> {
                use crate::application::ports::file_storage_port::FileMetadata;
                use chrono::Utc;
                Ok(FileMetadata {
                    size: 100,
                    modified_at: Utc::now().timestamp(),
                    is_file: true,
                    is_directory: false,
                })
            }
        }

        #[async_trait]
        impl crate::application::ports::ContentAddressedStoragePort for MockFileStorage {
            async fn import_file(
                &self,
                _source_path: &Path,
            ) -> Result<(std::path::PathBuf, String)> {
                Ok((
                    std::path::PathBuf::from("/mock/path"),
                    "mockhash".to_string(),
                ))
            }
            async fn exists_by_hash(&self, _hash: &str) -> Result<bool> {
                Ok(false)
            }
            async fn get_path_by_hash(&self, _hash: &str) -> Result<Option<std::path::PathBuf>> {
                Ok(None)
            }
        }

        struct MockContentExtractor;
        #[async_trait]
        impl crate::application::ports::ContentExtractionPort for MockContentExtractor {
            async fn extract_content(
                &self,
                _path: &std::path::Path,
            ) -> Result<crate::application::ports::content_extraction_port::ExtractedContentData>
            {
                Ok(
                    crate::application::ports::content_extraction_port::ExtractedContentData {
                        text: "Mock content".to_string(),
                        mime_type: "text/plain".to_string(),
                        page_count: Some(1),
                        word_count: 2,
                        char_count: 12,
                    },
                )
            }
            fn is_supported(&self, _path: &std::path::Path) -> bool {
                true
            }
        }

        struct MockEmbedder;
        #[async_trait]
        impl EmbeddingPort for MockEmbedder {
            async fn embed_single(&self, _text: &str) -> Result<Vec<f32>> {
                Ok(vec![0.1, 0.2, 0.3])
            }
            async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
                Ok(texts.iter().map(|_| vec![0.1, 0.2, 0.3]).collect())
            }
            fn dimension(&self) -> usize {
                3
            }
            async fn is_ready(&self) -> Result<bool> {
                Ok(true)
            }
        }

        struct MockDocRepo;
        #[async_trait]
        impl RepositoryPort<Document> for MockDocRepo {
            async fn save(&self, _entity: &Document) -> Result<()> {
                Ok(())
            }
            async fn find_by_id(&self, _id: &str) -> Result<Option<Document>> {
                Ok(None)
            }
            async fn find_all(&self) -> Result<Vec<Document>> {
                Ok(vec![])
            }
            async fn exists(&self, _id: &str) -> Result<bool> {
                Ok(false)
            }
            async fn find_by_filter(
                &self,
                _filter: &dyn crate::application::ports::repository_port::Filter,
            ) -> Result<Vec<Document>> {
                Ok(vec![])
            }
            async fn save_batch(&self, _entities: &[Document]) -> Result<()> {
                Ok(())
            }
            async fn delete(&self, _id: &str) -> Result<()> {
                Ok(())
            }
            async fn delete_batch(&self, _ids: &[&str]) -> Result<()> {
                Ok(())
            }
            async fn count(&self) -> Result<usize> {
                Ok(0)
            }
        }

        #[async_trait]
        impl crate::application::ports::DocumentRepositoryPort for MockDocRepo {
            async fn find_by_checksum(
                &self,
                _checksum: &crate::domain::value_objects::Checksum,
            ) -> Result<Option<Document>> {
                Ok(None)
            }
            async fn count_documents(&self) -> Result<i64> {
                Ok(0)
            }
            async fn count_chunks(&self) -> Result<i64> {
                Ok(0)
            }
            async fn find_file_path_by_id(&self, _document_id: &str) -> Result<String> {
                Ok(String::new())
            }
            async fn find_id_by_path(&self, _file_path: &str) -> Result<Option<String>> {
                Ok(None)
            }
            async fn document_exists(&self, _document_id: &str) -> Result<bool> {
                Ok(false)
            }
            async fn delete(&self, _document_id: &str) -> Result<()> {
                Ok(())
            }
            async fn find_all_paginated(&self, _limit: usize) -> Result<Vec<Document>> {
                Ok(vec![])
            }
        }

        struct MockEmbeddingRepo;
        #[async_trait]
        impl EmbeddingRepositoryPort for MockEmbeddingRepo {
            async fn create(
                &self,
                _chunk_id: &str,
                _vector: &[f32],
                _model: &str,
            ) -> Result<String> {
                unimplemented!()
            }
            async fn find_by_chunk(&self, _chunk_id: &str) -> Result<Option<Embedding>> {
                unimplemented!()
            }
            async fn save(&self, _entity: &Embedding, _vector: Vec<f32>) -> Result<()> {
                Ok(())
            }
            async fn save_batch(&self, _entries: Vec<(Embedding, Vec<f32>)>) -> Result<()> {
                Ok(())
            }
            async fn find_by_chunk_id(
                &self,
                _chunk_id: &str,
            ) -> Result<Option<(Embedding, Vec<f32>)>> {
                Ok(None)
            }
            async fn find_by_document_id(
                &self,
                _document_id: &str,
            ) -> Result<Vec<(Embedding, Vec<f32>)>> {
                Ok(vec![])
            }
            async fn delete_by_chunk_id(&self, _chunk_id: &str) -> Result<()> {
                Ok(())
            }
            async fn delete_by_document_id(&self, _document_id: &str) -> Result<()> {
                Ok(())
            }
            async fn count(&self) -> Result<i64> {
                Ok(0)
            }

            async fn create_batch(
                &self,
                _entries: Vec<(Embedding, Vec<f32>)>,
            ) -> Result<Vec<String>> {
                Ok(vec![])
            }
        }

        struct MockUnitOfWork;

        #[async_trait]
        impl crate::domain::repositories::UnitOfWork for MockUnitOfWork {
            fn chunk_repository(
                &self,
            ) -> Result<Box<dyn crate::application::ports::ChunkRepositoryPort + Send + '_>>
            {
                Err(AppError::InvalidState(
                    "Chunk repository not used in test".to_string(),
                ))
            }

            fn document_repository(
                &self,
            ) -> Result<Box<dyn crate::application::ports::DocumentRepositoryPort + Send + '_>>
            {
                Ok(Box::new(MockDocRepo))
            }

            fn embedding_repository(&self) -> Result<Box<dyn EmbeddingRepositoryPort + Send + '_>> {
                Ok(Box::new(MockEmbeddingRepo))
            }

            fn search_repository(
                &self,
            ) -> Result<Box<dyn crate::domain::repositories::SearchRepository + Send + '_>>
            {
                Err(AppError::InvalidState(
                    "Search repository not used in test".to_string(),
                ))
            }

            fn batch_job_repository(
                &self,
            ) -> Result<Box<dyn crate::application::ports::BatchJobRepositoryPort + Send + '_>>
            {
                Err(AppError::InvalidState(
                    "Batch job repository not used in test".to_string(),
                ))
            }

            fn system_repository(
                &self,
            ) -> Result<Box<dyn crate::domain::repositories::SystemRepository + Send + '_>>
            {
                Err(AppError::InvalidState(
                    "System repository not used in test".to_string(),
                ))
            }

            fn model_repository(
                &self,
            ) -> Result<Box<dyn crate::domain::repositories::unit_of_work::ModelRepositoryPort + '_>>
            {
                Err(AppError::InvalidState(
                    "Model repository not used in test".to_string(),
                ))
            }

            fn model_file_repository(
                &self,
            ) -> Result<
                Box<dyn crate::domain::repositories::unit_of_work::ModelFileRepositoryPort + '_>,
            > {
                Err(AppError::InvalidState(
                    "Model file repository not used in test".to_string(),
                ))
            }

            async fn commit(&mut self) -> Result<()> {
                Ok(())
            }

            async fn rollback(&mut self) -> Result<()> {
                Ok(())
            }
        }

        struct MockUnitOfWorkFactory;

        #[async_trait]
        impl UnitOfWorkFactory for MockUnitOfWorkFactory {
            async fn create(
                &self,
            ) -> Result<Box<dyn crate::domain::repositories::UnitOfWork + Send>> {
                Ok(Box::new(MockUnitOfWork))
            }
        }

        IndexFileUseCase::new(
            Arc::new(MockFileStorage), // content_storage
            Arc::new(MockFileStorage), // file_storage
            Arc::new(MockContentExtractor),
            Arc::new(MockEmbedder),
            Arc::new(MockDocRepo),
            Arc::new(MockEmbeddingRepo),
            Arc::new(MockUnitOfWorkFactory),
        )
    }

    #[tokio::test]
    async fn test_valid_batch_succeeds() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let mut file_paths = Vec::new();

        for i in 0..10 {
            let file_path = temp_dir.path().join(format!("file{}.txt", i));
            fs::write(&file_path, "test content").unwrap();
            file_paths.push(file_path.to_str().unwrap().to_string());
        }

        let batch_repo = Arc::new(MockBatchRepo::new());
        let index_file = Arc::new(create_mock_index_file_use_case());
        let uow_factory = Arc::new(MockUoWFactory);
        let use_case =
            StartBatchFileImportUseCase::new(batch_repo.clone(), index_file, uow_factory);

        let request = StartBatchFileImportRequestDto { file_paths };
        let response = use_case.execute(request).await.unwrap();

        assert!(!response.job_id.is_empty());

        // Verify job was created
        let jobs = batch_repo.jobs.lock().unwrap();
        assert!(jobs.contains_key(&response.job_id));
    }

    #[tokio::test]
    async fn test_empty_batch_fails() {
        let batch_repo = Arc::new(MockBatchRepo::new());
        let index_file = Arc::new(create_mock_index_file_use_case());
        let uow_factory = Arc::new(MockUoWFactory);
        let use_case = StartBatchFileImportUseCase::new(batch_repo, index_file, uow_factory);

        let request = StartBatchFileImportRequestDto { file_paths: vec![] };

        let result = use_case.execute(request).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            AppError::InvalidInput(msg) => {
                assert!(msg.contains("at least 1"));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_too_large_batch_fails() {
        let batch_repo = Arc::new(MockBatchRepo::new());
        let index_file = Arc::new(create_mock_index_file_use_case());
        let uow_factory = Arc::new(MockUoWFactory);
        let use_case = StartBatchFileImportUseCase::new(batch_repo, index_file, uow_factory);

        let file_paths = (0..101)
            .map(|i| temp_path(&format!("file{}.txt", i)))
            .collect();
        let request = StartBatchFileImportRequestDto { file_paths };

        let result = use_case.execute(request).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            AppError::InvalidInput(msg) => {
                assert!(msg.contains("exceeds maximum"));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_invalid_path_fails() {
        let batch_repo = Arc::new(MockBatchRepo::new());
        let index_file = Arc::new(create_mock_index_file_use_case());
        let uow_factory = Arc::new(MockUoWFactory);
        let use_case = StartBatchFileImportUseCase::new(batch_repo, index_file, uow_factory);

        let request = StartBatchFileImportRequestDto {
            file_paths: vec!["../../../etc/passwd".to_string()],
        };

        let result = use_case.execute(request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_background_processing_updates_status() {
        use std::fs;
        use tempfile::TempDir;
        use tokio::time::{sleep, Duration};

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("file.txt");
        fs::write(&file_path, "test content").unwrap();

        let batch_repo = Arc::new(MockBatchRepo::new());
        let index_file = Arc::new(create_mock_index_file_use_case());
        let uow_factory = Arc::new(MockUoWFactory);
        let use_case =
            StartBatchFileImportUseCase::new(batch_repo.clone(), index_file, uow_factory);

        let request = StartBatchFileImportRequestDto {
            file_paths: vec![file_path.to_str().unwrap().to_string()],
        };

        let response = use_case.execute(request).await.unwrap();

        // Wait for background processing
        sleep(Duration::from_millis(100)).await;

        // Check that status was updated
        let jobs = batch_repo.jobs.lock().unwrap();
        let status = jobs.get(&response.job_id).unwrap();

        // Status should have been updated from "pending" - could be "running", "completed", or "failed"
        // depending on whether the background task has started processing
        assert!(
            status != "pending",
            "Status should have been updated from pending, got: {}",
            status
        );
    }
}
