//! # Reindex Document Use Case
//!
//! Reindexes an existing document with updated content or strategy.
//!
//! This use case:
//! 1. Retrieves existing document from repository
//! 2. Reads current file content
//! 3. Reindexes with new chunking strategy
//! 4. Generates new embeddings
//! 5. Updates repository
//!
//! ## Example
//!
//! ```rust,no_run
//! use vault_desktop::application::use_cases::indexing::reindex_document::ReindexDocumentUseCase;
//!
//! # async fn example(use_case: ReindexDocumentUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let response = use_case.execute("doc-123".to_string()).await?;
//! println!("Reindexed with {} chunks", response.chunks_created);
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use crate::features::indexing::dto::IndexFileResponseDto;
use crate::application::factories::FileMetadataFactory;
use crate::application::ports::{EmbeddingPort, FileStoragePort, RepositoryPort};
use crate::domain::embedding_constants::DEFAULT_EMBEDDING_MODEL_NAME;
use crate::features::embedding::entity::Embedding;
use crate::domain::entities::Document;
use crate::domain::repositories::UnitOfWorkFactory;
use crate::domain::value_objects::chunking_strategy::ChunkingStrategy;
use crate::infrastructure::services::metadata_extraction::MetadataExtractor;
use crate::shared::error::{AppError, Result};

/// Reindex document use case.
///
/// Updates an existing document's index with current content.
///
/// ## Use Cases
///
/// - File content has changed
/// - Switching to a different chunking strategy
/// - Fixing indexing errors
/// - Updating embeddings with new model
///
/// ## Dependencies
///
/// - `FileStoragePort`: Reads updated file content
/// - `EmbeddingPort`: Generates new embeddings
/// - `RepositoryPort`: Retrieves and updates document
/// - `EmbeddingRepositoryPort`: Persists chunk embeddings
pub struct ReindexDocumentUseCase {
    file_storage: Arc<dyn FileStoragePort>,
    embedding_service: Arc<dyn EmbeddingPort>,
    document_repo: Arc<dyn RepositoryPort<Document>>,
    uow_factory: Arc<dyn UnitOfWorkFactory>,
}

impl ReindexDocumentUseCase {
    /// Create a new reindex document use case.
    ///
    /// # Arguments
    ///
    /// * `file_storage` - Service for reading file content
    /// * `embedding_service` - Service for generating embeddings
    /// * `document_repo` - Repository for document persistence
    /// * `embedding_repo` - Repository for persisting embeddings
    pub fn new(
        file_storage: Arc<dyn FileStoragePort>,
        embedding_service: Arc<dyn EmbeddingPort>,
        document_repo: Arc<dyn RepositoryPort<Document>>,
        uow_factory: Arc<dyn UnitOfWorkFactory>,
    ) -> Self {
        Self {
            file_storage,
            embedding_service,
            document_repo,
            uow_factory,
        }
    }

