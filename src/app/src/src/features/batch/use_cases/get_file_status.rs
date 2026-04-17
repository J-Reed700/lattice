//! # Get Batch File Status Use Case
//!
//! Retrieves the status and progress of a batch file import job.
//!
//! This use case orchestrates:
//! 1. Job ID validation
//! 2. Batch job retrieval
//! 3. Progress calculation
//! 4. Status determination
//!
//! ## Example
//!
//! ```rust,no_run
//! use vault_desktop::application::use_cases::batch::GetBatchFileStatusUseCase;
//! use vault_desktop::application::dtos::batch_dto::GetBatchStatusRequestDto;
//!
//! # async fn example(use_case: GetBatchFileStatusUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let request = GetBatchStatusRequestDto {
//!     job_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
//! };
//!
//! let response = use_case.execute(request).await?;
//! println!("Status: {:?}", response.status);
//! println!("Progress: {}/{}", response.progress.completed, response.progress.total);
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use crate::features::batch::dto::{
    BatchJobStatus, BatchProgressDto, GetBatchStatusRequestDto, GetBatchStatusResponseDto,
};
use crate::application::ports::BatchJobRepositoryPort;
use crate::shared::error::{AppError, Result};

/// Get batch file status use case.
///
/// Retrieves batch job progress and status, including:
/// - Overall job status (Pending, Running, Completed, Failed)
/// - Progress breakdown (total, completed, failed, pending)
///
/// ## Dependencies
///
/// - `BatchJobRepositoryPort`: Retrieves batch job data
pub struct GetBatchFileStatusUseCase {
    batch_repo: Arc<dyn BatchJobRepositoryPort>,
}

impl GetBatchFileStatusUseCase {
    /// Create a new get batch file status use case.
    ///
    /// # Arguments
    ///
    /// * `batch_repo` - Repository for batch job data
    pub fn new(batch_repo: Arc<dyn BatchJobRepositoryPort>) -> Self {
        Self { batch_repo }
    }

    /// Execute batch status retrieval.
    ///
    /// # Arguments
    ///
    /// * `request` - Request containing job ID
    ///
    /// # Returns
    ///
    /// Response with job status and progress
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Job ID is empty
    /// - Job not found
    /// - Repository access fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use vault_desktop::application::use_cases::batch::GetBatchFileStatusUseCase;
    /// # use vault_desktop::application::dtos::batch_dto::GetBatchStatusRequestDto;
    /// # async fn example(use_case: GetBatchFileStatusUseCase) -> Result<(), Box<dyn std::error::Error>> {
    /// let request = GetBatchStatusRequestDto {
    ///     job_id: "job-123".to_string(),
    /// };
    ///
    /// let response = use_case.execute(request).await?;
    /// println!("Completed: {}/{}", response.progress.completed, response.progress.total);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute(
        &self,
        request: GetBatchStatusRequestDto,
    ) -> Result<GetBatchStatusResponseDto> {
        // 1. Validate job_id
        if request.job_id.trim().is_empty() {
            return Err(AppError::InvalidInput("Job ID cannot be empty".to_string()));
        }

        // 2. Get job from repository
        let job = self.batch_repo.get_batch_job(&request.job_id).await?;

        // 3. Calculate progress from items
        let total = job.items.len();
        let completed = job
            .items
            .iter()
            .filter(|item| item.status == "completed")
            .count();
        let failed = job
            .items
            .iter()
            .filter(|item| item.status == "failed")
            .count();
        let pending = job
            .items
            .iter()
            .filter(|item| item.status == "pending" || item.status == "running")
            .count();

        let progress = BatchProgressDto {
            total,
            completed,
            failed,
            pending,
        };

        // 4. Determine overall status
        let status = determine_job_status(&job.status, total, completed, failed, pending);

        // 5. Return status + progress
        Ok(GetBatchStatusResponseDto {
            job_id: request.job_id,
            status,
            progress,
        })
    }
}

