//! File imports prepare and commit one document at a time so progress is durable.
use crate::application::ports::{BatchJobRepositoryPort, UnitOfWorkFactory};
use crate::features::batch::dto::{
    StartBatchFileImportRequestDto, StartBatchFileImportResponseDto,
};
use crate::features::indexing::dto::{ChunkingStrategyDto, IndexFileRequestDto};
use crate::features::indexing::use_cases::{
    index_file::PrepareForIndexingOutcome, IndexFileUseCase,
};
use crate::shared::{
    domain_types::ValidatedFilePath,
    error::{AppError, Result},
};
use std::{path::PathBuf, sync::Arc};
use uuid::Uuid;

const MIN_BATCH_SIZE: usize = 1;
const MAX_BATCH_SIZE: usize = 100;

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
struct FileImportOptions {
    #[serde(default)]
    space_id: Option<String>,
    #[serde(default)]
    indexing: Option<crate::features::batch::dto::FileIndexingOptionsDto>,
    #[serde(default)]
    file_order: Vec<String>,
}

#[derive(Clone)]
pub struct StartBatchFileImportUseCase {
    batch_repo: Arc<dyn BatchJobRepositoryPort>,
    index_file_use_case: Arc<IndexFileUseCase>,
    uow_factory: Arc<dyn UnitOfWorkFactory>,
    document_scope: Option<Arc<dyn crate::application::ports::document_scope::DocumentScopePort>>,
}

impl StartBatchFileImportUseCase {
    pub fn new(
        batch_repo: Arc<dyn BatchJobRepositoryPort>,
        index_file_use_case: Arc<IndexFileUseCase>,
        uow_factory: Arc<dyn UnitOfWorkFactory>,
    ) -> Self {
        Self {
            batch_repo,
            index_file_use_case,
            uow_factory,
            document_scope: None,
        }
    }

    pub fn with_document_scope(
        mut self,
        scope: Arc<dyn crate::application::ports::document_scope::DocumentScopePort>,
    ) -> Self {
        self.document_scope = Some(scope);
        self
    }

    pub async fn execute(
        &self,
        request: StartBatchFileImportRequestDto,
    ) -> Result<StartBatchFileImportResponseDto> {
        let count = request.file_paths.len();
        if count < MIN_BATCH_SIZE {
            return Err(AppError::InvalidInput(
                "Batch must contain at least 1 file".into(),
            ));
        }
        if count > MAX_BATCH_SIZE {
            return Err(AppError::InvalidInput(format!(
                "Batch size {count} exceeds maximum of {MAX_BATCH_SIZE}"
            )));
        }
        for path in &request.file_paths {
            ValidatedFilePath::new(PathBuf::from(path))?;
        }
        if let Some(space_id) = &request.space_id {
            let scope = self.document_scope.as_ref().ok_or_else(|| {
                AppError::ServiceNotAvailable("Space assignment is unavailable".into())
            })?;
            if !scope.space_exists(space_id).await? {
                return Err(AppError::InvalidInput(format!(
                    "Space not found: {space_id}"
                )));
            }
        }
        if let Some(group) = request
            .indexing
            .as_ref()
            .and_then(|i| i.source_group.as_ref())
        {
            group.validate()?;
        }
        let unique: std::collections::HashSet<_> = request.file_paths.iter().collect();
        if request
            .indexing
            .as_ref()
            .and_then(|i| i.source_group.as_ref())
            .is_some()
            && unique.len() != count
        {
            return Err(AppError::InvalidInput(
                "The same file was selected more than once".into(),
            ));
        }
        let job_id = Uuid::new_v4().to_string();
        let options = serde_json::to_string(&FileImportOptions {
            space_id: request.space_id,
            indexing: request.indexing,
            file_order: request.file_paths.clone(),
        })?;
        self.batch_repo
            .create_batch_job(&job_id, "file_import", count as i64, Some(&options))
            .await?;
        self.batch_repo
            .create_batch_items(&job_id, request.file_paths)
            .await?;
        self.spawn_job(job_id.clone());
        Ok(StartBatchFileImportResponseDto { job_id })
    }

