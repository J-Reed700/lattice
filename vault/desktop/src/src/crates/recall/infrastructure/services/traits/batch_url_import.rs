//! Batch URL import service trait
//!
//! This module defines the service interface for batch URL import operations,
//! following the DDD service trait pattern.

use crate::application::ports::batch_job_repository_port::BatchJobStatus;
use crate::shared::error::AppError;
use async_trait::async_trait;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

/// Service for batch URL import operations
///
/// This service orchestrates the entire batch import workflow:
/// - Create batch jobs
/// - Process URLs in background
/// - Track progress
/// - Handle cancellation
///
/// # Architecture
///
/// Following the "bricks and studs" philosophy:
/// - **Stud**: Trait defines public contract
/// - **Brick**: Concrete implementation handles orchestration
/// - **Regeneratable**: Can swap implementations (real vs mock)
///
/// # Example
///
/// ```rust
/// use crate::infrastructure::services::traits::BatchUrlImportServiceTrait;
///
/// async fn import_articles<S: BatchUrlImportServiceTrait>(
///     service: &S,
///     urls: Vec<String>,
/// ) -> Result<String, AppError> {
///     service.start_batch_import(urls, None).await
/// }
/// ```
#[async_trait]
pub trait BatchUrlImportServiceTrait: Send + Sync {
    /// Create a batch job and start processing URLs in background
    ///
    /// # Arguments
    /// * `urls` - List of URLs to import
    /// * `options` - Optional JSON string with import options
    ///
    /// # Returns
    /// Job ID for tracking progress
    ///
    /// # Example
    /// ```
    /// let job_id = service.start_batch_import(vec![
    ///     "https://example.com/article1".to_string(),
    ///     "https://example.com/article2".to_string(),
    /// ], None).await?;
    /// ```
    async fn start_batch_import(
        &self,
        urls: Vec<String>,
        options: Option<String>,
    ) -> Result<String, AppError>;

    /// Get status of a batch job
    ///
    /// # Arguments
    /// * `job_id` - Batch job ID
    ///
    /// # Returns
    /// BatchJobStatus with progress info
    ///
    /// # Example
    /// ```
    /// let status = service.get_batch_status(&job_id).await?;
    /// println!("Progress: {:.1}%", status.progress);
    /// ```
    async fn get_batch_status(&self, job_id: &str) -> Result<BatchJobStatus, AppError>;

    /// Cancel a running batch job
    ///
    /// # Arguments
    /// * `job_id` - Batch job ID
    ///
    /// # Returns
    /// Number of items cancelled
    ///
    /// # Example
    /// ```
    /// let cancelled = service.cancel_batch_job(&job_id).await?;
    /// println!("Cancelled {} pending items", cancelled);
    /// ```
    async fn cancel_batch_job(&self, job_id: &str) -> Result<usize, AppError>;
}

/// Mock implementation for testing
///
/// This mock supports:
/// - Failure injection via `with_failure()`
/// - Job ID injection via `with_job_id()`
/// - Status injection via `set_job_status()`
///
/// # Example
///
/// ```rust
/// use crate::infrastructure::services::traits::MockBatchUrlImportService;
///
/// #[tokio::test]
/// async fn test_batch_import() {
///     let mock = MockBatchUrlImportService::new()
///         .with_job_id("test-job-123".to_string());
///
///     let job_id = mock.start_batch_import(
///         vec!["https://example.com".to_string()],
///         None,
///     ).await.unwrap();
///
///     assert_eq!(job_id, "test-job-123");
/// }
/// ```
pub struct MockBatchUrlImportService {
    should_fail: bool,
    mock_job_id: Option<String>,
    mock_statuses: Arc<Mutex<HashMap<String, BatchJobStatus>>>,
}

