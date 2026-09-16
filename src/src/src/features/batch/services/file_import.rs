//! Batch File Import Service Implementation
//!
//! Orchestrates batch file import workflow: create job → process files → track progress.
//! Uses the "bricks and studs" philosophy with clear service boundaries.

use async_trait::async_trait;
use std::fs;
use std::sync::Arc;
use tokio::task;
use uuid::Uuid;

use super::file_import_trait::{BatchFileImportServiceTrait, ProcessedFileInfo};
use crate::application::ports::batch_job_repository_port::BatchJobRepositoryPort;
use crate::domain::value_objects::file_metadata::FileMetadata;
use crate::features::indexing::dto::{ChunkingStrategyDto, IndexFileRequestDto};
use crate::features::indexing::use_cases::index_file::IndexFileUseCase;
use crate::infrastructure::indexing::extraction::ContentExtractor;
use crate::shared::domain_types::ValidatedFilePath;
use crate::shared::error::AppError;

/// Maximum file size: 50MB
const MAX_FILE_SIZE: i64 = 50 * 1024 * 1024;

/// Service for batch file import operations
///
/// Orchestrates the entire batch workflow:
/// 1. Create batch job in database (status: pending)
/// 2. Spawn background Tokio task for processing
/// 3. Process each file: validate → read → create document
/// 4. Update progress after each file
/// 5. Mark job as completed/failed
///
/// # Architecture
///
/// - **Dependencies**: BatchJobRepository for job tracking
/// - **Async Processing**: Background task with Tokio
/// - **Error Handling**: Individual failures don't stop batch
/// - **Progress Tracking**: Real-time updates via repository
///
/// # Security
///
/// - All paths validated with ValidatedFilePath (prevents CWE-22)
/// - Extension/capability validation against extractor registry
/// - Size limits enforced (prevents resource exhaustion CWE-770)
/// - Rate limiting applied at command layer
///
/// # Example
///
/// ```rust,no_run
/// use lattice::infrastructure::services::batch_file_import::BatchFileImportService;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let service = BatchFileImportService::new(batch_job_repository);
///
/// let job_id = service.start_batch_import(vec![
///     ValidatedFilePath::new("/path/to/file1.pdf")?,
///     ValidatedFilePath::new("/path/to/file2.txt")?,
/// ]).await?;
///
/// // Job runs in background, check status later
/// # Ok(())
/// # }
/// ```
pub struct BatchFileImportService {
    /// Repository for batch job tracking
    batch_job_repo: Arc<dyn BatchJobRepositoryPort>,
    /// Use case for indexing files
    index_file_use_case: Arc<IndexFileUseCase>,
}

impl BatchFileImportService {
    /// Create a new batch file import service
    ///
    /// # Arguments
    ///
    /// * `batch_job_repo` - Repository for batch job tracking
    /// * `index_file_use_case` - Use case for indexing files
    ///
    /// # Returns
    ///
    /// New service instance
    pub fn new(
        batch_job_repo: Arc<dyn BatchJobRepositoryPort>,
        index_file_use_case: Arc<IndexFileUseCase>,
    ) -> Self {
        Self {
            batch_job_repo,
            index_file_use_case,
        }
    }

