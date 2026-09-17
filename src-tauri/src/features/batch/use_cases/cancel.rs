//! # Cancel Batch Job Use Case
//!
//! Cancels all pending items in a batch job.
//!
//! This use case:
//! 1. Validates job ID
//! 2. Marks all pending/running items as cancelled
//! 3. Returns count of cancelled items
//!
//! ## Example
//!
//! ```rust,no_run
//! use lattice::application::use_cases::batch::CancelBatchJobUseCase;
//! use lattice::application::dtos::batch_dto::CancelBatchJobRequestDto;
//!
//! # async fn example(use_case: CancelBatchJobUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let request = CancelBatchJobRequestDto {
//!     job_id: "batch-job-123".to_string(),
//! };
//!
//! let response = use_case.execute(request).await?;
//! println!("Cancelled {} items", response.cancelled_count);
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use crate::application::ports::BatchJobRepositoryPort;
use crate::features::batch::dto::{CancelBatchJobRequestDto, CancelBatchJobResponseDto};
use crate::shared::error::{AppError, Result};

/// Cancel batch job use case.
///
/// Cancels pending and running items in a batch job.
///
/// ## Dependencies
///
/// - `BatchJobRepositoryPort`: Updates batch job status
pub struct CancelBatchJobUseCase {
    batch_repo: Arc<dyn BatchJobRepositoryPort>,
}

impl CancelBatchJobUseCase {
    /// Create a new cancel batch job use case.
    ///
    /// # Arguments
    ///
    /// * `batch_repo` - Repository for batch job data
    pub fn new(batch_repo: Arc<dyn BatchJobRepositoryPort>) -> Self {
        Self { batch_repo }
    }

    /// Execute batch job cancellation.
    ///
    /// # Arguments
    ///
    /// * `request` - Request containing job ID to cancel
    ///
    /// # Returns
    ///
    /// Response with count of cancelled items
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
    /// # use lattice::application::use_cases::batch::CancelBatchJobUseCase;
    /// # use lattice::application::dtos::batch_dto::CancelBatchJobRequestDto;
    /// # async fn example(use_case: CancelBatchJobUseCase) -> Result<(), Box<dyn std::error::Error>> {
    /// let request = CancelBatchJobRequestDto {
    ///     job_id: "batch-job-123".to_string(),
    /// };
    ///
    /// let response = use_case.execute(request).await?;
    /// println!("Cancelled: {}", response.cancelled_count);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute(
        &self,
        request: CancelBatchJobRequestDto,
    ) -> Result<CancelBatchJobResponseDto> {
        // 1. Validate job ID
        if request.job_id.trim().is_empty() {
            return Err(AppError::InvalidInput("Job ID cannot be empty".to_string()));
        }

        let job = self.batch_repo.get_batch_job(&request.job_id).await?;
        if matches!(job.status.as_str(), "completed" | "failed" | "cancelled") {
            return Ok(CancelBatchJobResponseDto { cancelled_count: 0 });
        }

        // Publish cancellation first so the worker cannot begin another queued
        // item while cancellation is being applied to those rows.
        self.batch_repo
            .update_job_status(
                &request.job_id,
                "cancelled",
                None,
                Some(chrono::Utc::now().to_rfc3339()),
            )
            .await?;

        // A job may have only one currently-running item and zero pending items;
        // it is still cancelled and the worker stops after its current safe point.
        let cancelled_count = self
            .batch_repo
            .cancel_pending_items(&request.job_id)
            .await?;

        // 4. Return cancelled count
        Ok(CancelBatchJobResponseDto { cancelled_count })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::batch_job_repository_port::{
        BatchJobItem, BatchJobStatus, BatchJobSummary,
    };
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct MockBatchJobRepository {
        cancelled_items: Arc<Mutex<usize>>,
        job_status: Arc<Mutex<String>>,
    }

    impl MockBatchJobRepository {
        fn new(cancelled_items: usize) -> Self {
            Self {
                cancelled_items: Arc::new(Mutex::new(cancelled_items)),
                job_status: Arc::new(Mutex::new("running".into())),
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
            status: &str,
            _started_at: Option<String>,
            _completed_at: Option<String>,
        ) -> Result<()> {
            *self.job_status.lock().unwrap() = status.to_string();
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

        async fn get_batch_job(&self, _job_id: &str) -> Result<BatchJobStatus> {
            Ok(BatchJobStatus {
                id: _job_id.to_string(),
                job_type: "file_import".into(),
                status: self.job_status.lock().unwrap().clone(),
                total_items: 1,
                completed_items: 0,
                failed_items: 0,
                progress: 0.0,
                created_at: "2026-01-01T00:00:00Z".into(),
                started_at: None,
                completed_at: None,
                error_message: None,
                items: vec![],
            })
        }

        async fn get_pending_items(&self, _job_id: &str) -> Result<Vec<BatchJobItem>> {
            Ok(vec![])
        }

        async fn cancel_pending_items(&self, _job_id: &str) -> Result<usize> {
            Ok(*self.cancelled_items.lock().unwrap())
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
    async fn test_cancel_job_with_pending_items() {
        let repo = Arc::new(MockBatchJobRepository::new(5));
        let use_case = CancelBatchJobUseCase::new(repo.clone());

        let request = CancelBatchJobRequestDto {
            job_id: "test-job-123".to_string(),
        };

        let result = use_case.execute(request).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.cancelled_count, 5);
        assert_eq!(*repo.job_status.lock().unwrap(), "cancelled");
    }

    #[tokio::test]
    async fn test_cancel_job_with_only_a_running_item() {
        let repo = Arc::new(MockBatchJobRepository::new(0));
        let use_case = CancelBatchJobUseCase::new(repo.clone());

        let request = CancelBatchJobRequestDto {
            job_id: "completed-job".to_string(),
        };

        let result = use_case.execute(request).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.cancelled_count, 0);
        assert_eq!(*repo.job_status.lock().unwrap(), "cancelled");
    }

    #[tokio::test]
    async fn test_cancel_job_empty_id() {
        let repo = Arc::new(MockBatchJobRepository::new(0));
        let use_case = CancelBatchJobUseCase::new(repo);

        let request = CancelBatchJobRequestDto {
            job_id: "".to_string(),
        };

        let result = use_case.execute(request).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::InvalidInput(_)));
    }
}
