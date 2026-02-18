//! # Delete Batch Job Use Case
//!
//! Deletes a batch job and all its items (cascade delete).
//!
//! This use case:
//! 1. Validates job ID
//! 2. Deletes job and all items from repository
//! 3. Returns success status
//!
//! ## Example
//!
//! ```rust,no_run
//! use vault_desktop::application::use_cases::batch::DeleteBatchJobUseCase;
//! use vault_desktop::application::dtos::batch_dto::DeleteBatchJobRequestDto;
//!
//! # async fn example(use_case: DeleteBatchJobUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let request = DeleteBatchJobRequestDto {
//!     job_id: "batch-job-123".to_string(),
//! };
//!
//! let response = use_case.execute(request).await?;
//! println!("Deleted: {}", response.success);
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use crate::application::dtos::batch_dto::{DeleteBatchJobRequestDto, DeleteBatchJobResponseDto};
use crate::application::ports::BatchJobRepositoryPort;
use crate::shared::error::{AppError, Result};

/// Delete batch job use case.
///
/// Permanently deletes a batch job and all its items.
///
/// ## Dependencies
///
/// - `BatchJobRepositoryPort`: Deletes batch job data
pub struct DeleteBatchJobUseCase {
    batch_repo: Arc<dyn BatchJobRepositoryPort>,
}

impl DeleteBatchJobUseCase {
    /// Create a new delete batch job use case.
    ///
    /// # Arguments
    ///
    /// * `batch_repo` - Repository for batch job data
    pub fn new(batch_repo: Arc<dyn BatchJobRepositoryPort>) -> Self {
        Self { batch_repo }
    }

    /// Execute batch job deletion.
    ///
    /// # Arguments
    ///
    /// * `request` - Request containing job ID to delete
    ///
    /// # Returns
    ///
    /// Response indicating success
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
    /// # use vault_desktop::application::use_cases::batch::DeleteBatchJobUseCase;
    /// # use vault_desktop::application::dtos::batch_dto::DeleteBatchJobRequestDto;
    /// # async fn example(use_case: DeleteBatchJobUseCase) -> Result<(), Box<dyn std::error::Error>> {
    /// let request = DeleteBatchJobRequestDto {
    ///     job_id: "batch-job-123".to_string(),
    /// };
    ///
    /// let response = use_case.execute(request).await?;
    /// assert!(response.success);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute(
        &self,
        request: DeleteBatchJobRequestDto,
    ) -> Result<DeleteBatchJobResponseDto> {
        // 1. Validate job ID
        if request.job_id.trim().is_empty() {
            return Err(AppError::InvalidInput("Job ID cannot be empty".to_string()));
        }

        // 2. Delete batch job (cascade deletes items)
        self.batch_repo.delete_batch_job(&request.job_id).await?;

        // 3. Return success
        Ok(DeleteBatchJobResponseDto { success: true })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::batch_job_repository_port::{
        BatchJobItem, BatchJobStatus, BatchJobSummary,
    };
    use async_trait::async_trait;
    use std::collections::HashSet;
    use std::sync::Mutex;

    struct MockBatchJobRepository {
        deleted_jobs: Arc<Mutex<HashSet<String>>>,
        should_fail: bool,
    }

    impl MockBatchJobRepository {
        fn new(should_fail: bool) -> Self {
            Self {
                deleted_jobs: Arc::new(Mutex::new(HashSet::new())),
                should_fail,
            }
        }

        fn was_deleted(&self, job_id: &str) -> bool {
            self.deleted_jobs.lock().unwrap().contains(job_id)
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

        async fn get_batch_job(&self, _job_id: &str) -> Result<BatchJobStatus> {
            unimplemented!()
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

        async fn delete_batch_job(&self, job_id: &str) -> Result<()> {
            if self.should_fail {
                return Err(AppError::NotFound(format!("Job not found: {}", job_id)));
            }

            self.deleted_jobs.lock().unwrap().insert(job_id.to_string());
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_delete_job_success() {
        let repo = Arc::new(MockBatchJobRepository::new(false));
        let use_case =
            DeleteBatchJobUseCase::new(Arc::clone(&repo) as Arc<dyn BatchJobRepositoryPort>);

        let request = DeleteBatchJobRequestDto {
            job_id: "test-job-123".to_string(),
        };

        let result = use_case.execute(request).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert!(response.success);
        assert!(repo.was_deleted("test-job-123"));
    }

    #[tokio::test]
    async fn test_delete_job_not_found() {
        let repo = Arc::new(MockBatchJobRepository::new(true));
        let use_case = DeleteBatchJobUseCase::new(repo);

        let request = DeleteBatchJobRequestDto {
            job_id: "non-existent-job".to_string(),
        };

        let result = use_case.execute(request).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_delete_job_empty_id() {
        let repo = Arc::new(MockBatchJobRepository::new(false));
        let use_case = DeleteBatchJobUseCase::new(repo);

        let request = DeleteBatchJobRequestDto {
            job_id: "".to_string(),
        };

        let result = use_case.execute(request).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_delete_job_whitespace_id() {
        let repo = Arc::new(MockBatchJobRepository::new(false));
        let use_case = DeleteBatchJobUseCase::new(repo);

        let request = DeleteBatchJobRequestDto {
            job_id: "   ".to_string(),
        };

        let result = use_case.execute(request).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::InvalidInput(_)));
    }
}