    /// Spawn background task for batch processing
    ///
    /// This method runs in a separate Tokio task to avoid blocking the caller.
    /// It processes each file sequentially, updating progress after each item.
    ///
    /// # Process Flow
    ///
    /// 1. Update job status to "running"
    /// 2. Fetch pending batch items from database
    /// 3. Process each item:
    ///    - Update item status to "processing"
    ///    - Call IndexFileUseCase to actually index the file
    ///    - Update item status to "completed" with document_id
    ///    - Track failures and update item status to "failed"
    /// 4. Update job status to "completed" or "failed"
    ///
    /// # Error Handling
    ///
    /// Individual file failures don't stop the batch. Each failure is logged
    /// and tracked, but processing continues with the next file.
    ///
    /// # Arguments
    ///
    /// * `batch_job_repo` - Repository for job tracking
    /// * `index_file_use_case` - Use case for indexing files
    /// * `job_id` - Job ID to process
    async fn spawn_processing_task(
        batch_job_repo: Arc<dyn BatchJobRepositoryPort>,
        index_file_use_case: Arc<IndexFileUseCase>,
        job_id: String,
    ) {
        tokio::spawn(async move {
            tracing::info!("Starting batch file import job {}", job_id);

            // Update job status to running
            if let Err(e) = batch_job_repo
                .update_job_status(
                    &job_id,
                    "running",
                    Some(chrono::Utc::now().to_rfc3339()),
                    None,
                )
                .await
            {
                tracing::error!("Failed to update job status: {}", e);
                return;
            }

            // Get pending items from database
            let items = match batch_job_repo.get_pending_items(&job_id).await {
                Ok(items) => items,
                Err(e) => {
                    tracing::error!("Failed to get pending items: {}", e);
                    if let Err(update_err) = batch_job_repo
                        .update_job_status(
                            &job_id,
                            "failed",
                            None,
                            Some(chrono::Utc::now().to_rfc3339()),
                        )
                        .await
                    {
                        tracing::error!(
                            "Failed to update job status after pending item error: {}",
                            update_err
                        );
                    }
                    return;
                }
            };

            let total = items.len() as i64;
            let mut completed = 0i64;
            let mut failed = 0i64;

            for item in items {
                // Check if job was cancelled
                if let Ok(job) = batch_job_repo.get_batch_job(&job_id).await {
                    if job.status == "cancelled" {
                        tracing::info!("Job {} was cancelled", job_id);
                        break;
                    }
                }

                let file_path = &item.url; // URL field stores file path
                let item_id = &item.id;

                // Update item status to processing
                if let Err(e) = batch_job_repo
                    .update_item_status(item_id, "processing", None, None)
                    .await
                {
                    tracing::error!(
                        "Failed to update item {} status to processing: {}",
                        item_id,
                        e
                    );
                    failed += 1;
                    continue;
                }

                // Process file with IndexFileUseCase
                match Self::process_file_with_indexing(Arc::clone(&index_file_use_case), file_path)
                    .await
                {
                    Ok(document_id) => {
                        completed += 1;
                        tracing::info!(
                            "Successfully indexed file: {} (doc: {})",
                            file_path,
                            document_id
                        );

                        // Update item status to completed with document_id
                        if let Err(e) = batch_job_repo
                            .update_item_status(item_id, "completed", Some(&document_id), None)
                            .await
                        {
                            tracing::error!(
                                "Failed to update item {} status to completed: {}",
                                item_id,
                                e
                            );
                        }
                    }
                    Err(e) => {
                        failed += 1;
                        let error_msg = format!("Failed to index file {}: {}", file_path, e);
                        tracing::error!("{}", error_msg);

                        // Update item status to failed with error message
                        if let Err(e) = batch_job_repo
                            .update_item_status(item_id, "failed", None, Some(&error_msg))
                            .await
                        {
                            tracing::error!(
                                "Failed to update item {} status to failed: {}",
                                item_id,
                                e
                            );
                        }
                    }
                }

                // Update job progress
                let progress = (completed + failed) as f64 / total as f64;
                if let Err(e) = batch_job_repo
                    .update_progress(&job_id, completed, failed, progress)
                    .await
                {
                    tracing::error!("Failed to update progress: {}", e);
                }
            }

            // Mark job complete (status is "completed" even if some files failed)
            let final_status = if failed == total {
                "failed"
            } else {
                "completed"
            };

            if let Err(e) = batch_job_repo
                .update_job_status(
                    &job_id,
                    final_status,
                    None,
                    Some(chrono::Utc::now().to_rfc3339()),
                )
                .await
            {
                tracing::error!("Failed to mark job complete: {}", e);
            }

            tracing::info!(
                "Batch file import job {} completed: {}/{} succeeded, {}/{} failed",
                job_id,
                completed,
                total,
                failed,
                total
            );
        });
    }

