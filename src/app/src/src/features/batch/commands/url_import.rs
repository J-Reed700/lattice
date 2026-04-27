use serde::{Deserialize, Serialize};
use tauri::State;

use crate::features::batch::dto::{
    BatchJobStatusDto, CancelBatchJobRequestDto, GetBatchJobStatusRequestDto,
    StartBatchUrlImportRequestDto,
};
use crate::infrastructure::audit::{get_audit_logger, AuditAction, AuditEvent, AuditResult};
use crate::interfaces::di::Container;
use crate::shared::error::AppError;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartBatchImportRequest {
    pub urls: Vec<String>,
    pub options: Option<BatchImportOptions>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchImportOptions {
    pub extract_article: Option<bool>,
}

/// Starts a batch URL import job for processing multiple web pages
///
/// Creates a background batch job to import and index content from multiple URLs.
/// Returns a `job_id` immediately for async progress tracking while URLs are fetched,
/// processed, and indexed in the background. Useful for importing large lists of articles,
/// documentation, or web content into Lattice's searchable index.
///
/// # Arguments
///
/// * `request` - Batch import request with URLs and options
///   - `urls`: Array of URLs to import (HTTP/HTTPS)
///   - `options`: Optional batch import configuration
///
/// # Returns
///
/// * `Ok(String)` - Job ID for tracking progress (e.g., `"job_abc123"`)
/// * `Err(AppError)` - If rate limited or job creation fails
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many batch import requests (rate limited)
/// * `AppError::Other` - Failed to create batch job or validate URLs
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface StartBatchImportRequest {
///   urls: string[];
///   options?: {
///     extractArticle?: boolean;
///   };
/// }
///
/// // Start batch import of URLs
/// const jobId = await invoke<string>('start_batch_url_import', {
///   request: {
///     urls: [
///       'https://example.com/article-1',
///       'https://example.com/article-2',
///       'https://example.com/article-3'
///     ],
///     options: {
///       extractArticle: true  // Extract main article content only
///     }
///   }
/// });
///
/// console.log(`Batch job started: ${jobId}`);
///
/// // Poll for progress (see get_batch_job_status)
/// const interval = setInterval(async () => {
///   const status = await invoke('get_batch_job_status', { jobId });
///   console.log(`Progress: ${status.completed}/${status.total}`);
///
///   if (status.completed === status.total) {
///     clearInterval(interval);
///     console.log('Batch import complete!');
///   }
/// }, 1000);
/// ```
///
/// # Batch Import Options
///
/// **`extractArticle` (optional)**:
/// - `true`: Extract main article content using readability algorithm (cleaner, faster)
/// - `false`: Index entire page HTML (includes navigation, ads, boilerplate)
/// - **Default**: `false`
/// - **Recommended**: `true` for articles, `false` for documentation sites
///
/// # Background Processing
///
/// The batch job runs asynchronously in the background:
/// 1. **Job Creation**: Job created immediately, returns `job_id`
/// 2. **URL Fetching**: URLs fetched one at a time (respects rate limits)
/// 3. **Content Extraction**: HTML parsed, article content extracted (if enabled)
/// 4. **Indexing**: Content chunked and indexed with embeddings
/// 5. **Progress Updates**: Status updated per URL for polling
///
/// **Queue Behavior**:
/// - URLs processed sequentially (not in parallel)
/// - Failed URLs skipped, don't block remaining URLs
/// - Progress tracked: pending → processing → completed/failed
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Web ingest rate limiter prevents DoS attacks
/// - **Audit Logging (CWE-778)**: All batch jobs logged with URL count
/// - **URL Validation**: HTTP/HTTPS URLs only, malicious URLs rejected
/// - **Content Safety**: Downloaded content sanitized before indexing
///
/// # Use Cases
///
/// - **Documentation Import**: Import entire doc site for offline search
/// - **Article Collection**: Save reading list for semantic search
/// - **Research**: Import papers, blog posts, tutorials in bulk
/// - **Content Archive**: Create searchable archive of web content
/// - **Knowledge Base**: Build internal knowledge base from web resources
///
/// # Performance
///
/// - **URL Fetching**: ~1-3 seconds per URL (network dependent)
/// - **Indexing**: ~0.5-2 seconds per URL (embedding generation)
/// - **Total Time**: ~2-5 seconds per URL average
/// - **Large Batches**: 100 URLs take ~3-8 minutes
/// - **Progress Updates**: Poll every 1-2 seconds for UI updates
///
/// # Limitations
///
/// - **Max URLs**: No hard limit, but large batches (1000+) take time
/// - **Timeout**: Individual URL fetch timeout ~30 seconds
/// - **Failed URLs**: Skipped silently, check status for errors
/// - **Content Types**: HTML pages only (no PDFs, images, videos)
///
/// # Architecture
///
/// Thin controller following Phase 4 Command Migration pattern:
/// 1. Rate limiting via SecurityContext (CWE-770)
/// 2. Batch job creation via StartBatchUrlImportUseCase
/// 3. Background task spawned for URL processing
/// 4. Audit logging via AuditLogger (CWE-778)
///
/// # Command Flow
///
/// 1. Rate limiting check (web ingest limiter)
/// 2. Validate request (URL array, options)
/// 3. Serialize options to JSON
/// 4. Create batch job (returns job_id immediately)
/// 5. Spawn background task for URL processing
/// 6. Log audit event (success/failure with URL count)
/// 7. Return job_id for progress tracking
///
/// # Related Commands
///
/// - `get_batch_job_status(job_id)`: Poll for progress updates
/// - `cancel_batch_job(job_id)`: Cancel pending URLs
pub async fn start_batch_url_import(
    request: StartBatchImportRequest,
    container: State<'_, Container>,
) -> Result<String, AppError> {
    // Ensure embedding model is available before starting batch ingestion.
    match container.get_or_load_embedding().await {
        Ok(embedding) => match embedding.is_ready().await {
            Ok(true) => {
                tracing::info!("Embedding model ready for batch URL import");
            }
            Ok(false) => {
                return Err(AppError::AiModelsNotInstalled(
                        "AI models are not installed. This feature requires embedding models. \
                         Please download models from Settings → Models to enable this functionality."
                            .to_string(),
                    ));
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to verify embedding readiness before batch URL import: {}",
                    e
                );
            }
        },
        Err(e) => return Err(e),
    }

    // Rate limiting
    container
        .security_context()
        .rate_limiters()
        .web_ingest
        .check_rate_limit("batch_url_import")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    let use_case = container.start_batch_url_import_use_case();

    let url_count = request.urls.len();

    // Start batch (returns job_id immediately, processing happens in background)
    let use_case_request = StartBatchUrlImportRequestDto {
        urls: request.urls.clone(),
        options: request.options.map(|opts| {
            crate::features::batch::dto::BatchImportOptionsDto {
                extract_article: opts.extract_article,
            }
        }),
    };
    let result = use_case
        .execute(use_case_request)
        .await
        .map(|response| response.job_id);

    // Audit logging (CWE-778 mitigation)
    let audit_logger = get_audit_logger();
    match &result {
        Ok(job_id) => {
            let event = AuditEvent::new(AuditAction::DataImported, AuditResult::success())
                .with_resource_id(format!("batch_url_job:{}", job_id))
                .with_metadata("url_count", url_count.to_string())
                .with_metadata("job_id", job_id)
                .with_metadata("operation", "start_batch_url_import");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
        Err(e) => {
            let event = AuditEvent::new(
                AuditAction::DataImported,
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id("batch_url_job:failed")
            .with_metadata("url_count", url_count.to_string())
            .with_metadata("operation", "start_batch_url_import")
            .with_metadata("error", e.to_string());

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
    }

    result
}

/// Gets the current status and progress of a batch URL import job
///
/// Polls the batch job for progress updates, returning the current state, completion count,
/// and any errors encountered. Frontend applications call this repeatedly (every 1-2 seconds)
/// to display progress bars and completion status to users. Returns detailed statistics about
/// the batch job including total URLs, completed URLs, failed URLs, and current processing state.
///
/// # Arguments
///
/// * `job_id` - Unique batch job identifier returned from `start_batch_url_import`
///
/// # Returns
///
/// * `Ok(BatchJobStatus)` - Current job status with progress details
/// * `Err(AppError)` - If job_id not found or status fetch fails
///
/// # Errors
///
/// * `AppError::NotFound` - Job ID doesn't exist (invalid or expired)
/// * `AppError::Other` - Failed to fetch job status from database
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface BatchJobStatus {
///   jobId: string;
///   status: 'pending' | 'processing' | 'completed' | 'failed' | 'cancelled';
///   total: number;           // Total URLs in batch
///   completed: number;       // Successfully processed URLs
///   failed: number;          // Failed URLs (errors)
///   cancelled: number;       // Cancelled URLs
///   currentUrl?: string;     // URL currently being processed
///   error?: string;          // Error message if job failed
///   createdAt: string;       // ISO 8601 timestamp
///   updatedAt: string;       // ISO 8601 timestamp
/// }
///
/// // Poll for progress updates
/// const jobId = 'job_abc123';
/// const interval = setInterval(async () => {
///   const status = await invoke<BatchJobStatus>('get_batch_job_status', {
///     jobId: jobId
///   });
///
///   console.log(`Progress: ${status.completed}/${status.total}`);
///   console.log(`Status: ${status.status}`);
///
///   if (status.failed > 0) {
///     console.log(`Failed URLs: ${status.failed}`);
///   }
///
///   // Stop polling when complete
///   if (status.status === 'completed' || status.status === 'failed') {
///     clearInterval(interval);
///     console.log('Batch import finished!');
///   }
/// }, 1000); // Poll every 1 second
/// ```
///
/// # BatchJobStatus Fields
///
/// **`status`**: Job state
/// - `"pending"`: Job created, not started yet
/// - `"processing"`: URLs being processed
/// - `"completed"`: All URLs processed successfully
/// - `"failed"`: Job failed (see `error` field)
/// - `"cancelled"`: Job cancelled by user
///
/// **`total`**: Total number of URLs in batch
///
/// **`completed`**: Number of URLs successfully processed and indexed
///
/// **`failed`**: Number of URLs that failed (network error, timeout, invalid content)
///
/// **`cancelled`**: Number of URLs cancelled before processing
///
/// **`currentUrl` (optional)**: URL currently being fetched/indexed (helpful for UI)
///
/// **`error` (optional)**: Error message if job failed
///
/// **`createdAt`**: ISO 8601 timestamp of job creation
///
/// **`updatedAt`**: ISO 8601 timestamp of last status update
///
/// # Polling Best Practices
///
/// **Polling Interval**:
/// - **Recommended**: 1-2 seconds for responsive UI
/// - **Minimum**: 500ms (avoid overwhelming server)
/// - **Maximum**: 5 seconds (updates feel sluggish)
///
/// **Stop Conditions**:
/// - Stop when `status === "completed"`
/// - Stop when `status === "failed"`
/// - Stop when `status === "cancelled"`
/// - Stop when `completed + failed + cancelled === total`
///
/// **Progress Calculation**:
/// ```typescript
/// const progress = (status.completed / status.total) * 100;
/// const progressBar = `${Math.round(progress)}%`;
/// ```
///
/// # Security
///
/// - **No Rate Limiting**: Read-only operation, minimal resource usage
/// - **No Audit Logging**: Progress checks not security-sensitive
/// - **Job Isolation**: Users can only query their own jobs
///
/// # Use Cases
///
/// - **Progress Bar**: Display real-time progress in UI
/// - **Status Display**: Show "Processing 15/100 URLs..."
/// - **Error Reporting**: Show failed URL count and error messages
/// - **Completion Detection**: Trigger UI updates when job completes
///
/// # Performance
///
/// - **Query Speed**: ~1-5ms (database lookup)
/// - **Polling Overhead**: Negligible with 1-2 second intervals
/// - **Concurrent Polls**: Safe for multiple browser tabs/windows
///
/// # Architecture
///
/// Thin controller following Phase 4 Command Migration pattern:
/// 1. Fetch job status via GetBatchJobStatusUseCase
/// 2. Return serialized BatchJobStatus
///
/// # Command Flow
///
/// 1. Receive job_id from frontend
/// 2. Query batch job table by job_id
/// 3. Return status with progress counts
///
/// # Related Commands
///
/// - `start_batch_url_import(request)`: Create batch job and get job_id
/// - `cancel_batch_job(job_id)`: Cancel pending URLs in batch
pub async fn get_batch_job_status(
    job_id: String,
    container: State<'_, Container>,
) -> Result<BatchJobStatusDto, AppError> {
    let use_case = container.get_batch_job_status_use_case();
    use_case
        .execute(GetBatchJobStatusRequestDto { job_id })
        .await
}

/// Cancels a batch URL import job and stops processing pending URLs
///
/// Cancels all pending (not yet processed) URLs in the batch job and marks the job as
/// cancelled. URLs already completed remain in the index, but remaining URLs are not
/// fetched or indexed. Returns the count of URLs that were cancelled. Useful when users
/// want to stop a long-running batch import early or fix URL list errors.
///
/// # Arguments
///
/// * `job_id` - Unique batch job identifier to cancel
///
/// # Returns
///
/// * `Ok(usize)` - Number of pending URLs cancelled
/// * `Err(AppError)` - If job_id not found or cancellation fails
///
/// # Errors
///
/// * `AppError::NotFound` - Job ID doesn't exist (invalid or expired)
/// * `AppError::Other` - Failed to cancel job (database error)
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Start batch import
/// const jobId = await invoke<string>('start_batch_url_import', {
///   request: {
///     urls: [...] // 100 URLs
///   }
/// });
///
/// // User clicks "Cancel" button after 10 seconds
/// const cancelledCount = await invoke<number>('cancel_batch_job', {
///   jobId: jobId
/// });
///
/// console.log(`Cancelled ${cancelledCount} pending URLs`);
///
/// // Check final status
/// const status = await invoke('get_batch_job_status', { jobId });
/// console.log(`Completed: ${status.completed}, Cancelled: ${status.cancelled}`);
/// ```
///
/// # Cancellation Behavior
///
/// **What Gets Cancelled**:
/// - URLs with status `"pending"` (not started yet)
/// - URLs with status `"queued"` (waiting in queue)
///
/// **What Doesn't Get Cancelled**:
/// - URLs already completed successfully (`"completed"`)
/// - URLs already failed (`"failed"`)
/// - URL currently being processed (`"processing"`) - waits for completion
///
/// **State Transitions**:
/// - `pending` → `cancelled`
/// - `queued` → `cancelled`
/// - `processing` → `completed` or `failed` (finishes current URL)
/// - Job status: `processing` → `cancelled`
///
/// # Security
///
/// - **No Rate Limiting**: Cancellation is a stop operation (not resource-intensive)
/// - **Audit Logging (CWE-778)**: All cancellations logged with cancelled count
/// - **Job Isolation**: Users can only cancel their own jobs
///
/// # Use Cases
///
/// - **User Cancellation**: User clicks "Stop" button in UI
/// - **Error Recovery**: Cancel batch with wrong URLs, fix, restart
/// - **Resource Management**: Stop batch to free up system resources
/// - **Accidental Import**: Cancel unintended batch immediately
///
/// # Return Value
///
/// **Cancelled Count**:
/// - Number of URLs moved from `pending`/`queued` to `cancelled`
/// - Does NOT include already completed or failed URLs
/// - Does NOT include currently processing URL
///
/// **Example Scenarios**:
/// - **Batch**: 100 URLs, **Status**: 30 completed, 70 pending → **Returns**: 70
/// - **Batch**: 50 URLs, **Status**: 50 completed → **Returns**: 0 (all done)
/// - **Batch**: 20 URLs, **Status**: 5 completed, 1 processing, 14 pending → **Returns**: 14
///
/// # Performance
///
/// - **Cancellation Speed**: ~5-50ms (depends on pending count)
/// - **Database Update**: Bulk update of pending URLs to cancelled
/// - **Job Stop**: Immediate (background task checks cancellation flag)
///
/// # Post-Cancellation
///
/// **Job Status After Cancel**:
/// ```typescript
/// {
///   jobId: "job_abc123",
///   status: "cancelled",
///   total: 100,
///   completed: 25,
///   failed: 5,
///   cancelled: 70
/// }
/// ```
///
/// **Retained Data**:
/// - Successfully indexed URLs remain in search index
/// - Job record remains in database for history
/// - Audit logs preserved for compliance
///
/// **Restarting**:
/// - Cannot resume cancelled job
/// - Create new batch job with remaining URLs if needed
///
/// # Architecture
///
/// Thin controller following Phase 4 Command Migration pattern:
/// 1. Cancel batch job via CancelBatchJobUseCase
/// 2. Audit logging via AuditLogger (CWE-778)
///
/// # Command Flow
///
/// 1. Receive job_id from frontend
/// 2. Find batch job by job_id
/// 3. Update all pending URLs to cancelled status
/// 4. Mark job status as cancelled
/// 5. Signal background task to stop
/// 6. Log audit event (success/failure with cancelled count)
/// 7. Return cancelled count
///
/// # Related Commands
///
/// - `start_batch_url_import(request)`: Create batch job
/// - `get_batch_job_status(job_id)`: Check cancellation status
pub async fn cancel_batch_job(
    job_id: String,
    container: State<'_, Container>,
) -> Result<usize, AppError> {
    let use_case = container.cancel_batch_job_use_case();
    let job_id_clone = job_id.clone();
    let result = use_case
        .execute(CancelBatchJobRequestDto {
            job_id: job_id_clone,
        })
        .await
        .map(|response| response.cancelled_count);

    // Audit logging (CWE-778 mitigation)
    let audit_logger = get_audit_logger();
    match &result {
        Ok(cancelled_count) => {
            let event = AuditEvent::new(
                AuditAction::Custom("Batch job cancelled".to_string()),
                AuditResult::success(),
            )
            .with_resource_id(format!("batch_url_job:{}", job_id))
            .with_metadata("job_id", &job_id)
            .with_metadata("cancelled_count", cancelled_count.to_string())
            .with_metadata("operation", "cancel_batch_job");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
        Err(e) => {
            let event = AuditEvent::new(
                AuditAction::Custom("Batch job cancelled".to_string()),
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id(format!("batch_url_job:{}", job_id))
            .with_metadata("job_id", &job_id)
            .with_metadata("operation", "cancel_batch_job")
            .with_metadata("error", e.to_string());

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
    }

    result
}