/// Determine the overall job status based on item states.
///
/// # Status Rules
///
/// - All completed → Completed
/// - Any running → Running
/// - No completed, no running → Pending
/// - All failed → Failed
fn determine_job_status(
    db_status: &str,
    total: usize,
    completed: usize,
    failed: usize,
    pending: usize,
) -> BatchJobStatus {
    // If database says completed or failed, trust that
    if db_status == "completed" {
        return BatchJobStatus::Completed;
    }

    if db_status == "failed" {
        return BatchJobStatus::Failed;
    }

    // If database says running, use running
    if db_status == "running" {
        return BatchJobStatus::Running;
    }

    // Otherwise, determine from item states
    if failed == total && total > 0 {
        // All items failed
        BatchJobStatus::Failed
    } else if completed + failed == total && total > 0 {
        // All items processed (some may have failed)
        BatchJobStatus::Completed
    } else if completed > 0 || failed > 0 {
        // Some items processed, others pending
        BatchJobStatus::Running
    } else {
        // No items processed yet
        BatchJobStatus::Pending
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    use crate::application::ports::batch_job_repository_port::{
        BatchJobItem, BatchJobItemStatus, BatchJobStatus as PortBatchJobStatus, BatchJobSummary,
    };

    // Mock batch repository
    struct MockBatchRepo {
        job: Option<PortBatchJobStatus>,
    }

    impl MockBatchRepo {
        fn new(job: Option<PortBatchJobStatus>) -> Self {
            Self { job }
        }
    }

    #[async_trait]
    impl BatchJobRepositoryPort for MockBatchRepo {
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

        async fn get_batch_job(&self, job_id: &str) -> Result<PortBatchJobStatus> {
            self.job
                .clone()
                .ok_or_else(|| AppError::NotFound(format!("Job {} not found", job_id)))
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

    fn create_test_job(
        status: &str,
        items: Vec<(&str, &str)>, // (id, status)
    ) -> PortBatchJobStatus {
        PortBatchJobStatus {
            id: "test-job".to_string(),
            job_type: "file_import".to_string(),
            status: status.to_string(),
            total_items: items.len() as i64,
            completed_items: items.iter().filter(|(_, s)| *s == "completed").count() as i64,
            failed_items: items.iter().filter(|(_, s)| *s == "failed").count() as i64,
            progress: 0.0,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            started_at: None,
            completed_at: None,
            error_message: None,
            items: items
                .into_iter()
                .map(|(id, status)| BatchJobItemStatus {
                    id: id.to_string(),
                    url: format!("/path/{}.txt", id),
                    document_id: None,
                    status: status.to_string(),
                    error_message: None,
                })
                .collect(),
        }
    }

    #[tokio::test]
    async fn test_job_in_progress() {
        let job = create_test_job(
            "running",
            vec![
                ("item1", "completed"),
                ("item2", "running"),
                ("item3", "pending"),
            ],
        );

        let batch_repo = Arc::new(MockBatchRepo::new(Some(job)));
        let use_case = GetBatchFileStatusUseCase::new(batch_repo);

        let request = GetBatchStatusRequestDto {
            job_id: "test-job".to_string(),
        };

        let response = use_case.execute(request).await.unwrap();

        assert_eq!(response.status, BatchJobStatus::Running);
        assert_eq!(response.progress.total, 3);
        assert_eq!(response.progress.completed, 1);
        assert_eq!(response.progress.failed, 0);
        assert_eq!(response.progress.pending, 2);
    }

    #[tokio::test]
    async fn test_job_completed() {
        let job = create_test_job(
            "completed",
            vec![
                ("item1", "completed"),
                ("item2", "completed"),
                ("item3", "completed"),
            ],
        );

        let batch_repo = Arc::new(MockBatchRepo::new(Some(job)));
        let use_case = GetBatchFileStatusUseCase::new(batch_repo);

        let request = GetBatchStatusRequestDto {
            job_id: "test-job".to_string(),
        };

        let response = use_case.execute(request).await.unwrap();

        assert_eq!(response.status, BatchJobStatus::Completed);
        assert_eq!(response.progress.total, 3);
        assert_eq!(response.progress.completed, 3);
        assert_eq!(response.progress.failed, 0);
        assert_eq!(response.progress.pending, 0);
    }

    #[tokio::test]
    async fn test_job_with_failures() {
        let job = create_test_job(
            "completed",
            vec![
                ("item1", "completed"),
                ("item2", "failed"),
                ("item3", "completed"),
            ],
        );

        let batch_repo = Arc::new(MockBatchRepo::new(Some(job)));
        let use_case = GetBatchFileStatusUseCase::new(batch_repo);

        let request = GetBatchStatusRequestDto {
            job_id: "test-job".to_string(),
        };

        let response = use_case.execute(request).await.unwrap();

        assert_eq!(response.status, BatchJobStatus::Completed);
        assert_eq!(response.progress.total, 3);
        assert_eq!(response.progress.completed, 2);
        assert_eq!(response.progress.failed, 1);
        assert_eq!(response.progress.pending, 0);
    }

    #[tokio::test]
    async fn test_all_items_failed() {
        let job = create_test_job(
            "failed",
            vec![
                ("item1", "failed"),
                ("item2", "failed"),
                ("item3", "failed"),
            ],
        );

        let batch_repo = Arc::new(MockBatchRepo::new(Some(job)));
        let use_case = GetBatchFileStatusUseCase::new(batch_repo);

        let request = GetBatchStatusRequestDto {
            job_id: "test-job".to_string(),
        };

        let response = use_case.execute(request).await.unwrap();

        assert_eq!(response.status, BatchJobStatus::Failed);
        assert_eq!(response.progress.total, 3);
        assert_eq!(response.progress.completed, 0);
        assert_eq!(response.progress.failed, 3);
        assert_eq!(response.progress.pending, 0);
    }

    #[tokio::test]
    async fn test_invalid_job_id_empty() {
        let batch_repo = Arc::new(MockBatchRepo::new(None));
        let use_case = GetBatchFileStatusUseCase::new(batch_repo);

        let request = GetBatchStatusRequestDto {
            job_id: "".to_string(),
        };

        let result = use_case.execute(request).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            AppError::InvalidInput(msg) => {
                assert!(msg.contains("cannot be empty"));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_job_not_found() {
        let batch_repo = Arc::new(MockBatchRepo::new(None));
        let use_case = GetBatchFileStatusUseCase::new(batch_repo);

        let request = GetBatchStatusRequestDto {
            job_id: "nonexistent".to_string(),
        };

        let result = use_case.execute(request).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            AppError::NotFound(_) => {}
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_pending_job() {
        let job = create_test_job("pending", vec![("item1", "pending"), ("item2", "pending")]);

        let batch_repo = Arc::new(MockBatchRepo::new(Some(job)));
        let use_case = GetBatchFileStatusUseCase::new(batch_repo);

        let request = GetBatchStatusRequestDto {
            job_id: "test-job".to_string(),
        };

        let response = use_case.execute(request).await.unwrap();

        assert_eq!(response.status, BatchJobStatus::Pending);
        assert_eq!(response.progress.total, 2);
        assert_eq!(response.progress.completed, 0);
        assert_eq!(response.progress.failed, 0);
        assert_eq!(response.progress.pending, 2);
    }
}