    pub async fn retry_failed(
        &self,
        job_id: &str,
        item_id: Option<&str>,
        replacement_path: Option<&str>,
    ) -> Result<usize> {
        if let Some(path) = replacement_path {
            ValidatedFilePath::new(PathBuf::from(path))?;
        }
        let count = self
            .batch_repo
            .requeue_failed_files(job_id, item_id, replacement_path)
            .await?;
        self.spawn_job(job_id.to_owned());
        Ok(count)
    }

    /// Called only during startup, before new imports can start. Completed files
    /// remain committed; an interrupted current file is safely deduplicated on retry.
    pub async fn resume_interrupted(&self, job_id: String) -> Result<()> {
        let job = self.batch_repo.get_batch_job(&job_id).await?;
        if job.job_type != "file_import" || !matches!(job.status.as_str(), "pending" | "running") {
            return Ok(());
        }
        for item in job
            .items
            .iter()
            .filter(|item| matches!(item.status.as_str(), "running" | "processing"))
        {
            self.batch_repo
                .update_item_status(&item.id, "pending", None, None)
                .await?;
        }
        self.spawn_job(job_id);
        Ok(())
    }

    fn spawn_job(&self, job_id: String) {
        let worker = self.clone();
        tokio::spawn(async move {
            if let Err(error) = worker.process_job(&job_id).await {
                tracing::error!(%job_id, %error, "Batch file import stopped");
                // Surface orchestration failures instead of leaving a job running forever.
                if let Ok(job) = worker.batch_repo.get_batch_job(&job_id).await {
                    if job.status == "cancelled" {
                        return;
                    }
                    let mut failed = job.failed_items;
                    for item in job.items.iter().filter(|item| {
                        matches!(item.status.as_str(), "pending" | "running" | "processing")
                    }) {
                        if worker
                            .batch_repo
                            .update_item_status(&item.id, "failed", None, Some(&error.to_string()))
                            .await
                            .is_ok()
                        {
                            failed += 1;
                        }
                    }
                    let progress =
                        (job.completed_items + failed) as f64 / job.total_items.max(1) as f64;
                    let _ = worker
                        .batch_repo
                        .update_progress(&job_id, job.completed_items, failed, progress)
                        .await;
                }
                let _ = worker
                    .batch_repo
                    .update_job_status(
                        &job_id,
                        "failed",
                        None,
                        Some(chrono::Utc::now().to_rfc3339()),
                    )
                    .await;
            }
        });
    }

