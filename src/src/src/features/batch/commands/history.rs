//! Batch Job History Management Commands
//!
//! Thin command controllers for viewing and managing batch import job history following DDD
//! pattern. Provides read-only access to job history, job deletion, and retry functionality
//! for failed items.
//!
//! # Commands (3 total)
//!
//! - `list_batch_jobs` - List all batch jobs with pagination
//! - `delete_batch_job` - Delete a batch job and all its items
//! - `retry_failed_items` - Retry failed items by creating new job
//!
//! # Architecture
//!
//! Pure delegation pattern with no security layers (no rate limiting, no audit logging).
//! Commands delegate directly to `BatchJobRepositoryPort` for CRUD operations and
//! `BatchUrlImportService` for retry logic.
//!
//! # Batch Job Lifecycle
//!
//! 1. **Created**: Job created via `start_batch_url_import`
//! 2. **Processing**: Items processed asynchronously
//! 3. **Completed**: All items finished (success/failed)
//! 4. **History**: Job viewable via `list_batch_jobs`
//! 5. **Retry**: Failed items retried via `retry_failed_items`
//! 6. **Deleted**: Job removed via `delete_batch_job`
//!
//! # Use Cases
//!
//! - **History View**: Display past batch import jobs
//! - **Progress Tracking**: Monitor job completion status
//! - **Error Recovery**: Retry failed imports without re-processing successes
//! - **Cleanup**: Remove old job history

use crate::features::batch::dto::{
    DeleteBatchJobRequestDto, DeleteBatchJobResponseDto, ListBatchJobsRequestDto,
    ListBatchJobsResponseDto, RetryFailedItemsRequestDto, RetryFailedItemsResponseDto,
};
use crate::interfaces::di::Container;
use crate::shared::error::AppError;
use tauri::State;

/// List all batch jobs with pagination support
///
/// Returns batch job history ordered by creation date (newest first). Supports pagination
/// for efficient display of large job histories. Each job includes summary statistics
/// (total/success/failed counts) and metadata.
///
/// # Arguments
///
/// * `limit` - Max jobs to return (default: 20)
/// * `offset` - Skip N jobs (for pagination, default: 0)
/// * `container` - Service container with batch job repository
///
/// # Returns
///
/// * `Ok(ListBatchJobsResponseDto)` - Batch jobs with summary stats (empty if none)
/// * `Err(AppError)` - Query failed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface BatchJobSummary {
///   jobId: string;
///   jobType: 'file_import' | 'url_import';
///   status: 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';
///   totalItems: number;
///   completedItems: number;
///   failedItems: number;
///   createdAt: string;
/// }
///
/// // List recent jobs (first page)
/// const response = await invoke<ListBatchJobsResponseDto>('list_batch_jobs', {
///   limit: 20,
///   offset: 0
/// });
///
/// console.log(`Found ${response.jobs.length} batch jobs`);
///
/// // Paginated history view
/// const loadJobsPage = async (page: number, pageSize: number = 20) => {
///   return await invoke<ListBatchJobsResponseDto>('list_batch_jobs', {
///     limit: pageSize,
///     offset: page * pageSize
///   });
/// };
///
/// // Display job stats
/// response.jobs.forEach(job => {
///   console.log(`${job.jobId}: ${job.completedItems}/${job.totalItems} successful`);
/// });
/// ```
///
/// # Performance
///
/// - **Query Time**: ~5-20ms (depends on job count)
/// - **Pagination**: Efficient LIMIT/OFFSET query
///
/// # Architecture
///
/// Pure delegation to `ListBatchJobsUseCase`
pub async fn list_batch_jobs(
    limit: Option<i64>,
    offset: Option<i64>,
    container: State<'_, Container>,
) -> Result<ListBatchJobsResponseDto, AppError> {
    let use_case = container.list_batch_jobs_use_case();
    use_case
        .execute(ListBatchJobsRequestDto { limit, offset })
        .await
}

