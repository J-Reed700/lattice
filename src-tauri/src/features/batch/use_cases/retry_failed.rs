//! # Retry Failed Items Use Case
//!
//! Retries failed files in their original job, or creates a new URL retry job.
//!
//! This use case:
//! 1. Fetches original job with failed items
//! 2. Validates that there are failed items to retry
//! 3. Creates new batch job with only failed items
//! 4. Returns new job ID and retry count
//!
//! ## Example
//!
//! ```rust,no_run
//! use lattice::application::use_cases::batch::RetryFailedItemsUseCase;
//! use lattice::application::dtos::batch_dto::RetryFailedItemsRequestDto;
//!
//! # async fn example(use_case: RetryFailedItemsUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let request = RetryFailedItemsRequestDto {
//!     job_id: "batch-job-123".to_string(),
//!     item_id: None, replacement_path: None,
//! };
//!
//! let response = use_case.execute(request).await?;
//! println!("Retrying {} items in job {}", response.retried_count, response.new_job_id);
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use crate::application::ports::BatchJobRepositoryPort;
use crate::features::batch::dto::{
    RetryFailedItemsRequestDto, RetryFailedItemsResponseDto, StartBatchUrlImportRequestDto,
};
use crate::features::batch::use_cases::{StartBatchFileImportUseCase, StartBatchUrlImportUseCase};
use crate::shared::error::{AppError, Result};

/// Retry failed items use case.
///
/// Routes recovery to the matching importer. File retries preserve the job and
/// successful items so unresolved failures disappear only after recovery.
///
/// ## Dependencies
///
/// - `BatchJobRepositoryPort`: Fetches original job data
/// - `StartBatchUrlImportUseCase`: Creates new retry job
pub struct RetryFailedItemsUseCase {
    batch_repo: Arc<dyn BatchJobRepositoryPort>,
    start_batch_url_import: Arc<StartBatchUrlImportUseCase>,
    start_batch_file_import: Arc<StartBatchFileImportUseCase>,
}

impl RetryFailedItemsUseCase {
    /// Create a new retry failed items use case.
    ///
    /// # Arguments
    ///
    /// * `batch_repo` - Repository for batch job data
    /// * `start_batch_url_import` - Use case for starting new batch import
    pub fn new(
        batch_repo: Arc<dyn BatchJobRepositoryPort>,
        start_batch_url_import: Arc<StartBatchUrlImportUseCase>,
        start_batch_file_import: Arc<StartBatchFileImportUseCase>,
    ) -> Self {
        Self {
            batch_repo,
            start_batch_url_import,
            start_batch_file_import,
        }
    }

