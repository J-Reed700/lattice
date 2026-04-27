//! # Get Batch Job Status Use Case
//!
//! Retrieves detailed status of a batch job including all items.
//!
//! This use case:
//! 1. Validates job ID
//! 2. Fetches job from repository
//! 3. Transforms to DTO format
//! 4. Returns full job details with items
//!
//! ## Example
//!
//! ```rust,no_run
//! use vault_desktop::application::use_cases::batch::GetBatchJobStatusUseCase;
//! use vault_desktop::application::dtos::batch_dto::GetBatchJobStatusRequestDto;
//!
//! # async fn example(use_case: GetBatchJobStatusUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let request = GetBatchJobStatusRequestDto {
//!     job_id: "batch-job-123".to_string(),
//! };
//!
//! let status = use_case.execute(request).await?;
//! println!("Job status: {}", status.status);
//! println!("Completed: {}/{}", status.completed_items, status.total_items);
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use chrono::{DateTime, NaiveDateTime, Utc};

use crate::features::batch::dto::{
    BatchJobItemDto, BatchJobStatusDto, GetBatchJobStatusRequestDto,
};
use crate::application::ports::BatchJobRepositoryPort;
use crate::shared::error::{AppError, Result};

fn parse_batch_timestamp(timestamp: &str) -> Result<DateTime<Utc>> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(timestamp) {
        return Ok(parsed.with_timezone(&Utc));
    }

    for format in ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S%.f"] {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(timestamp, format) {
            return Ok(parsed.and_utc());
        }
    }

    Err(AppError::InvalidData(format!(
        "Invalid timestamp: {}",
        timestamp
    )))
}

/// Get batch job status use case.
///
/// Retrieves detailed information about a batch job.
///
/// ## Dependencies
///
/// - `BatchJobRepositoryPort`: Fetches batch job data
pub struct GetBatchJobStatusUseCase {
    batch_repo: Arc<dyn BatchJobRepositoryPort>,
}

impl GetBatchJobStatusUseCase {
    /// Create a new get batch job status use case.
    ///
    /// # Arguments
    ///
    /// * `batch_repo` - Repository for batch job data
    pub fn new(batch_repo: Arc<dyn BatchJobRepositoryPort>) -> Self {
        Self { batch_repo }
    }

    /// Execute batch job status retrieval.
    ///
    /// # Arguments
    ///
    /// * `request` - Request containing job ID to query
    ///
    /// # Returns
    ///
    /// Full batch job status with all items
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Job ID is empty
    /// - Job not found
    /// - Repository error
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use vault_desktop::application::use_cases::batch::GetBatchJobStatusUseCase;
    /// # use vault_desktop::application::dtos::batch_dto::GetBatchJobStatusRequestDto;
    /// # async fn example(use_case: GetBatchJobStatusUseCase) -> Result<(), Box<dyn std::error::Error>> {
    /// let request = GetBatchJobStatusRequestDto {
    ///     job_id: "batch-job-123".to_string(),
    /// };
    ///
    /// let status = use_case.execute(request).await?;
    /// for item in &status.items {
    ///     println!("Item {}: {}", item.target, item.status);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute(&self, request: GetBatchJobStatusRequestDto) -> Result<BatchJobStatusDto> {
        // 1. Validate job ID
        if request.job_id.trim().is_empty() {
            return Err(AppError::InvalidInput("Job ID cannot be empty".to_string()));
        }

        // 2. Fetch job from repository
        let job_status = self.batch_repo.get_batch_job(&request.job_id).await?;

        // 3. Parse created_at timestamp
        let created_at = parse_batch_timestamp(&job_status.created_at)?;

        // 4. Transform items to DTOs
        let items: Vec<BatchJobItemDto> = job_status
            .items
            .into_iter()
            .map(|item| BatchJobItemDto {
                item_id: item.id,
                target: item.url,
                status: item.status,
                error_message: item.error_message,
            })
            .collect();

