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
//! use lattice::application::use_cases::batch::StartBatchUrlImportUseCase;
//! use lattice::application::dtos::batch_dto::StartBatchUrlImportRequestDto;
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

use crate::application::ports::BatchJobRepositoryPort;
use crate::features::batch::dto::{StartBatchUrlImportRequestDto, StartBatchUrlImportResponseDto};
use crate::features::web::dto::IngestWebUrlRequestDto;
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
    /// # use lattice::application::use_cases::batch::StartBatchUrlImportUseCase;
    /// # use lattice::application::dtos::batch_dto::StartBatchUrlImportRequestDto;
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

            let _ = batch_repo
                .update_job_status(
                    &job_id_clone,
                    "running",
                    Some(chrono::Utc::now().to_rfc3339()),
                    None,
                )
                .await;

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

                let progress = if total > 0 {
                    ((completed + failed) as f64 / total as f64) * 100.0
                } else {
                    0.0
                };

                let _ = batch_repo
                    .update_progress(&job_id_clone, completed, failed, progress)
                    .await;
            }

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
        BatchJobItem, BatchJobItemStatus, BatchJobStatus, BatchJobSummary,
    };
    use crate::features::web::mocks::MockWebIngestionService;
    use crate::features::web::WebIngestionServiceTrait;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// In-memory record of one batch job, mirroring the columns the SQLite
    /// repository writes so the spawned worker can be observed end to end.
    #[derive(Clone)]
    struct MockJob {
        job_type: String,
        status: String,
        completed_items: i64,
        failed_items: i64,
        progress: f64,
        items: Vec<BatchJobItemStatus>,
    }

    struct MockBatchJobRepository {
        jobs: Arc<Mutex<HashMap<String, MockJob>>>,
    }

    impl MockBatchJobRepository {
        fn new() -> Self {
            Self {
                jobs: Arc::new(Mutex::new(HashMap::new())),
            }
        }

        /// Snapshot of a job, for assertions from the test thread.
        fn job(&self, job_id: &str) -> Option<MockJob> {
            self.jobs.lock().unwrap().get(job_id).cloned()
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
            self.jobs.lock().unwrap().insert(
                job_id.to_string(),
                MockJob {
                    job_type: job_type.to_string(),
                    status: "pending".to_string(),
                    completed_items: 0,
                    failed_items: 0,
                    progress: 0.0,
                    items: Vec::new(),
                },
            );
            Ok(())
        }

        async fn create_batch_items(&self, job_id: &str, urls: Vec<String>) -> Result<()> {
            let mut jobs = self.jobs.lock().unwrap();
            let job = jobs
                .get_mut(job_id)
                .ok_or_else(|| AppError::NotFound(format!("Unknown job {job_id}")))?;
            job.items = urls
                .into_iter()
                .enumerate()
                .map(|(index, url)| BatchJobItemStatus {
                    id: format!("{job_id}-item-{index}"),
                    url,
                    document_id: None,
                    status: "pending".to_string(),
                    error_message: None,
                })
                .collect();
            Ok(())
        }

        async fn update_job_status(
            &self,
            job_id: &str,
            status: &str,
            _started_at: Option<String>,
            _completed_at: Option<String>,
        ) -> Result<()> {
            if let Some(job) = self.jobs.lock().unwrap().get_mut(job_id) {
                job.status = status.to_string();
            }
            Ok(())
        }

        async fn update_progress(
            &self,
            job_id: &str,
            completed: i64,
            failed: i64,
            progress: f64,
        ) -> Result<()> {
            if let Some(job) = self.jobs.lock().unwrap().get_mut(job_id) {
                job.completed_items = completed;
                job.failed_items = failed;
                job.progress = progress;
            }
            Ok(())
        }

        async fn update_item_status(
            &self,
            item_id: &str,
            status: &str,
            document_id: Option<&str>,
            error_message: Option<&str>,
        ) -> Result<()> {
            let mut jobs = self.jobs.lock().unwrap();
            for job in jobs.values_mut() {
                if let Some(item) = job.items.iter_mut().find(|item| item.id == item_id) {
                    item.status = status.to_string();
                    item.document_id = document_id.map(str::to_string);
                    item.error_message = error_message.map(str::to_string);
                    return Ok(());
                }
            }
            Err(AppError::NotFound(format!("Unknown batch item {item_id}")))
        }

        async fn get_batch_job(&self, job_id: &str) -> Result<BatchJobStatus> {
            let jobs = self.jobs.lock().unwrap();
            let job = jobs
                .get(job_id)
                .ok_or_else(|| AppError::NotFound(format!("Unknown job {job_id}")))?;
            Ok(BatchJobStatus {
                id: job_id.to_string(),
                job_type: job.job_type.clone(),
                status: job.status.clone(),
                total_items: job.items.len() as i64,
                completed_items: job.completed_items,
                failed_items: job.failed_items,
                progress: job.progress,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                started_at: None,
                completed_at: None,
                error_message: None,
                items: job.items.clone(),
            })
        }

        async fn get_pending_items(&self, job_id: &str) -> Result<Vec<BatchJobItem>> {
            let jobs = self.jobs.lock().unwrap();
            let job = jobs
                .get(job_id)
                .ok_or_else(|| AppError::NotFound(format!("Unknown job {job_id}")))?;
            Ok(job
                .items
                .iter()
                .filter(|item| item.status == "pending")
                .map(|item| BatchJobItem {
                    id: item.id.clone(),
                    url: item.url.clone(),
                })
                .collect())
        }

        async fn cancel_pending_items(&self, job_id: &str) -> Result<usize> {
            let mut jobs = self.jobs.lock().unwrap();
            let Some(job) = jobs.get_mut(job_id) else {
                return Ok(0);
            };
            let mut cancelled = 0;
            for item in job.items.iter_mut().filter(|i| i.status == "pending") {
                item.status = "cancelled".to_string();
                cancelled += 1;
            }
            Ok(cancelled)
        }

        async fn list_batch_jobs(
            &self,
            _limit: Option<i64>,
            _offset: Option<i64>,
        ) -> Result<Vec<BatchJobSummary>> {
            Ok(vec![])
        }

        async fn delete_batch_job(&self, job_id: &str) -> Result<()> {
            self.jobs.lock().unwrap().remove(job_id);
            Ok(())
        }
    }

    /// `execute` spawns the worker, so poll until it reaches a terminal state.
    async fn await_finished(repo: &MockBatchJobRepository, job_id: &str) -> MockJob {
        for _ in 0..500 {
            if let Some(job) = repo.job(job_id) {
                if job.status == "completed" || job.status == "failed" {
                    return job;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("batch job {job_id} never reached a terminal status");
    }

    #[tokio::test]
    async fn test_valid_url_batch() {
        let batch_repo = Arc::new(MockBatchJobRepository::new());
        let ingestion = Arc::new(MockWebIngestionService::new());
        let use_case = StartBatchUrlImportUseCase::new(
            Arc::clone(&batch_repo) as Arc<dyn BatchJobRepositoryPort>,
            Arc::new(IngestWebUrlUseCase::new(
                Arc::clone(&ingestion) as Arc<dyn WebIngestionServiceTrait>
            )),
        );

        let urls = vec![
            "https://example.com/article1".to_string(),
            "https://example.com/article2".to_string(),
        ];
        let response = use_case
            .execute(StartBatchUrlImportRequestDto {
                urls: urls.clone(),
                options: None,
            })
            .await
            .unwrap();

        assert!(!response.job_id.is_empty());

        // The job row and one item per URL are written before `execute` returns.
        let queued = batch_repo.job(&response.job_id).unwrap();
        assert_eq!(queued.job_type, "url_import");
        assert_eq!(
            queued
                .items
                .iter()
                .map(|item| item.url.clone())
                .collect::<Vec<_>>(),
            urls
        );

        // The spawned worker drains the queue through the ingestion service.
        let finished = await_finished(&batch_repo, &response.job_id).await;
        assert_eq!(finished.status, "completed");
        assert_eq!(finished.completed_items, 2);
        assert_eq!(finished.failed_items, 0);
        assert!((finished.progress - 100.0).abs() < f64::EPSILON);
        assert!(finished
            .items
            .iter()
            .all(|item| item.status == "completed" && item.document_id.is_some()));
        assert_eq!(ingestion.get_ingested_urls(), urls);
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
        let request = StartBatchUrlImportRequestDto {
            urls: vec![],
            options: None,
        };

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
