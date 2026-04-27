//! Batch URL import service implementation
//!
//! Orchestrates batch URL import workflow: create job → process URLs → track progress.
//! Uses the "bricks and studs" philosophy with clear service boundaries.

use async_trait::async_trait;
use chrono::Utc;
use std::sync::Arc;
use tokio::task;
use uuid::Uuid;

use crate::application::ports::batch_job_repository_port::{
    BatchJobRepositoryPort, BatchJobStatus,
};
use crate::infrastructure::audit::{get_audit_logger, AuditAction, AuditEvent, AuditResult};
use crate::features::batch::BatchUrlImportServiceTrait;
use crate::features::web::WebIngestionServiceTrait;
use crate::shared::error::AppError;

/// Service for batch URL import operations
///
/// Orchestrates the entire batch workflow:
/// 1. Create batch job in database (status: pending)
/// 2. Spawn background Tokio task for processing
/// 3. Process each URL: extract article → create document
/// 4. Update progress after each URL
/// 5. Mark job as completed/failed
///
/// # Architecture
///
/// - **Dependencies**: WebIngestionService, BatchJobRepository
/// - **Async Processing**: Background task with Tokio
/// - **Error Handling**: Individual failures don't stop batch
/// - **Progress Tracking**: Real-time updates via repository
///
/// # Example
///
/// ```rust,no_run
/// use vault_desktop::infrastructure::services::batch_url_import::BatchUrlImportService;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let service = BatchUrlImportService::new(
///     web_ingestion_service,
///     batch_job_repository,
/// );
///
/// let job_id = service.start_batch_import(
///     vec!["https://example.com/article".to_string()],
///     None,
/// ).await?;
///
/// // Job runs in background, check status later
/// let status = service.get_batch_status(&job_id).await?;
/// println!("Progress: {:.1}%", status.progress);
/// # Ok(())
/// # }
/// ```
pub struct BatchUrlImportService {
    web_ingestion_service: Arc<dyn WebIngestionServiceTrait>,
    batch_job_repository: Arc<dyn BatchJobRepositoryPort>,
}

impl BatchUrlImportService {
    /// Create a new batch URL import service
    ///
    /// # Arguments
    ///
    /// * `web_ingestion_service` - Service for ingesting web content (fetch + extract + chunk + embed + store)
    /// * `batch_job_repository` - Repository for batch job tracking
    pub fn new(
        web_ingestion_service: Arc<dyn WebIngestionServiceTrait>,
        batch_job_repository: Arc<dyn BatchJobRepositoryPort>,
    ) -> Self {
        Self {
            web_ingestion_service,
            batch_job_repository,
        }
    }

    /// Process batch in background (spawned async task)
    ///
    /// This method runs in a separate Tokio task to avoid blocking the caller.
    /// It processes each URL sequentially, updating progress after each item.
    ///
    /// # Process Flow
    ///
    /// 1. Update job status to "running"
    /// 2. Get pending items from repository
    /// 3. For each item:
    ///    - Update item status to "processing"
    ///    - Extract article and create document
    ///    - Update item status to "completed" or "failed"
    ///    - Update job progress counters
    /// 4. Update job status to "completed" or "failed"
    ///
    /// # Error Handling
    ///
    /// Individual URL failures don't stop the batch. Each failure is logged
    /// and tracked, but processing continues with the next URL.
    async fn process_batch_async(
        web_ingestion_service: Arc<dyn WebIngestionServiceTrait>,
        batch_job_repository: Arc<dyn BatchJobRepositoryPort>,
        job_id: String,
    ) -> Result<(), AppError> {
        tracing::info!("Starting batch job {}", job_id);

        // Update status to running
        batch_job_repository
            .update_job_status(&job_id, "running", Some(Utc::now().to_rfc3339()), None)
            .await?;

        let mut completed = 0i64;
        let mut failed = 0i64;

        // Get pending items
        let items = batch_job_repository.get_pending_items(&job_id).await?;
        let total = items.len() as i64;

        // Process each item sequentially
        for item in items {
            // Update item status to processing
            batch_job_repository
                .update_item_status(&item.id, "processing", None, None)
                .await?;

            // Process URL
            match Self::process_single_url(web_ingestion_service.clone(), &item.url).await {
                Ok(document_id) => {
                    // Success
                    batch_job_repository
                        .update_item_status(&item.id, "completed", Some(&document_id), None)
                        .await?;
                    completed += 1;

                    // Audit successful URL import
                    let audit_logger = get_audit_logger();
                    let event =
                        AuditEvent::new(AuditAction::WebContentAccessed, AuditResult::success())
                            .with_resource_id(&item.url)
                            .with_metadata("resource_type", "batch_url")
                            .with_metadata("document_id", &document_id);
                    if let Err(e) = audit_logger.log(event).await {
                        tracing::warn!("Failed to write audit log: {}", e);
                    }
                }
                Err(e) => {
                    // Failure - continue with next item
                    tracing::error!("Failed to process {}: {}", item.url, e);
                    batch_job_repository
                        .update_item_status(&item.id, "failed", None, Some(&e.to_string()))
                        .await?;
                    failed += 1;

                    // Audit failed URL import
                    let audit_logger = get_audit_logger();
                    let event = AuditEvent::new(
                        AuditAction::WebContentAccessed,
                        AuditResult::failure(e.to_string()),
                    )
                    .with_resource_id(&item.url)
                    .with_metadata("resource_type", "batch_url");
                    if let Err(e) = audit_logger.log(event).await {
                        tracing::warn!("Failed to write audit log: {}", e);
                    }
                }
            }

            // Update progress
            let progress = ((completed + failed) as f64 / total as f64) * 100.0;
            batch_job_repository
                .update_progress(&job_id, completed, failed, progress)
                .await?;
        }

        // Update final status
        let final_status = if failed == total {
            "failed"
        } else {
            "completed"
        };

        batch_job_repository
            .update_job_status(&job_id, final_status, None, Some(Utc::now().to_rfc3339()))
            .await?;

        tracing::info!(
            "Batch job {} completed: {}/{} succeeded",
            job_id,
            completed,
            total
        );
        Ok(())
    }