    /// Process a file by calling IndexFileUseCase to actually index it
    ///
    /// This replaces the previous stub implementation that returned fake UUIDs.
    /// Now files are properly indexed with chunking and embeddings.
    ///
    /// # Arguments
    ///
    /// * `index_file_use_case` - Use case for indexing files
    /// * `file_path` - Path to the file to index
    ///
    /// # Returns
    ///
    /// Document ID of the indexed file
    ///
    /// # Errors
    ///
    /// - `AppError` if indexing fails
    async fn process_file_with_indexing(
        index_file_use_case: Arc<IndexFileUseCase>,
        file_path: &str,
    ) -> Result<String, AppError> {
        let request = IndexFileRequestDto {
            path: file_path.to_string(),
            chunking_strategy: ChunkingStrategyDto::Semantic { max_tokens: 800 },
            tags: None,
            metadata: None,
            space_id: None,
        };

        let response = index_file_use_case.execute(request).await?;

        Ok(response.document_id)
    }
}

#[async_trait]
impl BatchFileImportServiceTrait for BatchFileImportService {
    async fn start_batch_import(
        &self,
        file_paths: Vec<ValidatedFilePath>,
    ) -> Result<String, AppError> {
        // Validate all files first (fail fast)
        for path in &file_paths {
            self.validate_file(path)?;
        }

        // Create batch job
        let job_id = Uuid::new_v4().to_string();
        let total_items = file_paths.len() as i64;

        tracing::info!(
            "Creating batch file import job {} with {} files",
            job_id,
            total_items
        );

        self.batch_job_repo
            .create_batch_job(&job_id, "file_import", total_items, None)
            .await?;

        // Create batch items (store file paths as URLs)
        let urls: Vec<String> = file_paths
            .iter()
            .map(|p| p.as_path().to_string_lossy().to_string())
            .collect();

        self.batch_job_repo
            .create_batch_items(&job_id, urls)
            .await?;

        // Spawn background processing task
        Self::spawn_processing_task(
            Arc::clone(&self.batch_job_repo),
            Arc::clone(&self.index_file_use_case),
            job_id.clone(),
        )
        .await;

        tracing::info!("Batch file import job {} started in background", job_id);

        Ok(job_id)
    }

