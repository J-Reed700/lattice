//! # List Batch Jobs Use Case
//!
//! Lists all batch jobs with pagination support.
//!
//! This use case:
//! 1. Validates pagination parameters
//! 2. Fetches jobs from repository (newest first)
//! 3. Returns paginated job summaries
//!
//! ## Example
//!
//! ```rust,no_run
//! use lattice::application::use_cases::batch::ListBatchJobsUseCase;
//! use lattice::application::dtos::batch_dto::ListBatchJobsRequestDto;
//!
//! # async fn example(use_case: ListBatchJobsUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let request = ListBatchJobsRequestDto {
//!     limit: Some(20),
//!     offset: Some(0),
//! };
//!
//! let response = use_case.execute(request).await?;
//! println!("Found {} jobs", response.jobs.len());
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use chrono::{DateTime, NaiveDateTime, Utc};
use once_cell::sync::Lazy;

use crate::application::ports::BatchJobRepositoryPort;
use crate::features::batch::dto::{
    BatchJobSummaryDto, ListBatchJobsRequestDto, ListBatchJobsResponseDto,
};
use crate::shared::error::{AppError, Result};

/// Fallback epoch timestamp for invalid date parsing
///
/// # Panics
///
/// Panics on startup if hardcoded epoch date is invalid (this is a compile-time constant issue,
/// not a runtime error). This is acceptable as it would indicate a developer error.
#[allow(clippy::expect_used)] // Startup initialization - hardcoded constant
static EPOCH_FALLBACK: Lazy<DateTime<Utc>> = Lazy::new(|| {
    DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z")
        .expect(
            "IMMACULATE INVARIANT: Hardcoded epoch date '1970-01-01T00:00:00Z' is valid RFC3339",
        )
        .with_timezone(&Utc)
});

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 100;

fn parse_batch_timestamp(timestamp: &str) -> Option<DateTime<Utc>> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(timestamp) {
        return Some(parsed.with_timezone(&Utc));
    }

    for format in ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S%.f"] {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(timestamp, format) {
            return Some(parsed.and_utc());
        }
    }

    None
}

/// List batch jobs use case.
///
/// Retrieves paginated list of batch jobs.
///
/// ## Dependencies
///
/// - `BatchJobRepositoryPort`: Fetches batch job summaries
pub struct ListBatchJobsUseCase {
    batch_repo: Arc<dyn BatchJobRepositoryPort>,
}

impl ListBatchJobsUseCase {
    /// Create a new list batch jobs use case.
    ///
    /// # Arguments
    ///
    /// * `batch_repo` - Repository for batch job data
    pub fn new(batch_repo: Arc<dyn BatchJobRepositoryPort>) -> Self {
        Self { batch_repo }
    }

