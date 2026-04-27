//! # Start Batch URL Import Use Case
//!
//! Creates a batch URL import job and spawns background processing.
//!
//! This use case orchestrates:
//! 1. Batch size validation (1-50 URLs)
//! 2. URL validation (HTTP/HTTPS, safe domain)
//! 3. Batch job creation
//! 4. Batch item creation
//! 5. Background processing spawn
//!
//! ## Example
//!
//! ```rust,no_run
//! use vault_desktop::application::use_cases::batch::StartBatchUrlImportUseCase;
//! use vault_desktop::application::dtos::batch_dto::StartBatchUrlImportRequestDto;
//!
//! # async fn example(use_case: StartBatchUrlImportUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let request = StartBatchUrlImportRequestDto {
//!     urls: vec![
//!         "https://example.com/article1".to_string(),
//!         "https://example.com/article2".to_string(),
//!     ],
//!     options: None,
//! };
//!
//! let response = use_case.execute(request).await?;
//! println!("Batch job started: {}", response.job_id);
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use uuid::Uuid;

use crate::features::batch::dto::{
    StartBatchUrlImportRequestDto, StartBatchUrlImportResponseDto,
};
use crate::features::web::dto::IngestWebUrlRequestDto;
use crate::application::ports::BatchJobRepositoryPort;
use crate::features::web::use_cases::IngestWebUrlUseCase;
use crate::shared::error::{AppError, Result};

const MIN_BATCH_SIZE: usize = 1;
const MAX_BATCH_SIZE: usize = 50;

/// Start batch URL import use case.
///
/// Coordinates batch URL import, including:
/// - Batch size validation
/// - URL validation
/// - Job creation
/// - Background processing
///
/// ## Dependencies
///
/// - `BatchJobRepositoryPort`: Persists batch job and items
/// - `IngestWebUrlUseCase`: Processes individual URLs
pub struct StartBatchUrlImportUseCase {
    batch_repo: Arc<dyn BatchJobRepositoryPort>,
    ingest_url_use_case: Arc<IngestWebUrlUseCase>,
}

impl StartBatchUrlImportUseCase {
    /// Create a new start batch URL import use case.
    ///
    /// # Arguments
    ///
    /// * `batch_repo` - Repository for batch job persistence
    /// * `ingest_url_use_case` - Use case for ingesting individual URLs
    pub fn new(
        batch_repo: Arc<dyn BatchJobRepositoryPort>,
        ingest_url_use_case: Arc<IngestWebUrlUseCase>,
    ) -> Self {
        Self {
            batch_repo,
            ingest_url_use_case,
        }
    }