    fn validate_file(&self, path: &ValidatedFilePath) -> Result<FileMetadata, AppError> {
        #[allow(deprecated)]
        let metadata = FileMetadata::from_path(path.as_path())?;

        // Validate file type using the same extraction capability registry used by indexing.
        if !ContentExtractor::new().is_supported(path.as_path()) {
            return Err(AppError::InvalidInput(format!(
                "Unsupported file type: {}. This file extension is not supported for indexing.",
                metadata.mime_type()
            )));
        }

        // Validate size (max 50MB)
        if metadata.size_bytes() > MAX_FILE_SIZE {
            return Err(AppError::InvalidInput(format!(
                "File too large: {} bytes (max {} bytes)",
                metadata.size_bytes(),
                MAX_FILE_SIZE
            )));
        }

        Ok(metadata)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::batch_job_repository_port::{BatchJobItem, BatchJobStatus};
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
                job_type: "file_import".to_string(),
                status: "pending".to_string(),
                total_items,
                completed_items: 0,
                failed_items: 0,
                progress: 0.0,
                created_at: chrono::Utc::now().to_rfc3339(),
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

        async fn get_pending_items(&self, _job_id: &str) -> Result<Vec<BatchJobItem>, AppError> {
            Ok(vec![])
        }

        async fn cancel_pending_items(&self, _job_id: &str) -> Result<usize, AppError> {
            Ok(0)
        }

        async fn list_batch_jobs(
            &self,
            limit: Option<i64>,
            offset: Option<i64>,
        ) -> Result<
            Vec<crate::application::ports::batch_job_repository_port::BatchJobSummary>,
            AppError,
        > {
            let jobs: Vec<_> = self
                .jobs
                .lock()
                .map_err(|e| AppError::InternalError(format!("Failed to lock jobs map: {}", e)))?
                .values()
                .map(|status| {
                    crate::application::ports::batch_job_repository_port::BatchJobSummary {
                        id: status.id.clone(),
                        job_type: "import".to_string(),
                        status: status.status.clone(),
                        total_items: status.total_items,
                        completed_items: status.completed_items,
                        failed_items: status.failed_items,
                        progress: status.progress,
                        created_at: status.created_at.clone(),
                        completed_at: status.completed_at.clone(),
                    }
                })
                .collect();

            let offset = offset.unwrap_or(0) as usize;
            let jobs: Vec<_> = jobs.into_iter().skip(offset).collect();

            if let Some(limit) = limit {
                Ok(jobs.into_iter().take(limit as usize).collect())
            } else {
                Ok(jobs)
            }
        }

        async fn delete_batch_job(&self, job_id: &str) -> Result<(), AppError> {
            self.jobs
                .lock()
                .map_err(|e| AppError::InternalError(format!("Failed to lock jobs map: {}", e)))?
                .remove(job_id);
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_start_batch_import_creates_job() {
        let batch_job_repo = Arc::new(MockBatchJobRepository::new());
        let mock_index_use_case = create_mock_index_file_use_case();
        let service =
            BatchFileImportService::new(batch_job_repo.clone(), Arc::new(mock_index_use_case));

        // Create temp file for testing
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "test content").unwrap();

        let paths = vec![ValidatedFilePath::new(file_path).unwrap()];

        let job_id = service.start_batch_import(paths).await.unwrap();

        assert!(!job_id.is_empty());

        // Verify job was created
        let status = batch_job_repo.get_batch_job(&job_id).await.unwrap();
        assert_eq!(status.total_items, 1);
        assert_eq!(status.status, "pending");
    }

    #[test]
    fn test_validate_file_success() {
        let batch_job_repo = Arc::new(MockBatchJobRepository::new());
        let mock_index_use_case = create_mock_index_file_use_case();
        let service = BatchFileImportService::new(batch_job_repo, Arc::new(mock_index_use_case));

        // Create temp file
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "test content").unwrap();

        let path = ValidatedFilePath::new(file_path).unwrap();

        let result = service.validate_file(&path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_file_unsupported_type() {
        let batch_job_repo = Arc::new(MockBatchJobRepository::new());
        let mock_index_use_case = create_mock_index_file_use_case();
        let service = BatchFileImportService::new(batch_job_repo, Arc::new(mock_index_use_case));

        // Create temp file with unsupported extension
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.exe");
        std::fs::write(&file_path, "test content").unwrap();

        let path = ValidatedFilePath::new(file_path).unwrap();

        let result = service.validate_file(&path);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unsupported file type"));
    }

    // Helper to create mock IndexFileUseCase
    fn create_mock_index_file_use_case() -> IndexFileUseCase {
        use crate::application::ports::{
            EmbeddingPort, EmbeddingRepositoryPort, FileStoragePort, RepositoryPort,
        };
        use crate::domain::entities::document::Document;
        use crate::domain::repositories::UnitOfWorkFactory;
        use crate::features::embedding::entity::Embedding;
        use crate::shared::result::Result as AppResult;
        use std::path::Path;

        struct MockFileStorage;
        #[async_trait]
        impl FileStoragePort for MockFileStorage {
            async fn read_file(&self, _path: &Path) -> Result<String, AppError> {
                Ok("Mock content".to_string())
            }
            async fn write_file(&self, _path: &Path, _content: &str) -> Result<(), AppError> {
                Ok(())
            }
            async fn delete_file(&self, _path: &Path) -> Result<(), AppError> {
                Ok(())
            }
            async fn read_file_bytes(&self, _path: &Path) -> Result<Vec<u8>, AppError> {
                Ok(vec![])
            }
            async fn write_file_bytes(
                &self,
                _path: &Path,
                _content: &[u8],
            ) -> Result<(), AppError> {
                Ok(())
            }
            async fn compute_hash(&self, _path: &Path) -> Result<String, AppError> {
                Ok("hash".to_string())
            }
            async fn exists(&self, _path: &Path) -> bool {
                true
            }
            async fn metadata(
                &self,
                _path: &Path,
            ) -> Result<crate::application::ports::file_storage_port::FileMetadata, AppError>
            {
                use chrono::Utc;
                Ok(crate::application::ports::file_storage_port::FileMetadata {
                    size: 100,
                    modified_at: Utc::now().timestamp(),
                    is_file: true,
                    is_directory: false,
                })
            }
        }

        #[async_trait]
        impl crate::application::ports::ContentAddressedStoragePort for MockFileStorage {
            async fn import_file(
                &self,
                _source_path: &Path,
            ) -> Result<(std::path::PathBuf, String), AppError> {
                Ok((std::path::PathBuf::from("/mock/path"), "hash".to_string()))
            }
            async fn exists_by_hash(&self, _hash: &str) -> Result<bool, AppError> {
                Ok(true)
            }
            async fn get_path_by_hash(
                &self,
                _hash: &str,
            ) -> Result<Option<std::path::PathBuf>, AppError> {
                Ok(None)
            }
        }

        struct MockContentExtractor;
        #[async_trait]
        impl crate::application::ports::ContentExtractionPort for MockContentExtractor {
            async fn extract_content(
                &self,
                _path: &Path,
            ) -> Result<
                crate::application::ports::content_extraction_port::ExtractedContentData,
                AppError,
            > {
                Ok(
                    crate::application::ports::content_extraction_port::ExtractedContentData {
                        text: "Mock content".to_string(),
                        mime_type: "text/plain".to_string(),
                        page_count: Some(1),
                        word_count: 2,
                        char_count: 12,
                    },
                )
            }
            fn is_supported(&self, _path: &Path) -> bool {
                true
            }
        }

        struct MockEmbedding;
        #[async_trait]
        impl EmbeddingPort for MockEmbedding {
            async fn embed_single(&self, _text: &str) -> Result<Vec<f32>, AppError> {
                Ok(vec![0.1])
            }
            async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AppError> {
                Ok(texts.iter().map(|_| vec![0.1]).collect())
            }
            fn dimension(&self) -> usize {
                1
            }
            async fn is_ready(&self) -> Result<bool, AppError> {
                Ok(true)
            }
        }

        struct MockDocRepo;
        #[async_trait]
        impl RepositoryPort<Document> for MockDocRepo {
            async fn save(&self, _entity: &Document) -> Result<(), AppError> {
                Ok(())
            }
            async fn find_by_id(&self, _id: &str) -> Result<Option<Document>, AppError> {
                Ok(None)
            }
            async fn find_all(&self) -> Result<Vec<Document>, AppError> {
                Ok(vec![])
            }
            async fn exists(&self, _id: &str) -> Result<bool, AppError> {
                Ok(false)
            }
            async fn find_by_filter(
                &self,
                _filter: &dyn crate::application::ports::repository_port::Filter,
            ) -> Result<Vec<Document>, AppError> {
                Ok(vec![])
            }
            async fn save_batch(&self, _entities: &[Document]) -> Result<(), AppError> {
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
        }

        #[async_trait]
        impl crate::application::ports::DocumentRepositoryPort for MockDocRepo {
            async fn find_by_checksum(
                &self,
                _checksum: &crate::domain::value_objects::Checksum,
            ) -> Result<Option<Document>, AppError> {
                Ok(None)
            }
            async fn count_documents(&self) -> Result<i64, AppError> {
                Ok(0)
            }
            async fn count_chunks(&self) -> Result<i64, AppError> {
                Ok(0)
            }
            async fn find_file_path_by_id(&self, _document_id: &str) -> Result<String, AppError> {
                Ok(String::new())
            }
            async fn find_id_by_path(&self, _file_path: &str) -> Result<Option<String>, AppError> {
                Ok(None)
            }
            async fn document_exists(&self, _document_id: &str) -> Result<bool, AppError> {
                Ok(false)
            }
            async fn delete(&self, _document_id: &str) -> Result<(), AppError> {
                Ok(())
            }
            async fn find_all_paginated(&self, _limit: usize) -> Result<Vec<Document>, AppError> {
                Ok(vec![])
            }
        }

        struct MockEmbedRepo;
        #[async_trait]
        impl EmbeddingRepositoryPort for MockEmbedRepo {
            async fn create(
                &self,
                _chunk_id: &str,
                _vector: &[f32],
                _model: &str,
            ) -> Result<String, AppError> {
                unimplemented!()
            }
            async fn find_by_chunk(&self, _chunk_id: &str) -> Result<Option<Embedding>, AppError> {
                unimplemented!()
            }
            async fn save(&self, _entity: &Embedding, _vector: Vec<f32>) -> Result<(), AppError> {
                Ok(())
            }
            async fn save_batch(
                &self,
                _entries: Vec<(Embedding, Vec<f32>)>,
            ) -> Result<(), AppError> {
                Ok(())
            }
            async fn find_by_chunk_id(
                &self,
                _chunk_id: &str,
            ) -> Result<Option<(Embedding, Vec<f32>)>, AppError> {
                Ok(None)
            }
            async fn find_by_document_id(
                &self,
                _document_id: &str,
            ) -> Result<Vec<(Embedding, Vec<f32>)>, AppError> {
                Ok(vec![])
            }
            async fn delete_by_chunk_id(&self, _chunk_id: &str) -> Result<(), AppError> {
                Ok(())
            }
            async fn delete_by_document_id(&self, _document_id: &str) -> Result<(), AppError> {
                Ok(())
            }
            async fn count(&self) -> Result<i64, AppError> {
                Ok(0)
            }
            async fn create_batch(
                &self,
                entries: Vec<(Embedding, Vec<f32>)>,
            ) -> Result<Vec<String>, AppError> {
                Ok(entries
                    .iter()
                    .enumerate()
                    .map(|(i, _)| format!("emb_{}", i))
                    .collect())
            }
        }

        struct MockUnitOfWork;

        #[async_trait]
        impl crate::domain::repositories::UnitOfWork for MockUnitOfWork {
            fn chunk_repository(
                &self,
            ) -> AppResult<Box<dyn crate::application::ports::ChunkRepositoryPort + Send + '_>>
            {
                Err(AppError::InvalidState(
                    "Chunk repository not used in test".to_string(),
                ))
            }

            fn document_repository(
                &self,
            ) -> AppResult<Box<dyn crate::application::ports::DocumentRepositoryPort + Send + '_>>
            {
                Ok(Box::new(MockDocRepo))
            }

            fn embedding_repository(
                &self,
            ) -> AppResult<Box<dyn EmbeddingRepositoryPort + Send + '_>> {
                Ok(Box::new(MockEmbedRepo))
            }

            fn search_repository(
                &self,
            ) -> AppResult<Box<dyn crate::domain::repositories::SearchRepository + Send + '_>>
            {
                Err(AppError::InvalidState(
                    "Search repository not used in test".to_string(),
                ))
            }

            fn batch_job_repository(
                &self,
            ) -> AppResult<Box<dyn crate::application::ports::BatchJobRepositoryPort + Send + '_>>
            {
                Err(AppError::InvalidState(
                    "Batch job repository not used in test".to_string(),
                ))
            }

            fn system_repository(
                &self,
            ) -> AppResult<Box<dyn crate::domain::repositories::SystemRepository + Send + '_>>
            {
                Err(AppError::InvalidState(
                    "System repository not used in test".to_string(),
                ))
            }

            fn model_repository(
                &self,
            ) -> AppResult<
                Box<dyn crate::domain::repositories::unit_of_work::ModelRepositoryPort + '_>,
            > {
                Err(AppError::InvalidState(
                    "Model repository not used in test".to_string(),
                ))
            }

            fn model_file_repository(
                &self,
            ) -> AppResult<
                Box<dyn crate::domain::repositories::unit_of_work::ModelFileRepositoryPort + '_>,
            > {
                Err(AppError::InvalidState(
                    "Model file repository not used in test".to_string(),
                ))
            }

            async fn commit(&mut self) -> AppResult<()> {
                Ok(())
            }

            async fn rollback(&mut self) -> AppResult<()> {
                Ok(())
            }
        }

        struct MockUnitOfWorkFactory;

        #[async_trait]
        impl UnitOfWorkFactory for MockUnitOfWorkFactory {
            async fn create(
                &self,
            ) -> AppResult<Box<dyn crate::domain::repositories::UnitOfWork + Send>> {
                Ok(Box::new(MockUnitOfWork))
            }
        }

        IndexFileUseCase::new(
            Arc::new(MockFileStorage),
            Arc::new(MockFileStorage),
            Arc::new(MockContentExtractor),
            Arc::new(MockEmbedding),
            Arc::new(MockDocRepo),
            Arc::new(MockEmbedRepo),
            Arc::new(MockUnitOfWorkFactory),
        )
    }
}