/// Delete a batch job with cascade deletion of all items
///
/// Permanently deletes a batch job and all its associated items (URLs and results).
/// Use for cleanup of old job history. Cannot be undone.
///
/// # Arguments
///
/// * `job_id` - ID of batch job to delete
/// * `container` - Service container with batch job repository
///
/// # Returns
///
/// * `Ok(DeleteBatchJobResponseDto)` - Deletion result (success flag)
/// * `Err(AppError)` - Deletion failed (job not found, database error)
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Delete specific job
/// const result = await invoke<DeleteBatchJobResponseDto>('delete_batch_job', { jobId: 'job-123' });
/// console.log('Job deleted:', result.success);
///
/// // Cleanup old jobs (older than 30 days)
/// const cleanupOldJobs = async () => {
///   const jobs = await invoke<BatchJobSummary[]>('list_batch_jobs', {
///     limit: 1000,
///     offset: 0
///   });
///
///   const thirtyDaysAgo = Date.now() - 30 * 24 * 60 * 60 * 1000;
///
///   for (const job of jobs) {
///     if (new Date(job.createdAt).getTime() < thirtyDaysAgo) {
///       await invoke('delete_batch_job', { jobId: job.id });
///     }
///   }
/// };
/// ```
///
/// # Warning
///
/// Deletion is permanent and cascades to all batch items. Job history is unrecoverable.
///
/// # Architecture
///
/// Pure delegation to `DeleteBatchJobUseCase`
pub async fn delete_batch_job(
    job_id: String,
    container: State<'_, Container>,
) -> Result<DeleteBatchJobResponseDto, AppError> {
    let use_case = container.delete_batch_job_use_case();
    use_case.execute(DeleteBatchJobRequestDto { job_id }).await
}

/// Retry failed items by creating new batch job
///
/// Extracts failed items from a completed job and creates a new batch import job to retry
/// them. Efficient error recovery without re-processing successful items. Returns new job
/// ID for progress tracking.
///
/// # Arguments
///
/// * `job_id` - ID of job with failed items
/// * `container` - Service container with repository and service
///
/// # Returns
///
/// * `Ok(RetryFailedItemsResponseDto)` - New job ID and retry count
/// * `Err(AppError::InvalidInput)` - No failed items to retry
/// * `Err(AppError)` - Retry failed (original job not found, service error)
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Retry failed items from job
/// const retry = await invoke<RetryFailedItemsResponseDto>('retry_failed_items', {
///   jobId: 'job-123'
/// });
///
/// console.log('Retry job created:', retry.newJobId);
///
/// // Monitor retry progress
/// const retryWithProgress = async (jobId: string) => {
///   const retry = await invoke<RetryFailedItemsResponseDto>('retry_failed_items', { jobId });
///
///   // Poll for completion
///   const interval = setInterval(async () => {
///     const job = await invoke<BatchJobSummary>('get_batch_job', {
///       jobId: retry.newJobId
///     });
///
///     if (job.status === 'completed') {
///       clearInterval(interval);
///       console.log('Retry completed!');
///     }
///   }, 1000);
/// };
///
/// // Automatic retry on failure
/// const importWithAutoRetry = async (urls: string[]) => {
///   const jobId = await invoke<string>('start_batch_url_import', { urls });
///   await waitForCompletion(jobId);
///
///   const job = await invoke<BatchJobSummary>('get_batch_job', { jobId });
///   if (job.failedCount > 0) {
///     console.log(`Retrying ${job.failedCount} failed items...`);
///     const retry = await invoke<RetryFailedItemsResponseDto>('retry_failed_items', { jobId });
///     await waitForCompletion(retry.newJobId);
///   }
/// };
/// ```
///
/// # Behavior
///
/// 1. Retrieve original batch job
/// 2. Filter items with status = "failed"
/// 3. Extract failed URLs
/// 4. Create new batch job with failed URLs only
/// 5. Return new job ID
///
/// # Error Handling
///
/// Returns `InvalidInput` error if no failed items exist (all succeeded).
///
/// # Architecture
///
/// Pure delegation to `RetryFailedItemsUseCase`
pub async fn retry_failed_items(
    job_id: String,
    container: State<'_, Container>,
) -> Result<RetryFailedItemsResponseDto, AppError> {
    let use_case = container.retry_failed_items_use_case();
    use_case
        .execute(RetryFailedItemsRequestDto { job_id })
        .await
}