    /// Execute batch jobs listing.
    ///
    /// # Arguments
    ///
    /// * `request` - Request with optional limit and offset
    ///
    /// # Returns
    ///
    /// Response with paginated job summaries (newest first)
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Limit or offset is negative
    /// - Repository error
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use lattice::application::use_cases::batch::ListBatchJobsUseCase;
    /// # use lattice::application::dtos::batch_dto::ListBatchJobsRequestDto;
    /// # async fn example(use_case: ListBatchJobsUseCase) -> Result<(), Box<dyn std::error::Error>> {
    /// let request = ListBatchJobsRequestDto {
    ///     limit: Some(10),
    ///     offset: Some(0),
    /// };
    ///
    /// let response = use_case.execute(request).await?;
    /// for job in &response.jobs {
    ///     println!("Job {}: {}", job.job_id, job.status);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute(
        &self,
        request: ListBatchJobsRequestDto,
    ) -> Result<ListBatchJobsResponseDto> {
        // 1. Validate and normalize pagination parameters
        let limit = match request.limit {
            Some(l) if l < 0 => {
                return Err(AppError::InvalidInput(
                    "Limit cannot be negative".to_string(),
                ))
            }
            Some(l) if l > MAX_LIMIT => MAX_LIMIT,
            Some(l) => l,
            None => DEFAULT_LIMIT,
        };

        let offset = match request.offset {
            Some(o) if o < 0 => {
                return Err(AppError::InvalidInput(
                    "Offset cannot be negative".to_string(),
                ))
            }
            Some(o) => o,
            None => 0,
        };

        // 2. Fetch jobs from repository
        let job_summaries = self
            .batch_repo
            .list_batch_jobs(Some(limit), Some(offset))
            .await?;

        // 3. Transform to DTOs
        let jobs: Vec<BatchJobSummaryDto> = job_summaries
            .into_iter()
            .map(|summary| {
                let created_at =
                    parse_batch_timestamp(&summary.created_at).unwrap_or(*EPOCH_FALLBACK);

                BatchJobSummaryDto {
                    job_id: summary.id,
                    job_type: summary.job_type,
                    status: summary.status,
                    total_items: summary.total_items,
                    completed_items: summary.completed_items,
                    failed_items: summary.failed_items,
                    created_at: created_at.to_rfc3339(),
                }
            })
            .collect();

        // 4. Return response
        Ok(ListBatchJobsResponseDto { jobs })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::batch_job_repository_port::{
        BatchItemState, BatchJobItem, BatchJobStatus, BatchJobSummary,
    };
    use async_trait::async_trait;

    struct MockBatchJobRepository {
        jobs: Vec<BatchJobSummary>,
    }

    impl MockBatchJobRepository {
        fn new(jobs: Vec<BatchJobSummary>) -> Self {
            Self { jobs }
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
            limit: Option<i64>,
            offset: Option<i64>,
        ) -> Result<Vec<BatchJobSummary>> {
            let offset = offset.unwrap_or(0) as usize;
            let limit = limit.unwrap_or(50) as usize;

            Ok(self.jobs.iter().skip(offset).take(limit).cloned().collect())
        }

        async fn delete_batch_job(&self, _job_id: &str) -> Result<()> {
            Ok(())
        }
    }

    fn create_test_jobs(count: usize) -> Vec<BatchJobSummary> {
        (0..count)
            .map(|i| BatchJobSummary {
                id: format!("job-{}", i),
                job_type: "url_import".to_string(),
                status: "completed".to_string(),
                total_items: 10,
                completed_items: 10,
                failed_items: 0,
                progress: 100.0,
                created_at: "2024-01-01T00:00:00Z".to_string(),
                completed_at: Some("2024-01-01T00:05:00Z".to_string()),
            })
            .collect()
    }

    #[tokio::test]
    async fn test_list_jobs_with_defaults() {
        let jobs = create_test_jobs(5);
        let repo = Arc::new(MockBatchJobRepository::new(jobs));
        let use_case = ListBatchJobsUseCase::new(repo);

        let request = ListBatchJobsRequestDto {
            limit: None,
            offset: None,
        };

        let result = use_case.execute(request).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.jobs.len(), 5);
    }

    #[tokio::test]
    async fn test_list_jobs_with_limit() {
        let jobs = create_test_jobs(20);
        let repo = Arc::new(MockBatchJobRepository::new(jobs));
        let use_case = ListBatchJobsUseCase::new(repo);

        let request = ListBatchJobsRequestDto {
            limit: Some(10),
            offset: None,
        };

        let result = use_case.execute(request).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.jobs.len(), 10);
    }

    #[tokio::test]
    async fn test_list_jobs_with_pagination() {
        let jobs = create_test_jobs(30);
        let repo = Arc::new(MockBatchJobRepository::new(jobs));
        let use_case = ListBatchJobsUseCase::new(repo);

        let request = ListBatchJobsRequestDto {
            limit: Some(10),
            offset: Some(10),
        };

        let result = use_case.execute(request).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.jobs.len(), 10);
        assert_eq!(response.jobs[0].job_id, "job-10");
    }

    #[tokio::test]
    async fn test_list_jobs_limit_capped_at_max() {
        let jobs = create_test_jobs(150);
        let repo = Arc::new(MockBatchJobRepository::new(jobs));
        let use_case = ListBatchJobsUseCase::new(repo);

        let request = ListBatchJobsRequestDto {
            limit: Some(200), // Exceeds MAX_LIMIT
            offset: None,
        };

        let result = use_case.execute(request).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.jobs.len(), MAX_LIMIT as usize);
    }

    #[tokio::test]
    async fn test_list_jobs_negative_limit() {
        let repo = Arc::new(MockBatchJobRepository::new(vec![]));
        let use_case = ListBatchJobsUseCase::new(repo);

        let request = ListBatchJobsRequestDto {
            limit: Some(-1),
            offset: None,
        };

        let result = use_case.execute(request).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_list_jobs_negative_offset() {
        let repo = Arc::new(MockBatchJobRepository::new(vec![]));
        let use_case = ListBatchJobsUseCase::new(repo);

        let request = ListBatchJobsRequestDto {
            limit: None,
            offset: Some(-1),
        };

        let result = use_case.execute(request).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_list_jobs_empty_list() {
        let repo = Arc::new(MockBatchJobRepository::new(vec![]));
        let use_case = ListBatchJobsUseCase::new(repo);

        let request = ListBatchJobsRequestDto {
            limit: None,
            offset: None,
        };

        let result = use_case.execute(request).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert!(response.jobs.is_empty());
    }

    #[tokio::test]
    async fn test_list_jobs_supports_sqlite_timestamp_format() {
        let jobs = vec![BatchJobSummary {
            id: "job-sqlite-ts".to_string(),
            job_type: "file_import".to_string(),
            status: "completed".to_string(),
            total_items: 1,
            completed_items: 1,
            failed_items: 0,
            progress: 100.0,
            created_at: "2026-02-04 12:34:56".to_string(),
            completed_at: None,
        }];

        let repo = Arc::new(MockBatchJobRepository::new(jobs));
        let use_case = ListBatchJobsUseCase::new(repo);

        let response = use_case
            .execute(ListBatchJobsRequestDto {
                limit: None,
                offset: None,
            })
            .await
            .unwrap();

        assert_eq!(response.jobs[0].created_at, "2026-02-04T12:34:56+00:00");
    }
}
