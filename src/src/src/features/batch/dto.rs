//! # Batch Operation DTOs
//!
//! Data Transfer Objects for batch file and URL import operations.
//!
//! This module provides DTOs for creating and monitoring batch import jobs.
//!
//! ## Example
//!
//! ```rust,no_run
//! use lattice::application::dtos::batch_dto::{
//!     StartBatchFileImportRequestDto,
//!     GetBatchStatusRequestDto,
//! };
//!
//! let request = StartBatchFileImportRequestDto {
//!     file_paths: vec![
//!         "/path/to/file1.txt".to_string(),
//!         "/path/to/file2.pdf".to_string(),
//!     ],
//!
//! space_id: None,
//! };
//! ```

use serde::{Deserialize, Serialize};

/// Request to start a batch file import job.
///
/// Accepts 1-100 file paths for batch processing.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StartBatchFileImportRequestDto {
    /// List of file paths to import (1-100 files).
    pub file_paths: Vec<String>,
    #[serde(default)]
    pub indexing: Option<FileIndexingOptionsDto>,
    #[serde(default)]
    pub space_id: Option<String>,
}

/// Response after starting a batch file import job.
///
/// Contains the job ID for status tracking.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StartBatchFileImportResponseDto {
    /// Unique identifier for the batch job.
    pub job_id: String,
}

/// Request to get the status of a batch job.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GetBatchStatusRequestDto {
    /// Unique identifier of the batch job.
    pub job_id: String,
}

/// Response containing batch job status and progress.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GetBatchStatusResponseDto {
    /// Unique identifier for the batch job.
    pub job_id: String,
    /// Overall status of the batch job.
    pub status: BatchJobStatus,
    /// Progress breakdown by item status.
    pub progress: BatchProgressDto,
}

/// Overall status of a batch job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum BatchJobStatus {
    /// Job created but not yet started.
    Pending,
    /// Job is currently processing items.
    Running,
    /// Job completed successfully (may include some failures).
    Completed,
    /// Job failed (all items failed).
    Failed,
}

/// Progress information for a batch job.
///
/// Tracks the number of items in each state.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BatchProgressDto {
    /// Total number of items in the batch.
    pub total: usize,
    /// Number of items successfully completed.
    pub completed: usize,
    /// Number of items that failed.
    pub failed: usize,
    /// Number of items still pending.
    pub pending: usize,
}

/// Request to start a batch URL import job.
///
/// Creates a batch job that processes multiple URLs concurrently.
///
/// # Example
/// ```rust,no_run
/// use lattice::application::dtos::batch_dto::{
///     StartBatchUrlImportRequestDto, BatchImportOptionsDto
/// };
///
/// let request = StartBatchUrlImportRequestDto {
///     urls: vec![
///         "https://example.com/article1".to_string(),
///         "https://example.com/article2".to_string(),
///     ],
///     options: Some(BatchImportOptionsDto {
///         extract_article: Some(true),
///     }),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StartBatchUrlImportRequestDto {
    /// List of URLs to import (1-50 URLs)
    pub urls: Vec<String>,

    /// Optional import options
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<BatchImportOptionsDto>,
}

/// Options for batch import operations.
///
/// # Example
/// ```rust
/// use lattice::application::dtos::batch_dto::BatchImportOptionsDto;
///
/// let options = BatchImportOptionsDto {
///     extract_article: Some(true),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BatchImportOptionsDto {
    /// Whether to extract article content (default: true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extract_article: Option<bool>,
}

/// Response from starting a batch URL import.
///
/// Contains the job ID for tracking progress.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StartBatchUrlImportResponseDto {
    /// Unique batch job ID for tracking
    pub job_id: String,
}

/// Request to get batch job status.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GetBatchJobStatusRequestDto {
    /// The batch job ID to query
    pub job_id: String,
}

/// Full batch job status with all items.
///
/// Contains detailed status for the job and each item.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BatchJobStatusDto {
    /// Unique batch job ID
    pub job_id: String,

    /// Job type: "file_import" or "url_import"
    pub job_type: String,

    /// Job status: "pending", "running", "completed", "cancelled"
    pub status: String,

    /// Total number of items in the batch
    pub total_items: i64,

    /// Number of successfully completed items
    pub completed_items: i64,

    /// Number of failed items
    pub failed_items: i64,

    /// Job creation timestamp (ISO 8601 string)
    pub created_at: String,

    /// Job completion timestamp, when the job has finished.
    pub completed_at: Option<String>,

    /// List of all batch items with their status
    pub items: Vec<BatchJobItemDto>,
}

/// Status of a single batch job item.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BatchJobItemDto {
    /// Unique item ID
    pub item_id: String,

    /// Target URL or file path
    pub target: String,

    /// Item status: "pending", "running", "completed", "failed", "cancelled"
    pub status: String,

    /// Error message if status is "failed"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// Canonical document produced by this import, when available.
    pub document_id: Option<String>,
}

/// Request to cancel a batch job.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CancelBatchJobRequestDto {
    /// The batch job ID to cancel
    pub job_id: String,
}

/// Response from cancelling a batch job.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CancelBatchJobResponseDto {
    /// Number of items that were cancelled
    pub cancelled_count: usize,
}

/// Request to list batch jobs (paginated).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ListBatchJobsRequestDto {
    /// Maximum number of jobs to return (default: 50, max: 100)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,

    /// Number of jobs to skip (for pagination)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<i64>,
}

/// Response containing paginated list of batch jobs.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ListBatchJobsResponseDto {
    /// List of batch job summaries (newest first)
    pub jobs: Vec<BatchJobSummaryDto>,
}

/// Summary information for a batch job.
///
/// Lightweight representation for job history list.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BatchJobSummaryDto {
    /// Unique batch job ID
    pub job_id: String,

    /// Job type: "file_import" or "url_import"
    pub job_type: String,

    /// Job status: "pending", "running", "completed", "cancelled"
    pub status: String,

    /// Total number of items in the batch
    pub total_items: i64,

    /// Number of successfully completed items
    pub completed_items: i64,

    /// Number of failed items
    pub failed_items: i64,

    /// Job creation timestamp (ISO 8601 string)
    pub created_at: String,
}

/// Request to delete a batch job.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DeleteBatchJobRequestDto {
    /// The batch job ID to delete
    pub job_id: String,
}

/// Response from deleting a batch job.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DeleteBatchJobResponseDto {
    /// Whether the deletion was successful
    pub success: bool,
}

/// Request to retry failed items from a batch job.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RetryFailedItemsRequestDto {
    /// The batch job ID to retry failed items from
    pub job_id: String,
    #[serde(default)]
    pub item_id: Option<String>,
    #[serde(default)]
    pub replacement_path: Option<String>,
}

/// Response from retrying failed items.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RetryFailedItemsResponseDto {
    /// Job to monitor (file retries reuse the original job).
    pub new_job_id: String,

    /// Number of failed items being retried
    pub retried_count: usize,
}

/// Optional import context. File order comes from the request's file_paths.
#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FileIndexingOptionsDto {
    pub source_group: Option<crate::domain::value_objects::source_context::SourceGroup>,
    #[serde(default)]
    pub rebuild_existing: bool,
}