    /// Execute document reindexing.
    ///
    /// # Arguments
    ///
    /// * `document_id` - ID of document to reindex
    ///
    /// # Returns
    ///
    /// Response with updated indexing statistics
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Document not found
    /// - File no longer exists
    /// - Reindexing fails
    /// - Embedding generation fails
    /// - Update fails
    pub async fn execute(&self, document_id: String) -> Result<IndexFileResponseDto> {
        // 1. Retrieve existing document
        let mut document = self
            .document_repo
            .find_by_id(&document_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Document not found: {}", document_id)))?;

        // 2. Read current file content (clone to avoid borrow conflict)
        let file_path = document.document().file_path().to_path_buf();
        let content = self.file_storage.read_file(&file_path).await?;

        // 3. Reindex with an inferred strategy based on existing chunk sizes,
        // then adapt for new content length.
        let strategy = ChunkingStrategy::infer_from_existing_chunks(document.chunks())
            .adapt_for_content(&content);
        document.reindex(content.clone(), strategy)?;

        // 3a. Extract and set metadata
        let metadata_extractor = MetadataExtractor::new();

        // Get file extension
        let file_ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

        // Get file metadata for quality score calculation
        let file_metadata = FileMetadataFactory::from_path(&file_path)?;

        // Detect language
        let language = metadata_extractor.detect_language(&content, file_ext);

        // Categorize
        let category = metadata_extractor.categorize(
            &content,
            file_metadata.file_name(),
            file_metadata.mime_type(),
        );

        // Calculate quality score
        let quality_score = metadata_extractor.calculate_quality_score(
            &content,
            file_metadata.size_bytes(),
            file_metadata.modified_at(),
        );

        // Count words
        let word_count = metadata_extractor.count_words(&content);

        // Set document metadata
        document.set_language(language.clone());
        document.set_category(category);
        document.set_quality_score(quality_score);
        document.set_word_count(word_count);

        // Set chunk token counts
        let chunk_count = document.chunks().len();
        for index in 0..chunk_count {
            let chunk = document.chunks().get(index).ok_or_else(|| {
                AppError::InvalidInput(format!("Chunk index {} out of bounds", index))
            })?;
            let token_count = metadata_extractor.count_tokens(chunk.content());
            document.set_chunk_token_count(index, token_count);
        }

        // 4. Generate new embeddings
        let chunk_texts: Vec<String> = document
            .chunks()
            .iter()
            .map(|c| c.content().to_string())
            .collect();

        let embeddings = self.embedding_service.embed_batch(&chunk_texts).await?;

        // 5. Create and persist new embeddings
        let embedding_entries: Vec<(Embedding, Vec<f32>)> = document
            .chunks()
            .iter()
            .zip(embeddings.iter())
            .map(|(chunk, embedding_vec)| {
                let embedding_metadata = Embedding::new(
                    chunk.id().clone(),
                    DEFAULT_EMBEDDING_MODEL_NAME.to_string(),
                    embedding_vec.len(),
                );
                (embedding_metadata, embedding_vec.clone())
            })
            .collect();

        let mut uow = self.uow_factory.create().await?;
        let db_result = {
            let document_repo = uow.document_repository()?;
            let embedding_repo = uow.embedding_repository()?;

            embedding_repo
                .delete_by_document_id(document.document().id().as_str())
                .await?;
            document_repo.save(&document).await?;
            embedding_repo.save_batch(embedding_entries).await?;
            Ok(())
        };

        match db_result {
            Ok(()) => {
                uow.commit().await?;
            }
            Err(err) => {
                if let Err(rollback_err) = uow.rollback().await {
                    return Err(AppError::Database(format!(
                        "Reindex failed: {}; rollback failed: {}",
                        err, rollback_err
                    )));
                }
                return Err(err);
            }
        }

        // 8. Build response
        let file_path_str = document
            .document()
            .file_path()
            .to_str()
            .ok_or_else(|| AppError::InvalidInput("Invalid file path encoding".to_string()))?;

        Ok(IndexFileResponseDto {
            document_id: document.document().id().to_string(),
            chunks_created: document.chunks().len(),
            status: "reindexed".to_string(),
            error: None,
            file_path: file_path_str.to_string(),
        })
    }

    /// Execute reindexing with a specific chunking strategy.
    ///
    /// # Arguments
    ///
    /// * `document_id` - ID of document to reindex
    /// * `strategy` - New chunking strategy to use
    ///
    /// # Returns
    ///
    /// Response with updated indexing statistics
    pub async fn execute_with_strategy(
        &self,
        document_id: String,
        strategy: ChunkingStrategy,
    ) -> Result<IndexFileResponseDto> {
        let mut document = self
            .document_repo
            .find_by_id(&document_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Document not found: {}", document_id)))?;

        let file_path = document.document().file_path().to_path_buf();
        let content = self.file_storage.read_file(&file_path).await?;

        let effective_strategy = strategy.adapt_for_content(&content);
        document.reindex(content.clone(), effective_strategy)?;

        // Extract and set metadata
        let metadata_extractor = MetadataExtractor::new();

        // Get file extension
        let file_ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

        // Get file metadata for quality score calculation
        let file_metadata = FileMetadataFactory::from_path(&file_path)?;

        // Detect language
        let language = metadata_extractor.detect_language(&content, file_ext);

        // Categorize
        let category = metadata_extractor.categorize(
            &content,
            file_metadata.file_name(),
            file_metadata.mime_type(),
        );

        // Calculate quality score
        let quality_score = metadata_extractor.calculate_quality_score(
            &content,
            file_metadata.size_bytes(),
            file_metadata.modified_at(),
        );

        // Count words
        let word_count = metadata_extractor.count_words(&content);

        // Set document metadata
        document.set_language(language.clone());
        document.set_category(category);
        document.set_quality_score(quality_score);
        document.set_word_count(word_count);

        // Set chunk token counts
        let chunk_count = document.chunks().len();
        for index in 0..chunk_count {
            let chunk = document.chunks().get(index).ok_or_else(|| {
                AppError::InvalidInput(format!(
                    "Chunk index {} out of bounds in reindex no-persist",
                    index
                ))
            })?;
            let token_count = metadata_extractor.count_tokens(chunk.content());
            document.set_chunk_token_count(index, token_count);
        }

        let chunk_texts: Vec<String> = document
            .chunks()
            .iter()
            .map(|c| c.content().to_string())
            .collect();

        let embeddings = self.embedding_service.embed_batch(&chunk_texts).await?;

        let embedding_entries: Vec<(Embedding, Vec<f32>)> = document
            .chunks()
            .iter()
            .zip(embeddings.iter())
            .map(|(chunk, embedding_vec)| {
                let embedding_metadata = Embedding::new(
                    chunk.id().clone(),
                    DEFAULT_EMBEDDING_MODEL_NAME.to_string(),
                    embedding_vec.len(),
                );
                (embedding_metadata, embedding_vec.clone())
            })
            .collect();

        let mut uow = self.uow_factory.create().await?;
        let db_result = {
            let document_repo = uow.document_repository()?;
            let embedding_repo = uow.embedding_repository()?;

            embedding_repo
                .delete_by_document_id(document.document().id().as_str())
                .await?;
            document_repo.save(&document).await?;
            embedding_repo.save_batch(embedding_entries).await?;
            Ok(())
        };

        match db_result {
            Ok(()) => {
                uow.commit().await?;
            }
            Err(err) => {
                if let Err(rollback_err) = uow.rollback().await {
                    return Err(AppError::Database(format!(
                        "Reindex failed: {}; rollback failed: {}",
                        err, rollback_err
                    )));
                }
                return Err(err);
            }
        }

        let file_path_str = document
            .document()
            .file_path()
            .to_str()
            .ok_or_else(|| AppError::InvalidInput("Invalid file path encoding".to_string()))?;

        Ok(IndexFileResponseDto {
            document_id: document.document().id().to_string(),
            chunks_created: document.chunks().len(),
            status: "reindexed".to_string(),
            error: None,
            file_path: file_path_str.to_string(),
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::{EmbeddingRepositoryPort, FileMetadata, Filter};
    use crate::features::embedding::entity::Embedding;
    use crate::domain::entities::Document;
    use crate::domain::repositories::UnitOfWorkFactory;
    use crate::shared::domain_types::ValidatedFilePath;
    use async_trait::async_trait;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use tempfile::TempDir;

    // Mock file storage
    struct MockFileStorage;

    #[async_trait]
    impl FileStoragePort for MockFileStorage {
        async fn read_file(&self, _path: &Path) -> Result<String> {
            // Return content long enough to create chunks with default strategy (size=512)
            Ok("Updated content for reindexing. This is a longer text that will be chunked properly with the default chunking strategy. \
            We need to ensure this content is long enough to create at least one chunk when using the default FixedSize strategy with size 512. \
            This text is being extended to reach that minimum length requirement so that the reindexing tests can verify that chunks are created successfully. \
            Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. \
            Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.".to_string())
        }

        async fn read_file_bytes(&self, _path: &Path) -> Result<Vec<u8>> {
            Ok(b"Updated content for reindexing. This is a longer text that will be chunked properly with the default chunking strategy.".to_vec())
        }

        async fn write_file(&self, _path: &Path, _content: &str) -> Result<()> {
            Ok(())
        }

        async fn write_file_bytes(&self, _path: &Path, _content: &[u8]) -> Result<()> {
            Ok(())
        }

        async fn delete_file(&self, _path: &Path) -> Result<()> {
            Ok(())
        }

        async fn compute_hash(&self, _path: &Path) -> Result<String> {
            Ok("mock_hash".to_string())
        }

        async fn exists(&self, _path: &Path) -> bool {
            true
        }

        async fn metadata(&self, _path: &Path) -> Result<FileMetadata> {
            Ok(FileMetadata {
                size: 1000,
                modified_at: 1640000000,
                is_file: true,
                is_directory: false,
            })
        }
    }

    // Mock embedder
    struct MockEmbedder;

    #[async_trait]
    impl EmbeddingPort for MockEmbedder {
        async fn embed_single(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![0.1, 0.2])
        }

        async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![0.1, 0.2]).collect())
        }

        fn dimension(&self) -> usize {
            2
        }

        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }

    struct MockUowDocumentRepo;

    #[async_trait]
    impl RepositoryPort<Document> for MockUowDocumentRepo {
        async fn save(&self, _entity: &Document) -> Result<()> {
            Ok(())
        }

        async fn find_by_id(&self, _id: &str) -> Result<Option<Document>> {
            Ok(None)
        }

        async fn find_all(&self) -> Result<Vec<Document>> {
            Ok(vec![])
        }

        async fn find_by_filter(&self, _filter: &dyn Filter) -> Result<Vec<Document>> {
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

        async fn exists(&self, _id: &str) -> Result<bool> {
            Ok(false)
        }
    }

    #[async_trait]
    impl crate::application::ports::DocumentRepositoryPort for MockUowDocumentRepo {
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
    }

    struct MockUowEmbeddingRepo;

    #[async_trait]
    impl EmbeddingRepositoryPort for MockUowEmbeddingRepo {
        async fn create(&self, _chunk_id: &str, _vector: &[f32], _model: &str) -> Result<String> {
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

        async fn find_by_chunk_id(&self, _chunk_id: &str) -> Result<Option<(Embedding, Vec<f32>)>> {
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

        async fn create_batch(&self, _entries: Vec<(Embedding, Vec<f32>)>) -> Result<Vec<String>> {
            Ok(vec![])
        }
    }

    struct MockUnitOfWork;

    #[async_trait]
    impl crate::domain::repositories::UnitOfWork for MockUnitOfWork {
        fn chunk_repository(
            &self,
        ) -> Result<Box<dyn crate::application::ports::ChunkRepositoryPort + Send + '_>> {
            Err(AppError::InvalidState(
                "Chunk repository not used in test".to_string(),
            ))
        }

        fn document_repository(
            &self,
        ) -> Result<Box<dyn crate::application::ports::DocumentRepositoryPort + Send + '_>>
        {
            Ok(Box::new(MockUowDocumentRepo))
        }

        fn embedding_repository(&self) -> Result<Box<dyn EmbeddingRepositoryPort + Send + '_>> {
            Ok(Box::new(MockUowEmbeddingRepo))
        }

        fn search_repository(
            &self,
        ) -> Result<Box<dyn crate::domain::repositories::SearchRepository + Send + '_>> {
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
        ) -> Result<Box<dyn crate::domain::repositories::SystemRepository + Send + '_>> {
            Err(AppError::InvalidState(
                "System repository not used in test".to_string(),
            ))
        }

        fn model_repository(
            &self,
        ) -> Result<Box<dyn crate::domain::repositories::unit_of_work::ModelRepositoryPort + '_>>
        {
            Err(AppError::InvalidState(
                "Model repository not used in test".to_string(),
            ))
        }

        fn model_file_repository(
            &self,
        ) -> Result<Box<dyn crate::domain::repositories::unit_of_work::ModelFileRepositoryPort + '_>>
        {
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
        async fn create(&self) -> Result<Box<dyn crate::domain::repositories::UnitOfWork + Send>> {
            Ok(Box::new(MockUnitOfWork))
        }
    }

    // Mock repository that stores a single document
    struct MockDocumentRepo {
        document: Mutex<Option<Document>>,
    }

    impl MockDocumentRepo {
        fn new() -> Self {
            Self {
                document: Mutex::new(None),
            }
        }

        fn set_document(&self, doc: Document) {
            *self.document.lock().unwrap() = Some(doc);
        }
    }

    #[async_trait]
    impl RepositoryPort<Document> for MockDocumentRepo {
        async fn save(&self, entity: &Document) -> Result<()> {
            *self.document.lock().unwrap() = Some(entity.clone());
            Ok(())
        }

        async fn find_by_id(&self, _id: &str) -> Result<Option<Document>> {
            Ok(self.document.lock().unwrap().clone())
        }

        async fn find_all(&self) -> Result<Vec<Document>> {
            Ok(self.document.lock().unwrap().iter().cloned().collect())
        }

        async fn find_by_filter(&self, _filter: &dyn Filter) -> Result<Vec<Document>> {
            Ok(vec![])
        }

        async fn save_batch(&self, entities: &[Document]) -> Result<()> {
            if let Some(entity) = entities.first() {
                *self.document.lock().unwrap() = Some(entity.clone());
            }
            Ok(())
        }

        async fn delete(&self, _id: &str) -> Result<()> {
            *self.document.lock().unwrap() = None;
            Ok(())
        }

        async fn delete_batch(&self, _ids: &[&str]) -> Result<()> {
            *self.document.lock().unwrap() = None;
            Ok(())
        }

        async fn count(&self) -> Result<usize> {
            Ok(if self.document.lock().unwrap().is_some() {
                1
            } else {
                0
            })
        }

        async fn exists(&self, _id: &str) -> Result<bool> {
            Ok(self.document.lock().unwrap().is_some())
        }
    }

    fn create_test_aggregate() -> (Document, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Original content").unwrap();

        let validated_path = ValidatedFilePath::new(file_path).unwrap();
        let content = "Original content".to_string();
        let strategy = ChunkingStrategy::FixedSize { size: 10 };
        let metadata = crate::domain::value_objects::FileMetadata::new(
            "test.txt".to_string(),
            "text/plain".to_string(),
            content.len() as i64,
            chrono::Utc::now(),
        )
        .unwrap();
        let checksum = crate::domain::value_objects::Checksum::new("a".repeat(64)).unwrap();

        let document =
            Document::from_file(validated_path, metadata, checksum, content, strategy).unwrap();
        (document, temp_dir)
    }

    #[tokio::test]

    async fn test_reindex_document() {
        let repo = Arc::new(MockDocumentRepo::new());
        let (aggregate, _temp_dir) = create_test_aggregate();
        let doc_id = aggregate.id().to_string();
        let original_chunks = aggregate.chunks().len();

        repo.set_document(aggregate);

        let use_case = ReindexDocumentUseCase::new(
            Arc::new(MockFileStorage),
            Arc::new(MockEmbedder),
            repo.clone(),
            Arc::new(MockUnitOfWorkFactory),
        );

        let response = use_case.execute(doc_id.clone()).await.unwrap();

        assert_eq!(response.document_id, doc_id);
        assert_eq!(response.status, "reindexed");
        assert!(response.error.is_none());
        // Chunks may differ due to different content
        assert!(response.chunks_created > 0);
    }

    #[tokio::test]
    async fn test_reindex_document_not_found() {
        let repo = Arc::new(MockDocumentRepo::new());
        // Don't set any document

        let use_case = ReindexDocumentUseCase::new(
            Arc::new(MockFileStorage),
            Arc::new(MockEmbedder),
            repo,
            Arc::new(MockUnitOfWorkFactory),
        );

        let result = use_case.execute("nonexistent-id".to_string()).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
    }

    #[tokio::test]

    async fn test_reindex_with_different_strategy() {
        let repo = Arc::new(MockDocumentRepo::new());
        let (aggregate, _temp_dir) = create_test_aggregate();
        let doc_id = aggregate.id().to_string();

        repo.set_document(aggregate);

        let use_case = ReindexDocumentUseCase::new(
            Arc::new(MockFileStorage),
            Arc::new(MockEmbedder),
            repo,
            Arc::new(MockUnitOfWorkFactory),
        );

        // Reindex with different chunk size
        let new_strategy = ChunkingStrategy::FixedSize { size: 5 };
        let response = use_case
            .execute_with_strategy(doc_id, new_strategy)
            .await
            .unwrap();

        // Should have more chunks with smaller size
        assert!(response.chunks_created > 0);
        assert_eq!(response.status, "reindexed");
    }
}