    async fn process_job(&self, job_id: &str) -> Result<()> {
        let job = self.batch_repo.get_batch_job(job_id).await?;
        if job.status == "cancelled" {
            return Ok(());
        }
        self.batch_repo
            .update_job_status(
                job_id,
                "running",
                Some(chrono::Utc::now().to_rfc3339()),
                None,
            )
            .await?;
        let items = self.batch_repo.get_pending_items(job_id).await?;
        let options = self.batch_repo.get_job_options(job_id).await?;
        let options = options
            .map(|json| serde_json::from_str::<FileImportOptions>(&json))
            .transpose()?
            .unwrap_or_default();
        let space_id = options.space_id.clone();
        // Recompute from items: the app may have exited between committing an
        // item and updating the aggregate counters.
        let mut completed = job
            .items
            .iter()
            .filter(|item| item.status == "completed")
            .count() as i64;
        let mut failed = job
            .items
            .iter()
            .filter(|item| item.status == "failed")
            .count() as i64;
        let total = job.total_items.max(items.len() as i64).max(1);
        self.batch_repo
            .update_progress(
                job_id,
                completed,
                failed,
                (completed + failed) as f64 / total as f64,
            )
            .await?;
        for item in items {
            if self.batch_repo.get_batch_job(job_id).await?.status == "cancelled" {
                return Ok(());
            }
            self.batch_repo
                .update_item_status(&item.id, "processing", None, None)
                .await?;
            let request = IndexFileRequestDto {
                path: item.url.clone(),
                chunking_strategy: ChunkingStrategyDto::Semantic { max_tokens: 800 },
                tags: None,
                metadata: {
                    let mut metadata = std::collections::HashMap::new();
                    if let Some(indexing) = &options.indexing {
                        if let Some(group) = &indexing.source_group {
                            // item IDs are stable across retries; repository creation order is stable.
                            let position = options
                                .file_order
                                .iter()
                                .position(|path| path == &item.url)
                                .or_else(|| {
                                    job.items
                                        .iter()
                                        .position(|candidate| candidate.id == item.id)
                                })
                                .ok_or_else(|| {
                                    AppError::InvalidState(
                                        "Import item has no reading position".into(),
                                    )
                                })?;
                            let context =
                                crate::domain::value_objects::source_context::SourceContext {
                                    group: group.clone(),
                                    position: position as u32,
                                };
                            metadata
                                .insert("source_context".into(), serde_json::to_string(&context)?);
                        }
                    }
                    Some(metadata)
                },
                space_id: space_id.clone(),
            };
            let outcome = self.index_file_use_case.prepare_for_indexing(request).await;
            if self.batch_repo.get_batch_job(job_id).await?.status == "cancelled" {
                self.batch_repo
                    .update_item_status(&item.id, "skipped", None, Some("Import cancelled"))
                    .await?;
                return Ok(());
            }
            let result = match outcome {
                Ok(PrepareForIndexingOutcome::Duplicate { document_id }) => Ok(document_id),
                Ok(PrepareForIndexingOutcome::Prepared(prepared)) => {
                    self.index_file_use_case
                        .commit_prepared(*prepared, self.uow_factory.as_ref())
                        .await
                }
                Err(error) => Err(error),
            };
            let result = match (result, space_id.as_deref()) {
                (Ok(document_id), Some(space)) => match self.document_scope.as_ref() {
                    Some(scope) => scope
                        .assign_documents(std::slice::from_ref(&document_id), space)
                        .await
                        .map(|()| document_id),
                    None => Err(AppError::ServiceNotAvailable(
                        "Space assignment is unavailable".into(),
                    )),
                },
                (result, _) => result,
            };
            match result {
                Ok(document_id) => {
                    self.batch_repo
                        .update_item_status(&item.id, "completed", Some(&document_id), None)
                        .await?;
                    completed += 1;
                }
                Err(error) => {
                    tracing::warn!(%job_id, file = %item.url, %error, "File import failed");
                    self.batch_repo
                        .update_item_status(&item.id, "failed", None, Some(&error.to_string()))
                        .await?;
                    failed += 1;
                }
            }
            // The document and its embeddings are already committed. No batch
            // repository writes occur while the document transaction is open.
            self.batch_repo
                .update_progress(
                    job_id,
                    completed,
                    failed,
                    (completed + failed) as f64 / total as f64,
                )
                .await?;
        }
        if self.batch_repo.get_batch_job(job_id).await?.status != "cancelled" {
            self.batch_repo
                .update_job_status(
                    job_id,
                    if failed > 0 {
                        "failed"
                    } else if completed < total {
                        "cancelled"
                    } else {
                        "completed"
                    },
                    None,
                    Some(chrono::Utc::now().to_rfc3339()),
                )
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use crate::application::ports::batch_job_repository_port::{
        BatchJobItem, BatchJobStatus as PortBatchJobStatus, BatchJobSummary,
    };

    struct MockUoWFactory;

    #[async_trait]
    impl crate::application::ports::UnitOfWorkFactory for MockUoWFactory {
        async fn create(&self) -> Result<Box<dyn crate::application::ports::UnitOfWork + Send>> {
            use crate::domain::repositories::mocks::MockUnitOfWork;
            let mut mock = MockUnitOfWork::new();

            mock.expect_commit().returning(|| Ok(()));
            mock.expect_rollback().returning(|| Ok(()));

            Ok(Box::new(mock))
        }
    }

    struct MockBatchRepo {
        jobs: Arc<Mutex<HashMap<String, String>>>, // job_id -> status
        items: Arc<Mutex<HashMap<String, Vec<String>>>>, // job_id -> file_paths
    }

    fn temp_path(name: &str) -> String {
        std::env::temp_dir()
            .join(name)
            .to_string_lossy()
            .to_string()
    }

    impl MockBatchRepo {
        fn new() -> Self {
            Self {
                jobs: Arc::new(Mutex::new(HashMap::new())),
                items: Arc::new(Mutex::new(HashMap::new())),
            }
        }
    }

    #[async_trait]
    impl BatchJobRepositoryPort for MockBatchRepo {
        async fn create_batch_job(
            &self,
            job_id: &str,
            _job_type: &str,
            _total_items: i64,
            _options: Option<&str>,
        ) -> Result<()> {
            self.jobs
                .lock()
                .unwrap()
                .insert(job_id.to_string(), "pending".to_string());
            Ok(())
        }

        async fn create_batch_items(&self, job_id: &str, urls: Vec<String>) -> Result<()> {
            self.items.lock().unwrap().insert(job_id.to_string(), urls);
            Ok(())
        }

        async fn update_job_status(
            &self,
            job_id: &str,
            status: &str,
            _started_at: Option<String>,
            _completed_at: Option<String>,
        ) -> Result<()> {
            if let Some(job_status) = self.jobs.lock().unwrap().get_mut(job_id) {
                *job_status = status.to_string();
            }
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

        async fn get_batch_job(&self, _job_id: &str) -> Result<PortBatchJobStatus> {
            Ok(PortBatchJobStatus {
                id: "test".to_string(),
                job_type: "file_import".to_string(),
                status: "pending".to_string(),
                total_items: 0,
                completed_items: 0,
                failed_items: 0,
                progress: 0.0,
                created_at: "".to_string(),
                started_at: None,
                completed_at: None,
                error_message: None,
                items: vec![],
            })
        }

        async fn get_pending_items(&self, job_id: &str) -> Result<Vec<BatchJobItem>> {
            let items = self.items.lock().unwrap();
            let urls = items.get(job_id).cloned().unwrap_or_default();

            Ok(urls
                .into_iter()
                .enumerate()
                .map(|(i, url)| BatchJobItem {
                    id: format!("{}-{}", job_id, i),
                    url,
                })
                .collect())
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

    fn create_mock_index_file_use_case() -> IndexFileUseCase {
        use crate::application::ports::UnitOfWorkFactory;
        use crate::application::ports::{
            EmbeddingPort, EmbeddingRepositoryPort, FileStoragePort, RepositoryPort,
        };
        use crate::domain::entities::document::Document;
        use crate::features::embedding::entity::Embedding;
        use std::path::Path;

        struct MockFileStorage;
        #[async_trait]
        impl FileStoragePort for MockFileStorage {
            async fn read_file(&self, _path: &Path) -> Result<String> {
                Ok("test content".to_string())
            }
            async fn write_file(&self, _path: &Path, _content: &str) -> Result<()> {
                Ok(())
            }
            async fn delete_file(&self, _path: &Path) -> Result<()> {
                Ok(())
            }
            async fn read_file_bytes(&self, _path: &Path) -> Result<Vec<u8>> {
                Ok(vec![])
            }
            async fn write_file_bytes(&self, _path: &Path, _content: &[u8]) -> Result<()> {
                Ok(())
            }
            async fn compute_hash(&self, _path: &Path) -> Result<String> {
                Ok("hash".to_string())
            }
            async fn exists(&self, _path: &Path) -> bool {
                true
            }
            async fn metadata(
                &self,
                _path: &Path,
            ) -> Result<crate::application::ports::file_storage_port::FileMetadata> {
                use crate::application::ports::file_storage_port::FileMetadata;
                use chrono::Utc;
                Ok(FileMetadata {
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
            ) -> Result<crate::application::ports::ImportedBlob> {
                let hash = "a".repeat(64);
                Ok(crate::application::ports::ImportedBlob {
                    path: std::path::PathBuf::from("/mock/path"),
                    lease: crate::application::ports::BlobLease::detached(&hash),
                    hash,
                })
            }
            async fn exists_by_hash(&self, _hash: &str) -> Result<bool> {
                Ok(false)
            }
            async fn get_path_by_hash(&self, _hash: &str) -> Result<Option<std::path::PathBuf>> {
                Ok(None)
            }

            fn owns(&self, _path: &Path) -> bool {
                false
            }
            async fn list_hashes(&self) -> Result<Vec<String>> {
                Ok(Vec::new())
            }
            async fn remove_if_unreferenced(
                &self,
                _hash: &str,
                _refs: &dyn crate::application::ports::BlobReferenceCheck,
            ) -> Result<crate::application::ports::BlobRemoval> {
                Ok(crate::application::ports::BlobRemoval::Retained(
                    crate::application::ports::RetainReason::Missing,
                ))
            }
        }

        struct MockContentExtractor;
        #[async_trait]
        impl crate::application::ports::ContentExtractionPort for MockContentExtractor {
            async fn extract_content(
                &self,
                _path: &std::path::Path,
            ) -> Result<crate::application::ports::content_extraction_port::ExtractedContentData>
            {
                Ok(
                    crate::application::ports::content_extraction_port::ExtractedContentData {
                        text: "Mock content".to_string(),
                        mime_type: "text/plain".to_string(),
                        page_count: Some(1),
                        page_ranges: Vec::new(),
                        word_count: 2,
                        char_count: 12,
                    },
                )
            }
            fn is_supported(&self, _path: &std::path::Path) -> bool {
                true
            }
        }

        struct MockEmbedder;
        #[async_trait]
        impl EmbeddingPort for MockEmbedder {
            async fn embed_single(&self, _text: &str) -> Result<Vec<f32>> {
                Ok(vec![0.1, 0.2, 0.3])
            }
            async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
                Ok(texts.iter().map(|_| vec![0.1, 0.2, 0.3]).collect())
            }
            fn dimension(&self) -> usize {
                3
            }
            async fn is_ready(&self) -> Result<bool> {
                Ok(true)
            }
        }

        struct MockDocRepo;
        #[async_trait]
        impl RepositoryPort<Document> for MockDocRepo {
            async fn save(&self, _entity: &Document) -> Result<()> {
                Ok(())
            }
            async fn find_by_id(&self, _id: &str) -> Result<Option<Document>> {
                Ok(None)
            }
            async fn find_all(&self) -> Result<Vec<Document>> {
                Ok(vec![])
            }
            async fn exists(&self, _id: &str) -> Result<bool> {
                Ok(false)
            }
            async fn find_by_filter(
                &self,
                _filter: &dyn crate::application::ports::repository_port::Filter,
            ) -> Result<Vec<Document>> {
                Ok(vec![])
            }
            async fn save_batch(&self, _entities: &[Document]) -> Result<()> {
                Ok(())
            }
            async fn delete(&self, _id: &str) -> Result<()> {
                Ok(())
            }
            async fn delete_batch(&self, _ids: &[&str]) -> Result<()> {
                Ok(())
            }
            async fn count(&self) -> Result<usize> {
                Ok(0)
            }
        }

        #[async_trait]
        impl crate::application::ports::DocumentRepositoryPort for MockDocRepo {
            async fn find_by_checksum(
                &self,
                _checksum: &crate::domain::value_objects::Checksum,
            ) -> Result<Option<Document>> {
                Ok(None)
            }
            async fn count_documents(&self) -> Result<i64> {
                Ok(0)
            }
            async fn count_chunks(&self) -> Result<i64> {
                Ok(0)
            }
            async fn find_file_path_by_id(&self, _document_id: &str) -> Result<String> {
                Ok(String::new())
            }
            async fn find_id_by_path(&self, _file_path: &str) -> Result<Option<String>> {
                Ok(None)
            }
            async fn document_exists(&self, _document_id: &str) -> Result<bool> {
                Ok(false)
            }
            async fn delete(&self, _document_id: &str) -> Result<()> {
                Ok(())
            }
            async fn find_all_paginated(&self, _limit: usize) -> Result<Vec<Document>> {
                Ok(vec![])
            }
        }

        struct MockEmbeddingRepo;
        #[async_trait]
        impl EmbeddingRepositoryPort for MockEmbeddingRepo {
            async fn create(
                &self,
                _chunk_id: &str,
                _vector: &[f32],
                _model: &str,
            ) -> Result<String> {
                unimplemented!()
            }
            async fn find_by_chunk(&self, _chunk_id: &str) -> Result<Option<Embedding>> {
                unimplemented!()
            }
            async fn save(&self, _entity: &Embedding, _vector: Vec<f32>) -> Result<()> {
                Ok(())
            }
            async fn save_batch(&self, _entries: Vec<(Embedding, Vec<f32>)>) -> Result<()> {
                Ok(())
            }
            async fn find_by_chunk_id(
                &self,
                _chunk_id: &str,
            ) -> Result<Option<(Embedding, Vec<f32>)>> {
                Ok(None)
            }
            async fn find_by_document_id(
                &self,
                _document_id: &str,
            ) -> Result<Vec<(Embedding, Vec<f32>)>> {
                Ok(vec![])
            }
            async fn delete_by_chunk_id(&self, _chunk_id: &str) -> Result<()> {
                Ok(())
            }
            async fn delete_by_document_id(&self, _document_id: &str) -> Result<()> {
                Ok(())
            }
            async fn count(&self) -> Result<i64> {
                Ok(0)
            }

            async fn create_batch(
                &self,
                _entries: Vec<(Embedding, Vec<f32>)>,
            ) -> Result<Vec<String>> {
                Ok(vec![])
            }
        }

        struct MockUnitOfWork;

        #[async_trait]
        impl crate::application::ports::UnitOfWork for MockUnitOfWork {
            fn chunk_repository(
                &self,
            ) -> Result<Box<dyn crate::application::ports::ChunkRepositoryPort + Send + '_>>
            {
                Err(AppError::InvalidState(
                    "Chunk repository not used in test".to_string(),
                ))
            }

            fn document_repository(
                &self,
            ) -> Result<Box<dyn crate::application::ports::DocumentRepositoryPort + Send + '_>>
            {
                Ok(Box::new(MockDocRepo))
            }

            fn embedding_repository(&self) -> Result<Box<dyn EmbeddingRepositoryPort + Send + '_>> {
                Ok(Box::new(MockEmbeddingRepo))
            }

            fn search_repository(
                &self,
            ) -> Result<Box<dyn crate::domain::repositories::SearchRepository + Send + '_>>
            {
                Err(AppError::InvalidState(
                    "Search repository not used in test".to_string(),
                ))
            }

            fn batch_job_repository(
                &self,
            ) -> Result<Box<dyn crate::application::ports::BatchJobRepositoryPort + Send + '_>>
            {
                Err(AppError::InvalidState(
                    "Batch job repository not used in test".to_string(),
                ))
            }

            fn system_repository(
                &self,
            ) -> Result<Box<dyn crate::domain::repositories::SystemRepository + Send + '_>>
            {
                Err(AppError::InvalidState(
                    "System repository not used in test".to_string(),
                ))
            }

            fn model_repository(
                &self,
            ) -> Result<Box<dyn crate::application::ports::unit_of_work::ModelRepositoryPort + '_>>
            {
                Err(AppError::InvalidState(
                    "Model repository not used in test".to_string(),
                ))
            }

            fn model_file_repository(
                &self,
            ) -> Result<
                Box<dyn crate::application::ports::unit_of_work::ModelFileRepositoryPort + '_>,
            > {
                Err(AppError::InvalidState(
                    "Model file repository not used in test".to_string(),
                ))
            }

            async fn commit(&mut self) -> Result<()> {
                Ok(())
            }

            async fn rollback(&mut self) -> Result<()> {
                Ok(())
            }
        }

        struct MockUnitOfWorkFactory;

        #[async_trait]
        impl UnitOfWorkFactory for MockUnitOfWorkFactory {
            async fn create(
                &self,
            ) -> Result<Box<dyn crate::application::ports::UnitOfWork + Send>> {
                Ok(Box::new(MockUnitOfWork))
            }
        }

        IndexFileUseCase::new(
            Arc::new(MockFileStorage), // content_storage
            Arc::new(MockFileStorage), // file_storage
            Arc::new(MockContentExtractor),
            Arc::new(MockEmbedder),
            Arc::new(MockDocRepo),
            Arc::new(MockEmbeddingRepo),
            Arc::new(MockUnitOfWorkFactory),
        )
    }

    #[tokio::test]
    async fn test_valid_batch_succeeds() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let mut file_paths = Vec::new();

        for i in 0..10 {
            let file_path = temp_dir.path().join(format!("file{}.txt", i));
            fs::write(&file_path, "test content").unwrap();
            file_paths.push(file_path.to_str().unwrap().to_string());
        }

        let batch_repo = Arc::new(MockBatchRepo::new());
        let index_file = Arc::new(create_mock_index_file_use_case());
        let uow_factory = Arc::new(MockUoWFactory);
        let use_case =
            StartBatchFileImportUseCase::new(batch_repo.clone(), index_file, uow_factory);

        let request = StartBatchFileImportRequestDto {
            indexing: None,
            file_paths,
            space_id: None,
        };
        let response = use_case.execute(request).await.unwrap();

        assert!(!response.job_id.is_empty());

        let jobs = batch_repo.jobs.lock().unwrap();
        assert!(jobs.contains_key(&response.job_id));
    }

    #[tokio::test]
    async fn test_empty_batch_fails() {
        let batch_repo = Arc::new(MockBatchRepo::new());
        let index_file = Arc::new(create_mock_index_file_use_case());
        let uow_factory = Arc::new(MockUoWFactory);
        let use_case = StartBatchFileImportUseCase::new(batch_repo, index_file, uow_factory);

        let request = StartBatchFileImportRequestDto {
            indexing: None,
            file_paths: vec![],
            space_id: None,
        };

        let result = use_case.execute(request).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            AppError::InvalidInput(msg) => {
                assert!(msg.contains("at least 1"));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_too_large_batch_fails() {
        let batch_repo = Arc::new(MockBatchRepo::new());
        let index_file = Arc::new(create_mock_index_file_use_case());
        let uow_factory = Arc::new(MockUoWFactory);
        let use_case = StartBatchFileImportUseCase::new(batch_repo, index_file, uow_factory);

        let file_paths = (0..101)
            .map(|i| temp_path(&format!("file{}.txt", i)))
            .collect();
        let request = StartBatchFileImportRequestDto {
            indexing: None,
            file_paths,
            space_id: None,
        };

        let result = use_case.execute(request).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            AppError::InvalidInput(msg) => {
                assert!(msg.contains("exceeds maximum"));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_invalid_path_fails() {
        let batch_repo = Arc::new(MockBatchRepo::new());
        let index_file = Arc::new(create_mock_index_file_use_case());
        let uow_factory = Arc::new(MockUoWFactory);
        let use_case = StartBatchFileImportUseCase::new(batch_repo, index_file, uow_factory);

        let request = StartBatchFileImportRequestDto {
            indexing: None,
            file_paths: vec!["../../../etc/passwd".to_string()],

            space_id: None,
        };

        let result = use_case.execute(request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_background_processing_updates_status() {
        use std::fs;
        use tempfile::TempDir;
        use tokio::time::{sleep, Duration};

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("file.txt");
        fs::write(&file_path, "test content").unwrap();

        let batch_repo = Arc::new(MockBatchRepo::new());
        let index_file = Arc::new(create_mock_index_file_use_case());
        let uow_factory = Arc::new(MockUoWFactory);
        let use_case =
            StartBatchFileImportUseCase::new(batch_repo.clone(), index_file, uow_factory);

        let request = StartBatchFileImportRequestDto {
            indexing: None,
            file_paths: vec![file_path.to_str().unwrap().to_string()],

            space_id: None,
        };

        let response = use_case.execute(request).await.unwrap();

        // Wait for background processing
        sleep(Duration::from_millis(100)).await;

        let jobs = batch_repo.jobs.lock().unwrap();
        let status = jobs.get(&response.job_id).unwrap();

        // Status should have been updated from "pending" - could be "running", "completed", or "failed"
        // depending on whether the background task has started processing
        assert!(
            status != "pending",
            "Status should have been updated from pending, got: {}",
            status
        );
    }
}

#[cfg(test)]
mod progress_tests;