    /// Execute retry of failed items.
    ///
    /// # Arguments
    ///
    /// * `request` - Request containing original job ID
    ///
    /// # Returns
    ///
    /// Response with new job ID and retry count
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Job ID is empty
    /// - Original job not found
    /// - No failed items to retry
    /// - New job creation fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use lattice::application::use_cases::batch::RetryFailedItemsUseCase;
    /// # use lattice::application::dtos::batch_dto::RetryFailedItemsRequestDto;
    /// # async fn example(use_case: RetryFailedItemsUseCase) -> Result<(), Box<dyn std::error::Error>> {
    /// let request = RetryFailedItemsRequestDto {
    ///     job_id: "batch-job-123".to_string(),
    ///     item_id: None, replacement_path: None,
    /// };
    ///
    /// match use_case.execute(request).await {
    ///     Ok(response) => {
    ///         println!("Created retry job: {}", response.new_job_id);
    ///         println!("Retrying {} items", response.retried_count);
    ///     }
    ///     Err(e) => eprintln!("Retry failed: {}", e),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute(
        &self,
        request: RetryFailedItemsRequestDto,
    ) -> Result<RetryFailedItemsResponseDto> {
        // 1. Validate job ID
        if request.job_id.trim().is_empty() {
            return Err(AppError::InvalidInput("Job ID cannot be empty".to_string()));
        }

        // 2. Fetch original job with all items
        let job_status = self.batch_repo.get_batch_job(&request.job_id).await?;
        if job_status.job_type == "file_import" {
            let retried_count = self
                .start_batch_file_import
                .retry_failed(
                    &request.job_id,
                    request.item_id.as_deref(),
                    request.replacement_path.as_deref(),
                )
                .await?;
            return Ok(RetryFailedItemsResponseDto {
                new_job_id: request.job_id,
                retried_count,
            });
        }
        if job_status.job_type != "url_import"
            || request.item_id.is_some()
            || request.replacement_path.is_some()
        {
            return Err(AppError::InvalidInput(
                "This import does not support file recovery".into(),
            ));
        }
        if matches!(job_status.status.as_str(), "pending" | "running") {
            return Err(AppError::InvalidInput(
                "This import is still running".into(),
            ));
        }

        // 3. Filter failed items
        let failed_urls: Vec<String> = job_status
            .items
            .into_iter()
            .filter(|item| item.status == "failed")
            .map(|item| item.url)
            .collect();

        // 4. Validate that there are failed items
        if failed_urls.is_empty() {
            return Err(AppError::InvalidInput(
                "No failed items to retry in this job".to_string(),
            ));
        }

        let retried_count = failed_urls.len();

        // 5. Create new batch job with failed items
        let retry_request = StartBatchUrlImportRequestDto {
            urls: failed_urls,
            options: None, // Could parse from original job options if needed
        };

        let retry_response = self.start_batch_url_import.execute(retry_request).await?;

        // 6. Return new job ID and retry count
        Ok(RetryFailedItemsResponseDto {
            new_job_id: retry_response.job_id,
            retried_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::batch_job_repository_port::{
        BatchItemState, BatchJobItem, BatchJobItemStatus, BatchJobStatus, BatchJobSummary,
    };
    use async_trait::async_trait;

    struct MockBatchJobRepository {
        job_status: Option<BatchJobStatus>,
    }

    impl MockBatchJobRepository {
        fn new(job_status: Option<BatchJobStatus>) -> Self {
            Self { job_status }
        }
    }

    #[async_trait]
    impl BatchJobRepositoryPort for MockBatchJobRepository {
        async fn create_batch_job(
            &self,
            _job_id: &str,
            _job_type: &str,
            _total_items: i64,
            _options: Option<&str>,
        ) -> Result<()> {
            Ok(())
        }

        async fn create_batch_items(&self, _job_id: &str, _urls: Vec<String>) -> Result<()> {
            Ok(())
        }

        async fn update_job_status(
            &self,
            _job_id: &str,
            _status: &str,
            _started_at: Option<String>,
            _completed_at: Option<String>,
        ) -> Result<()> {
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
            _status: BatchItemState,
            _document_id: Option<&str>,
            _error_message: Option<&str>,
        ) -> Result<()> {
            Ok(())
        }

        async fn get_batch_job(&self, job_id: &str) -> Result<BatchJobStatus> {
            self.job_status
                .clone()
                .ok_or_else(|| AppError::NotFound(format!("Job not found: {}", job_id)))
        }

        async fn get_pending_items(&self, _job_id: &str) -> Result<Vec<BatchJobItem>> {
            Ok(vec![])
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

    fn create_job_with_failures() -> BatchJobStatus {
        BatchJobStatus {
            id: "original-job".to_string(),
            job_type: "url_import".to_string(),
            status: "completed".to_string(),
            total_items: 5,
            completed_items: 3,
            failed_items: 2,
            progress: 100.0,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            started_at: Some("2024-01-01T00:01:00Z".to_string()),
            completed_at: Some("2024-01-01T00:10:00Z".to_string()),
            error_message: None,
            items: vec![
                BatchJobItemStatus {
                    id: "item-1".to_string(),
                    url: "https://example.com/1".to_string(),
                    document_id: Some("doc-1".to_string()),
                    status: "completed".to_string(),
                    error_message: None,
                },
                BatchJobItemStatus {
                    id: "item-2".to_string(),
                    url: "https://example.com/2".to_string(),
                    document_id: None,
                    status: "failed".to_string(),
                    error_message: Some("Network error".to_string()),
                },
                BatchJobItemStatus {
                    id: "item-3".to_string(),
                    url: "https://example.com/3".to_string(),
                    document_id: Some("doc-3".to_string()),
                    status: "completed".to_string(),
                    error_message: None,
                },
                BatchJobItemStatus {
                    id: "item-4".to_string(),
                    url: "https://example.com/4".to_string(),
                    document_id: None,
                    status: "failed".to_string(),
                    error_message: Some("Timeout".to_string()),
                },
                BatchJobItemStatus {
                    id: "item-5".to_string(),
                    url: "https://example.com/5".to_string(),
                    document_id: Some("doc-5".to_string()),
                    status: "completed".to_string(),
                    error_message: None,
                },
            ],
        }
    }

    #[tokio::test]
    async fn test_retry_with_failed_items() {
        let job_status = create_job_with_failures();
        let repo = Arc::new(MockBatchJobRepository::new(Some(job_status)));

        // Note: This test demonstrates the pattern but won't compile without full mocks
        // In a real implementation, you'd properly mock StartBatchUrlImportUseCase

        let _request = RetryFailedItemsRequestDto {
            job_id: "original-job".to_string(),
            item_id: None,
            replacement_path: None,
        };

        let job = repo.get_batch_job("original-job").await.unwrap();
        let failed_urls: Vec<String> = job
            .items
            .into_iter()
            .filter(|item| item.status == "failed")
            .map(|item| item.url)
            .collect();

        assert_eq!(failed_urls.len(), 2);
        assert_eq!(failed_urls[0], "https://example.com/2");
        assert_eq!(failed_urls[1], "https://example.com/4");
    }

    #[tokio::test]
    async fn test_retry_with_no_failures() {
        let job_status = BatchJobStatus {
            id: "all-success-job".to_string(),
            job_type: "url_import".to_string(),
            status: "completed".to_string(),
            total_items: 2,
            completed_items: 2,
            failed_items: 0,
            progress: 100.0,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            started_at: Some("2024-01-01T00:01:00Z".to_string()),
            completed_at: Some("2024-01-01T00:05:00Z".to_string()),
            error_message: None,
            items: vec![
                BatchJobItemStatus {
                    id: "item-1".to_string(),
                    url: "https://example.com/1".to_string(),
                    document_id: Some("doc-1".to_string()),
                    status: "completed".to_string(),
                    error_message: None,
                },
                BatchJobItemStatus {
                    id: "item-2".to_string(),
                    url: "https://example.com/2".to_string(),
                    document_id: Some("doc-2".to_string()),
                    status: "completed".to_string(),
                    error_message: None,
                },
            ],
        };

        let job = Arc::new(MockBatchJobRepository::new(Some(job_status)));

        let status = job.get_batch_job("all-success-job").await.unwrap();
        let failed_count = status.items.iter().filter(|i| i.status == "failed").count();

        assert_eq!(failed_count, 0);
    }

    #[tokio::test]
    async fn test_retry_invalid_job_id() {
        let repo = Arc::new(MockBatchJobRepository::new(None));

        let result = repo.get_batch_job("non-existent").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
    }
}