    /// Process a single URL: delegate to WebIngestionService
    ///
    /// This method delegates the complete ingestion pipeline to WebIngestionService:
    /// 1. Fetch web content
    /// 2. Extract and clean text
    /// 3. Chunk content semantically
    /// 4. Generate embeddings
    /// 5. Store document, chunks, and embeddings atomically
    ///
    /// # Arguments
    ///
    /// * `web_ingestion_service` - Service for complete web ingestion pipeline
    /// * `url` - The URL to process
    ///
    /// # Returns
    ///
    /// The created document ID on success
    ///
    /// # Errors
    ///
    /// - `AppError::Network` if URL fetch fails
    /// - `AppError::Other` if content extraction fails
    /// - `AppError::EmbeddingFailed` if embedding generation fails
    /// - `AppError::Database` if storage fails
    async fn process_single_url(
        web_ingestion_service: Arc<dyn WebIngestionServiceTrait>,
        url: &str,
    ) -> Result<String, AppError> {
        // Delegate to web ingestion service (handles fetch + extract + chunk + embed + store)
        tracing::debug!("Ingesting web content from {}", url);
        let result = web_ingestion_service.ingest_url(url).await?;

        tracing::info!(
            "Successfully imported article from {}: {} ({} chunks from {} words)",
            url,
            result.title,
            result.chunks_created,
            result.word_count
        );

        Ok(result.document_id)
    }
}

#[async_trait]
impl BatchUrlImportServiceTrait for BatchUrlImportService {
    async fn start_batch_import(
        &self,
        urls: Vec<String>,
        options: Option<String>,
    ) -> Result<String, AppError> {
        let job_id = Uuid::new_v4().to_string();
        let total_items = urls.len() as i64;

        tracing::info!("Creating batch job {} with {} URLs", job_id, total_items);

        // 1. Create batch job (status: pending)
        self.batch_job_repository
            .create_batch_job(&job_id, "url_import", total_items, options.as_deref())
            .await?;

        // 2. Create batch items
        self.batch_job_repository
            .create_batch_items(&job_id, urls.clone())
            .await?;

        // 3. Spawn background task for processing
        let web_ingestion_service = Arc::clone(&self.web_ingestion_service);
        let batch_job_repository = Arc::clone(&self.batch_job_repository);
        let job_id_clone = job_id.clone();

        tokio::spawn(async move {
            if let Err(e) = Self::process_batch_async(
                web_ingestion_service,
                batch_job_repository,
                job_id_clone.clone(),
            )
            .await
            {
                tracing::error!("Batch job {} failed: {}", job_id_clone, e);
            }
        });

        // Audit batch job creation
        let audit_logger = get_audit_logger();
        let event = AuditEvent::new(AuditAction::WebContentAccessed, AuditResult::success())
            .with_resource_id(&job_id)
            .with_metadata("resource_type", "batch_job")
            .with_metadata("total_items", total_items.to_string());
        if let Err(e) = audit_logger.log(event).await {
            tracing::warn!("Failed to write audit log: {}", e);
        }

        tracing::info!("Batch job {} started in background", job_id);

        Ok(job_id)
    }