        // 5. Return full job status
        Ok(BatchJobStatusDto {
            job_id: job_status.id,
            job_type: job_status.job_type,
            status: job_status.status,
            total_items: job_status.total_items,
            completed_items: job_status.completed_items,
            failed_items: job_status.failed_items,
            created_at: created_at.to_rfc3339(),
            items,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::batch_job_repository_port::{
        BatchJobItem, BatchJobItemStatus, BatchJobStatus, BatchJobSummary,
    };
    use async_trait::async_trait;

    struct MockBatchJobRepository {
        should_return_job: bool,
        created_at: String,
    }

    impl MockBatchJobRepository {
        fn new(should_return_job: bool) -> Self {
            Self {
                should_return_job,
                created_at: "2024-01-01T00:00:00Z".to_string(),
            }
        }

        fn new_with_created_at(should_return_job: bool, created_at: &str) -> Self {
            Self {
                should_return_job,
                created_at: created_at.to_string(),
            }
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
            _status: &str,
            _document_id: Option<&str>,
            _error_message: Option<&str>,
        ) -> Result<()> {
            Ok(())
        }

        async fn get_batch_job(&self, job_id: &str) -> Result<BatchJobStatus> {
            if !self.should_return_job {
                return Err(AppError::NotFound(format!("Job not found: {}", job_id)));
            }

            Ok(BatchJobStatus {
                id: job_id.to_string(),
                job_type: "url_import".to_string(),
                status: "running".to_string(),
                total_items: 3,
                completed_items: 1,
                failed_items: 1,
                progress: 66.67,
                created_at: self.created_at.clone(),
                started_at: Some("2024-01-01T00:01:00Z".to_string()),
                completed_at: None,
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
                        document_id: None,
                        status: "pending".to_string(),
                        error_message: None,
                    },
                ],
            })
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

    #[tokio::test]
    async fn test_get_job_status_success() {
        let repo = Arc::new(MockBatchJobRepository::new(true));
        let use_case = GetBatchJobStatusUseCase::new(repo);

        let request = GetBatchJobStatusRequestDto {
            job_id: "test-job-123".to_string(),
        };

        let result = use_case.execute(request).await;
        assert!(result.is_ok());

        let status = result.unwrap();
        assert_eq!(status.job_id, "test-job-123");
        assert_eq!(status.status, "running");
        assert_eq!(status.total_items, 3);
        assert_eq!(status.completed_items, 1);
        assert_eq!(status.failed_items, 1);
        assert_eq!(status.items.len(), 3);
    }

    #[tokio::test]
    async fn test_get_job_status_not_found() {
        let repo = Arc::new(MockBatchJobRepository::new(false));
        let use_case = GetBatchJobStatusUseCase::new(repo);

        let request = GetBatchJobStatusRequestDto {
            job_id: "non-existent-job".to_string(),
        };

        let result = use_case.execute(request).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_get_job_status_empty_id() {
        let repo = Arc::new(MockBatchJobRepository::new(true));
        let use_case = GetBatchJobStatusUseCase::new(repo);

        let request = GetBatchJobStatusRequestDto {
            job_id: "".to_string(),
        };

        let result = use_case.execute(request).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_transforms_items_correctly() {
        let repo = Arc::new(MockBatchJobRepository::new(true));
        let use_case = GetBatchJobStatusUseCase::new(repo);

        let request = GetBatchJobStatusRequestDto {
            job_id: "test-job".to_string(),
        };

        let status = use_case.execute(request).await.unwrap();

        // Verify first item (completed)
        assert_eq!(status.items[0].item_id, "item-1");
        assert_eq!(status.items[0].target, "https://example.com/1");
        assert_eq!(status.items[0].status, "completed");
        assert!(status.items[0].error_message.is_none());

        // Verify second item (failed)
        assert_eq!(status.items[1].status, "failed");
        assert_eq!(
            status.items[1].error_message,
            Some("Network error".to_string())
        );

        // Verify third item (pending)
        assert_eq!(status.items[2].status, "pending");
    }

    #[tokio::test]
    async fn test_get_job_status_supports_sqlite_timestamp_format() {
        let repo = Arc::new(MockBatchJobRepository::new_with_created_at(
            true,
            "2026-02-04 12:34:56",
        ));
        let use_case = GetBatchJobStatusUseCase::new(repo);

        let request = GetBatchJobStatusRequestDto {
            job_id: "test-job".to_string(),
        };

        let status = use_case.execute(request).await.unwrap();
        assert_eq!(status.created_at, "2026-02-04T12:34:56+00:00");
    }
}