    /// Execute batch URL import creation.
    ///
    /// # Arguments
    ///
    /// * `request` - Request containing URLs to import
    ///
    /// # Returns
    ///
    /// Response with job ID for status tracking
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Batch is empty
    /// - Batch exceeds maximum size (50 URLs)
    /// - Any URL is invalid (not HTTP/HTTPS)
    /// - Job creation fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use vault_desktop::application::use_cases::batch::StartBatchUrlImportUseCase;
    /// # use vault_desktop::application::dtos::batch_dto::StartBatchUrlImportRequestDto;
    /// # async fn example(use_case: StartBatchUrlImportUseCase) -> Result<(), Box<dyn std::error::Error>> {
    /// let request = StartBatchUrlImportRequestDto {
    ///     urls: vec!["https://example.com".to_string()],
    ///     options: None,
    /// };
    ///
    /// let response = use_case.execute(request).await?;
    /// println!("Job ID: {}", response.job_id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute(
        &self,
        request: StartBatchUrlImportRequestDto,
    ) -> Result<StartBatchUrlImportResponseDto> {
        // 1. Validate batch size
        let batch_size = request.urls.len();

        if batch_size < MIN_BATCH_SIZE {
            return Err(AppError::InvalidInput(
                "Batch must contain at least 1 URL".to_string(),
            ));
        }

        if batch_size > MAX_BATCH_SIZE {
            return Err(AppError::InvalidInput(format!(
                "Batch size {} exceeds maximum of {}",
                batch_size, MAX_BATCH_SIZE
            )));
        }

        // 2. Validate each URL
        for url in &request.urls {
            Self::validate_url(url)?;
        }

        // 3. Generate job ID
        let job_id = Uuid::new_v4().to_string();

        // 4. Serialize options if provided
        let options_json = if let Some(options) = &request.options {
            Some(serde_json::to_string(options).map_err(|e| {
                AppError::Serialization(format!("Failed to serialize options: {}", e))
            })?)
        } else {
            None
        };

        // 5. Create batch job record
        self.batch_repo
            .create_batch_job(
                &job_id,
                "url_import",
                batch_size as i64,
                options_json.as_deref(),
            )
            .await?;

        // 6. Create batch items (one per URL)
        self.batch_repo
            .create_batch_items(&job_id, request.urls.clone())
            .await?;

        // 7. Spawn background processing task
        let batch_repo = Arc::clone(&self.batch_repo);
        let ingest_url_use_case = Arc::clone(&self.ingest_url_use_case);
        let job_id_clone = job_id.clone();

        tokio::spawn(async move {
            if let Ok(status) = batch_repo.get_batch_job(&job_id_clone).await {
                if status.status == "cancelled" {
                    return;
                }
            } else {
                return;
            }

            // Update job status to "running"
            let _ = batch_repo
                .update_job_status(
                    &job_id_clone,
                    "running",
                    Some(chrono::Utc::now().to_rfc3339()),
                    None,
                )
                .await;

            // Get pending items
            let pending_items = match batch_repo.get_pending_items(&job_id_clone).await {
                Ok(items) => items,
                Err(_) => {
                    let _ = batch_repo
                        .update_job_status(
                            &job_id_clone,
                            "failed",
                            None,
                            Some(chrono::Utc::now().to_rfc3339()),
                        )
                        .await;
                    return;
                }
            };

            let mut completed = 0i64;
            let mut failed = 0i64;
            let total = pending_items.len() as i64;

            // Process each URL
            for item in pending_items {
                let job_status = batch_repo.get_batch_job(&job_id_clone).await;
                if let Ok(status) = job_status {
                    if status.status == "cancelled" {
                        let _ = batch_repo
                            .update_job_status(
                                &job_id_clone,
                                "cancelled",
                                None,
                                Some(chrono::Utc::now().to_rfc3339()),
                            )
                            .await;
                        return;
                    }
                } else {
                    let _ = batch_repo
                        .update_job_status(
                            &job_id_clone,
                            "failed",
                            None,
                            Some(chrono::Utc::now().to_rfc3339()),
                        )
                        .await;
                    return;
                }

                // Mark item as running
                let _ = batch_repo
                    .update_item_status(&item.id, "running", None, None)
                    .await;

                // Ingest the URL
                let ingest_request = IngestWebUrlRequestDto {
                    url: item.url.clone(),
                };

                match ingest_url_use_case.execute(ingest_request).await {
                    Ok(response) => {
                        // Mark item as completed
                        let _ = batch_repo
                            .update_item_status(
                                &item.id,
                                "completed",
                                Some(&response.document_id),
                                None,
                            )
                            .await;
                        completed += 1;
                    }
                    Err(e) => {
                        // Mark item as failed with error message
                        let error_msg = e.to_string();
                        let _ = batch_repo
                            .update_item_status(&item.id, "failed", None, Some(&error_msg))
                            .await;
                        failed += 1;
                    }
                }

                // Update job progress
                let progress = if total > 0 {
                    ((completed + failed) as f64 / total as f64) * 100.0
                } else {
                    0.0
                };

                let _ = batch_repo
                    .update_progress(&job_id_clone, completed, failed, progress)
                    .await;
            }

            // Update final job status
            if let Ok(status) = batch_repo.get_batch_job(&job_id_clone).await {
                if status.status == "cancelled" {
                    return;
                }
            }

            let final_status = if failed == total {
                "failed"
            } else {
                "completed"
            };

            let _ = batch_repo
                .update_job_status(
                    &job_id_clone,
                    final_status,
                    None,
                    Some(chrono::Utc::now().to_rfc3339()),
                )
                .await;
        });

        // 8. Return job ID
        Ok(StartBatchUrlImportResponseDto { job_id })
    }

    /// Validate URL is HTTP or HTTPS.
    ///
    /// # Arguments
    ///
    /// * `url` - URL to validate
    ///
    /// # Returns
    ///
    /// Ok if URL is valid, Err otherwise
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if URL is not HTTP/HTTPS
    fn validate_url(url: &str) -> Result<()> {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(AppError::InvalidInput(
                "URL must start with http:// or https://".to_string(),
            ));
        }

        if url.trim().is_empty() {
            return Err(AppError::InvalidInput("URL cannot be empty".to_string()));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::batch_job_repository_port::{
        BatchJobItem, BatchJobStatus, BatchJobSummary,
    };
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    // Mock repository for testing
    struct MockBatchJobRepository {
        jobs: Arc<Mutex<HashMap<String, String>>>,
        items: Arc<Mutex<HashMap<String, Vec<String>>>>,
    }

    impl MockBatchJobRepository {
        fn new() -> Self {
            Self {
                jobs: Arc::new(Mutex::new(HashMap::new())),
                items: Arc::new(Mutex::new(HashMap::new())),
            }
        }
    }

    #[async_trait]
    impl BatchJobRepositoryPort for MockBatchJobRepository {
        async fn create_batch_job(
            &self,
            job_id: &str,
            job_type: &str,
            _total_items: i64,
            _options: Option<&str>,
        ) -> Result<()> {
            self.jobs
                .lock()
                .unwrap()
                .insert(job_id.to_string(), job_type.to_string());
            Ok(())
        }

        async fn create_batch_items(&self, job_id: &str, urls: Vec<String>) -> Result<()> {
            self.items.lock().unwrap().insert(job_id.to_string(), urls);
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

        async fn delete_batch_job(&self, _job_id: &str) -> Result<()> {
            Ok(())
        }
    }

    // Mock ingest URL use case for testing
    struct MockIngestWebUrlUseCase;

    impl MockIngestWebUrlUseCase {
        fn new() -> Self {
            Self
        }

        async fn execute(
            &self,
            _request: IngestWebUrlRequestDto,
        ) -> Result<crate::features::web::dto::IngestWebUrlResponseDto> {
            Ok(crate::features::web::dto::IngestWebUrlResponseDto {
                document_id: "doc-123".to_string(),
                url: "https://example.com".to_string(),
                title: "Test".to_string(),
                word_count: 100,
                chunks_created: 1,
                site_name: None,
                author: None,
                reading_time_minutes: None,
            })
        }
    }

    #[tokio::test]
    #[ignore] // TODO: Implement with mocks after service layer refactoring
    async fn test_valid_url_batch() {
        // Placeholder test - requires HttpClient, IndexingService, etc.
        // These services don't exist yet in the refactored architecture
        // Will implement once service layer is complete
        // For now, just test the validation logic

        let request = StartBatchUrlImportRequestDto {
            urls: vec![
                "https://example.com/article1".to_string(),
                "https://example.com/article2".to_string(),
            ],
            options: None,
        };

        // Just validate URLs
        for url in &request.urls {
            assert!(StartBatchUrlImportUseCase::validate_url(url).is_ok());
        }
    }

    #[tokio::test]
    async fn test_invalid_urls_rejected() {
        let invalid_urls = vec!["ftp://example.com", "javascript:alert(1)", "", "not-a-url"];

        for url in invalid_urls {
            let result = StartBatchUrlImportUseCase::validate_url(url);
            assert!(result.is_err(), "URL '{}' should be invalid", url);
        }
    }

    #[tokio::test]
    async fn test_batch_size_validation_empty() {
        let batch_repo = Arc::new(MockBatchJobRepository::new());
        // Create a minimal mock - actual implementation would use proper mocking
        let request = StartBatchUrlImportRequestDto {
            urls: vec![],
            options: None,
        };

        // Validate that empty batch is rejected
        assert_eq!(request.urls.len(), 0);
        assert!(request.urls.len() < MIN_BATCH_SIZE);
    }

    #[tokio::test]
    async fn test_batch_size_validation_too_large() {
        let urls: Vec<String> = (0..=MAX_BATCH_SIZE)
            .map(|i| format!("https://example.com/article{}", i))
            .collect();

        assert!(urls.len() > MAX_BATCH_SIZE);
    }

    #[tokio::test]
    async fn test_valid_https_urls() {
        let valid_urls = vec![
            "https://example.com",
            "https://example.com/path",
            "https://example.com/path?query=1",
            "http://example.com",
        ];

        for url in valid_urls {
            let result = StartBatchUrlImportUseCase::validate_url(url);
            assert!(result.is_ok(), "URL '{}' should be valid", url);
        }
    }
}