    async fn get_batch_status(&self, job_id: &str) -> Result<BatchJobStatus, AppError> {
        tracing::debug!("Getting status for batch job {}", job_id);
        self.batch_job_repository.get_batch_job(job_id).await
    }

    async fn cancel_batch_job(&self, job_id: &str) -> Result<usize, AppError> {
        tracing::info!("Cancelling batch job {}", job_id);

        let cancelled = self
            .batch_job_repository
            .cancel_pending_items(job_id)
            .await?;

        self.batch_job_repository
            .update_job_status(job_id, "cancelled", None, Some(Utc::now().to_rfc3339()))
            .await?;

        tracing::info!("Cancelled {} pending items for job {}", cancelled, job_id);

        Ok(cancelled)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::DocumentRepositoryPort;
    use crate::features::web::mocks::MockWebIngestionService;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Mock BatchJobRepository for testing
    struct MockBatchJobRepository {
        jobs: Arc<Mutex<HashMap<String, BatchJobStatus>>>,
    }

    impl MockBatchJobRepository {
        fn new() -> Self {
            Self {
                jobs: Arc::new(Mutex::new(HashMap::new())),
            }
        }
    }

    #[async_trait]
    impl BatchJobRepositoryPort for MockBatchJobRepository {
        async fn create_batch_job(
            &self,
            job_id: &str,
            _job_type: &str,
            total_items: i64,
            _options: Option<&str>,
        ) -> Result<(), AppError> {
            let status = BatchJobStatus {
                id: job_id.to_string(),
                job_type: "url_import".to_string(),
                status: "pending".to_string(),
                total_items,
                completed_items: 0,
                failed_items: 0,
                progress: 0.0,
                created_at: Utc::now().to_rfc3339(),
                started_at: None,
                completed_at: None,
                error_message: None,
                items: vec![],
            };
            self.jobs
                .lock()
                .map_err(|e| AppError::InternalError(format!("Failed to lock jobs map: {}", e)))?
                .insert(job_id.to_string(), status);
            Ok(())
        }

        async fn create_batch_items(
            &self,
            _job_id: &str,
            _urls: Vec<String>,
        ) -> Result<(), AppError> {
            Ok(())
        }

        async fn update_job_status(
            &self,
            job_id: &str,
            status: &str,
            started_at: Option<String>,
            completed_at: Option<String>,
        ) -> Result<(), AppError> {
            let mut jobs = self
                .jobs
                .lock()
                .map_err(|e| AppError::InternalError(format!("Failed to lock jobs map: {}", e)))?;
            if let Some(job) = jobs.get_mut(job_id) {
                job.status = status.to_string();
                if let Some(started) = started_at {
                    job.started_at = Some(started);
                }
                if let Some(completed) = completed_at {
                    job.completed_at = Some(completed);
                }
            }
            Ok(())
        }

        async fn update_progress(
            &self,
            job_id: &str,
            completed: i64,
            failed: i64,
            progress: f64,
        ) -> Result<(), AppError> {
            let mut jobs = self
                .jobs
                .lock()
                .map_err(|e| AppError::InternalError(format!("Failed to lock jobs map: {}", e)))?;
            if let Some(job) = jobs.get_mut(job_id) {
                job.completed_items = completed;
                job.failed_items = failed;
                job.progress = progress;
            }
            Ok(())
        }

        async fn update_item_status(
            &self,
            _item_id: &str,
            _status: &str,
            _document_id: Option<&str>,
            _error_message: Option<&str>,
        ) -> Result<(), AppError> {
            Ok(())
        }

        async fn get_batch_job(&self, job_id: &str) -> Result<BatchJobStatus, AppError> {
            self.jobs
                .lock()
                .map_err(|e| AppError::InternalError(format!("Failed to lock jobs map: {}", e)))?
                .get(job_id)
                .cloned()
                .ok_or_else(|| AppError::NotFound(format!("Job not found: {}", job_id)))
        }

        async fn get_pending_items(
            &self,
            _job_id: &str,
        ) -> Result<Vec<crate::application::ports::batch_job_repository_port::BatchJobItem>, AppError>
        {
            Ok(vec![])
        }

        async fn cancel_pending_items(&self, _job_id: &str) -> Result<usize, AppError> {
            Ok(0)
        }

        async fn list_batch_jobs(
            &self,
            _limit: Option<i64>,
            _offset: Option<i64>,
        ) -> Result<
            Vec<crate::application::ports::batch_job_repository_port::BatchJobSummary>,
            AppError,
        > {
            Ok(vec![])
        }

        async fn delete_batch_job(&self, job_id: &str) -> Result<(), AppError> {
            self.jobs
                .lock()
                .map_err(|e| AppError::InternalError(format!("Failed to lock jobs map: {}", e)))?
                .remove(job_id);
            Ok(())
        }
    }

    /// Mock DocumentRepository for testing
    struct MockDocumentRepository;

    #[async_trait]
    impl
        crate::application::ports::repository_port::RepositoryPort<
            crate::domain::entities::Document,
        > for MockDocumentRepository
    {
        async fn find_by_id(
            &self,
            _id: &str,
        ) -> Result<Option<crate::domain::entities::Document>, AppError> {
            Ok(None)
        }

        async fn find_by_filter(
            &self,
            _filter: &dyn crate::application::ports::repository_port::Filter,
        ) -> Result<Vec<crate::domain::entities::Document>, AppError> {
            Ok(vec![])
        }

        async fn find_all(&self) -> Result<Vec<crate::domain::entities::Document>, AppError> {
            Ok(vec![])
        }

        async fn save(&self, _entity: &crate::domain::entities::Document) -> Result<(), AppError> {
            Ok(())
        }

        async fn save_batch(
            &self,
            _entities: &[crate::domain::entities::Document],
        ) -> Result<(), AppError> {
            Ok(())
        }

        async fn delete(&self, _id: &str) -> Result<(), AppError> {
            Ok(())
        }

        async fn delete_batch(&self, _ids: &[&str]) -> Result<(), AppError> {
            Ok(())
        }

        async fn count(&self) -> Result<usize, AppError> {
            Ok(0)
        }

        async fn exists(&self, _id: &str) -> Result<bool, AppError> {
            Ok(false)
        }
    }

    #[async_trait]
    impl DocumentRepositoryPort for MockDocumentRepository {
        async fn find_file_path_by_id(&self, _document_id: &str) -> Result<String, AppError> {
            Ok("/test/path".to_string())
        }

        async fn document_exists(&self, _document_id: &str) -> Result<bool, AppError> {
            Ok(false)
        }

        async fn find_id_by_path(&self, _file_path: &str) -> Result<Option<String>, AppError> {
            Ok(None)
        }

        async fn delete(&self, _document_id: &str) -> Result<(), AppError> {
            Ok(())
        }

        async fn find_by_checksum(
            &self,
            _checksum: &crate::domain::value_objects::Checksum,
        ) -> Result<Option<crate::domain::entities::Document>, AppError> {
            Ok(None)
        }

        async fn count_documents(&self) -> Result<i64, AppError> {
            Ok(0)
        }

        async fn count_chunks(&self) -> Result<i64, AppError> {
            Ok(0)
        }
    }

    #[tokio::test]
    async fn test_start_batch_import_creates_job() {
        let web_ingestion_service = Arc::new(MockWebIngestionService::new());
        let batch_job_repository = Arc::new(MockBatchJobRepository::new());

        let service =
            BatchUrlImportService::new(web_ingestion_service, batch_job_repository.clone());

        let urls = vec!["https://example.com/article".to_string()];
        let job_id = service.start_batch_import(urls, None).await.unwrap();

        assert!(!job_id.is_empty());

        // Verify job was created
        let status = batch_job_repository.get_batch_job(&job_id).await.unwrap();
        assert_eq!(status.total_items, 1);
        assert_eq!(status.status, "pending");
    }

    #[tokio::test]
    async fn test_get_batch_status() {
        let web_ingestion_service = Arc::new(MockWebIngestionService::new());
        let batch_job_repository = Arc::new(MockBatchJobRepository::new());

        let service =
            BatchUrlImportService::new(web_ingestion_service, batch_job_repository.clone());

        let urls = vec!["https://example.com/article".to_string()];
        let job_id = service.start_batch_import(urls, None).await.unwrap();

        let status = service.get_batch_status(&job_id).await.unwrap();
        assert_eq!(status.id, job_id);
    }

    #[tokio::test]
    async fn test_cancel_batch_job() {
        let web_ingestion_service = Arc::new(MockWebIngestionService::new());
        let batch_job_repository = Arc::new(MockBatchJobRepository::new());

        let service =
            BatchUrlImportService::new(web_ingestion_service, batch_job_repository.clone());

        let urls = vec!["https://example.com/article".to_string()];
        let job_id = service.start_batch_import(urls, None).await.unwrap();

        let cancelled = service.cancel_batch_job(&job_id).await.unwrap();
        assert_eq!(cancelled, 0); // Mock returns 0

        let status = batch_job_repository.get_batch_job(&job_id).await.unwrap();
        assert_eq!(status.status, "cancelled");
    }
}