impl MockBatchUrlImportService {
    /// Create a new mock service
    pub fn new() -> Self {
        Self {
            should_fail: false,
            mock_job_id: None,
            mock_statuses: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Configure mock to fail all operations
    pub fn with_failure(mut self) -> Self {
        self.should_fail = true;
        self
    }

    /// Configure mock to return specific job ID
    pub fn with_job_id(mut self, job_id: String) -> Self {
        self.mock_job_id = Some(job_id);
        self
    }

    /// Inject a job status for testing
    pub fn set_job_status(&self, job_id: &str, status: BatchJobStatus) {
        self.mock_statuses.lock().insert(job_id.to_string(), status);
    }
}

impl Default for MockBatchUrlImportService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BatchUrlImportServiceTrait for MockBatchUrlImportService {
    async fn start_batch_import(
        &self,
        urls: Vec<String>,
        _options: Option<String>,
    ) -> Result<String, AppError> {
        if self.should_fail {
            return Err(AppError::Database("Mock batch import failed".to_string()));
        }

        let job_id = self
            .mock_job_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        // Create mock status
        let status = BatchJobStatus {
            id: job_id.clone(),
            job_type: "url_import".to_string(),
            status: "pending".to_string(),
            total_items: urls.len() as i64,
            completed_items: 0,
            failed_items: 0,
            progress: 0.0,
            created_at: chrono::Utc::now().to_rfc3339(),
            started_at: None,
            completed_at: None,
            error_message: None,
            items: urls
                .iter()
                .map(|url| {
                    use crate::application::ports::batch_job_repository_port::BatchJobItemStatus;
                    BatchJobItemStatus {
                        id: uuid::Uuid::new_v4().to_string(),
                        url: url.clone(),
                        document_id: None,
                        status: "pending".to_string(),
                        error_message: None,
                    }
                })
                .collect(),
        };

        self.set_job_status(&job_id, status);

        Ok(job_id)
    }

    async fn get_batch_status(&self, job_id: &str) -> Result<BatchJobStatus, AppError> {
        if self.should_fail {
            return Err(AppError::NotFound("Mock job not found".to_string()));
        }

        self.mock_statuses
            .lock()
            .get(job_id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("Job not found: {}", job_id)))
    }

    async fn cancel_batch_job(&self, job_id: &str) -> Result<usize, AppError> {
        if self.should_fail {
            return Err(AppError::NotFound("Mock job not found".to_string()));
        }

        // Update mock status to cancelled
        let mut statuses = self.mock_statuses.lock();
        if let Some(status) = statuses.get_mut(job_id) {
            status.status = "cancelled".to_string();
            let pending = status
                .items
                .iter()
                .filter(|i| i.status == "pending")
                .count();
            Ok(pending)
        } else {
            Err(AppError::NotFound(format!("Job not found: {}", job_id)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_start_batch_import() {
        let mock = MockBatchUrlImportService::new();

        let urls = vec![
            "https://example.com/1".to_string(),
            "https://example.com/2".to_string(),
        ];

        let job_id = mock.start_batch_import(urls.clone(), None).await.unwrap();

        assert!(!job_id.is_empty());

        let status = mock.get_batch_status(&job_id).await.unwrap();
        assert_eq!(status.total_items, 2);
        assert_eq!(status.status, "pending");
    }

    #[tokio::test]
    async fn test_mock_with_job_id() {
        let mock = MockBatchUrlImportService::new().with_job_id("test-123".to_string());

        let job_id = mock
            .start_batch_import(vec!["https://example.com".to_string()], None)
            .await
            .unwrap();

        assert_eq!(job_id, "test-123");
    }

    #[tokio::test]
    async fn test_mock_with_failure() {
        let mock = MockBatchUrlImportService::new().with_failure();

        let result = mock
            .start_batch_import(vec!["https://example.com".to_string()], None)
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_cancel_batch_job() {
        let mock = MockBatchUrlImportService::new();

        let urls = vec![
            "https://example.com/1".to_string(),
            "https://example.com/2".to_string(),
        ];

        let job_id = mock.start_batch_import(urls, None).await.unwrap();

        let cancelled = mock.cancel_batch_job(&job_id).await.unwrap();

        assert_eq!(cancelled, 2);

        let status = mock.get_batch_status(&job_id).await.unwrap();
        assert_eq!(status.status, "cancelled");
    }

    #[tokio::test]
    async fn test_mock_set_job_status() {
        let mock = MockBatchUrlImportService::new();

        let custom_status = BatchJobStatus {
            id: "custom-job".to_string(),
            job_type: "url_import".to_string(),
            status: "completed".to_string(),
            total_items: 5,
            completed_items: 5,
            failed_items: 0,
            progress: 100.0,
            created_at: chrono::Utc::now().to_rfc3339(),
            started_at: Some(chrono::Utc::now().to_rfc3339()),
            completed_at: Some(chrono::Utc::now().to_rfc3339()),
            error_message: None,
            items: vec![],
        };

        mock.set_job_status("custom-job", custom_status.clone());

        let status = mock.get_batch_status("custom-job").await.unwrap();
        assert_eq!(status.status, "completed");
        assert_eq!(status.total_items, 5);
        assert_eq!(status.progress, 100.0);
    }
}
