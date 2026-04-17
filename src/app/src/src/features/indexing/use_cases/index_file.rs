//! # Index File Use Case
//!
//! Indexes a single file into the document store.
//!
//! This use case orchestrates:
//! 1. File path validation
//! 2. File content reading
//! 3. Document aggregate creation with chunking
//! 4. Embedding generation for chunks
//! 5. Persistence to repository
//!
//! ## Example
//!
//! ```rust,no_run
//! use vault_desktop::application::use_cases::indexing::index_file::IndexFileUseCase;
//! use vault_desktop::application::dtos::indexing_dto::{IndexFileRequestDto, ChunkingStrategyDto};
//!
//! # async fn example(use_case: IndexFileUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let request = IndexFileRequestDto {
//!     path: "/path/to/document.txt".to_string(),
//!     chunking_strategy: ChunkingStrategyDto::FixedSize { size: 512 },
//! };
//!
//! let response = use_case.execute(request).await?;
//! println!("Indexed document {} with {} chunks",
//!     response.document_id,
//!     response.chunks_created
//! );
//! # Ok(())
//! # }
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crate::features::indexing::dto::{IndexFileRequestDto, IndexFileResponseDto};
use crate::application::factories::{ChecksumFactory, FileMetadataFactory};
use crate::features::indexing::mapper::IndexingMapper;
use crate::application::ports::{
    ContentAddressedStoragePort, ContentExtractionPort, DocumentRepositoryPort, EmbeddingPort,
    EmbeddingRepositoryPort, FileStoragePort, VectorSearchPort,
};
use crate::domain::embedding_constants::DEFAULT_EMBEDDING_MODEL_NAME;
use crate::domain::entities::Document;
use crate::features::embedding::entity::Embedding;
use crate::domain::repositories::UnitOfWorkFactory;
use crate::infrastructure::services::metadata_extraction::MetadataExtractor;
use crate::shared::domain_types::ValidatedFilePath;
use crate::shared::error::{AppError, Result};
use tracing::{info, instrument};

// Type aliases for complex return types
/// Type alias for embedding with its vector representation.
pub type EmbeddingWithVector = (Embedding, Vec<f32>);

/// Type alias for prepared document ready for indexing.
/// (document, embeddings, library_path, imported_new)
pub type PreparedDocument = (Document, Vec<EmbeddingWithVector>, String, bool);

pub enum PrepareForIndexingOutcome {
    Prepared(PreparedDocument),
    Duplicate { document_id: String },
}

/// Index file use case.
///
/// Coordinates indexing of a single file, including:
/// - File validation and content extraction
/// - Document chunking
/// - Embedding generation
/// - Persistence
///
/// ## Dependencies
///
/// - `ContentAddressedStoragePort`: Imports files to library storage
/// - `FileStoragePort`: Reads file content
/// - `EmbeddingPort`: Generates embeddings for chunks
/// - `RepositoryPort`: Persists document aggregate
/// - `EmbeddingRepositoryPort`: Persists chunk embeddings
pub struct IndexFileUseCase {
    content_storage: Arc<dyn ContentAddressedStoragePort>,
    file_storage: Arc<dyn FileStoragePort>,
    content_extractor: Arc<dyn ContentExtractionPort>,
    embedding_service: Arc<dyn EmbeddingPort>,
    document_repo: Arc<dyn DocumentRepositoryPort>,
    embedding_repo: Arc<dyn EmbeddingRepositoryPort>,
    uow_factory: Arc<dyn UnitOfWorkFactory>,
    /// Shared in-memory vector index for runtime updates.
    /// When new embeddings are persisted, they are also added here
    /// so that newly indexed documents are immediately searchable.
    vector_search: Option<Arc<dyn VectorSearchPort>>,
}

impl IndexFileUseCase {
    /// Create a new index file use case.
    ///
    /// # Arguments
    ///
    /// * `content_storage` - Service for importing files to library
    /// * `file_storage` - Service for reading file content
    /// * `content_extractor` - Service for extracting content from files
    /// * `embedding_service` - Service for generating embeddings
    /// * `document_repo` - Repository for persisting documents
    /// * `embedding_repo` - Repository for persisting embeddings
    pub fn new(
        content_storage: Arc<dyn ContentAddressedStoragePort>,
        file_storage: Arc<dyn FileStoragePort>,
        content_extractor: Arc<dyn ContentExtractionPort>,
        embedding_service: Arc<dyn EmbeddingPort>,
        document_repo: Arc<dyn DocumentRepositoryPort>,
        embedding_repo: Arc<dyn EmbeddingRepositoryPort>,
        uow_factory: Arc<dyn UnitOfWorkFactory>,
    ) -> Self {
        Self {
            content_storage,
            file_storage,
            content_extractor,
            embedding_service,
            document_repo,
            embedding_repo,
            uow_factory,
            vector_search: None,
        }
    }

    /// Set the shared in-memory vector index for runtime updates.
    ///
    /// When set, newly indexed embeddings are also added to this index
    /// so they become immediately searchable without requiring an app restart.
    pub fn with_vector_search(mut self, vs: Arc<dyn VectorSearchPort>) -> Self {
        self.vector_search = Some(vs);
        self
    }

    /// Execute file indexing.
    ///
    /// # Arguments
    ///
    /// * `request` - Index request containing file path and chunking strategy
    ///
    /// # Returns
    ///
    /// Response with document ID and indexing statistics
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - File path is invalid or inaccessible
    /// - File content cannot be read
    /// - Chunking fails
    /// - Embedding generation fails
    /// - Persistence fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use vault_desktop::application::use_cases::indexing::index_file::IndexFileUseCase;
    /// # use vault_desktop::application::dtos::indexing_dto::{IndexFileRequestDto, ChunkingStrategyDto};
    /// # async fn example(use_case: IndexFileUseCase) -> Result<(), Box<dyn std::error::Error>> {
    /// let request = IndexFileRequestDto {
    ///     path: "/docs/paper.pdf".to_string(),
    ///     chunking_strategy: ChunkingStrategyDto::Semantic { max_tokens: 512 },
    /// };
    ///
    /// match use_case.execute(request).await {
    ///     Ok(response) => println!("Success: {} chunks", response.chunks_created),
    ///     Err(e) => eprintln!("Failed: {}", e),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self, request), fields(file_path = %request.path))]
    pub async fn execute(&self, request: IndexFileRequestDto) -> Result<IndexFileResponseDto> {
        let mut uow = self.uow_factory.create().await?;
        let result = self.execute_with_uow(&mut uow, request).await;
        match result {
            Ok(response) => {
                uow.commit().await?;
                Ok(response)
            }
            Err(err) => {
                if let Err(rollback_err) = uow.rollback().await {
                    return Err(AppError::Database(format!(
                        "Indexing failed: {}; rollback failed: {}",
                        err, rollback_err
                    )));
                }
                Err(err)
            }
        }
    }

    /// Execute file indexing with Unit of Work (single atomic transaction).
    ///
    /// This variant wraps both document and embedding saves in a single
    /// transaction for atomicity. The caller must commit the transaction.
    ///
    /// # Purpose
    ///
    /// Provides atomic file indexing where both document metadata and embeddings
    /// are committed together. If either operation fails, both are rolled back.
    ///
    /// # Arguments
    ///
    /// * `uow` - Mutable reference to Unit of Work (manages transaction)
    /// * `request` - Index request with file path and chunking strategy
    ///
    /// # Returns
    ///
    /// Response with document ID and statistics
    ///
    /// # Errors
    ///
    /// Same as `execute()` plus transaction errors
    ///
    /// # Transaction Semantics
    ///
    /// - Does NOT commit the transaction (caller's responsibility)
    /// - Allows batch operations to share one transaction across multiple files
    /// - If this method returns Ok, changes are staged but not committed
    /// - If this method returns Err, changes should be rolled back by caller
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use recall_desktop::application::use_cases::indexing::index_file::IndexFileUseCase;
    /// # use recall_desktop::domain::repositories::UnitOfWorkFactory;
    /// # async fn example(use_case: IndexFileUseCase, factory: impl UnitOfWorkFactory) {
    /// let mut uow = factory.create().await.unwrap();
    /// let response = use_case.execute_with_uow(&mut uow, request).await.unwrap();
    /// uow.commit().await.unwrap(); // Commit after success
    /// # }
    /// ```
    /// Prepare file for indexing (extraction + embeddings) WITHOUT opening transaction.
    /// Returns prepared aggregate and embeddings ready to be saved.
    ///
    /// This is Phase 1 of the "Prepare-Then-Commit" pattern for batch operations.
    /// Call this to prepare files, then save all at once in a single fast transaction.
    #[instrument(skip(self, request), fields(file_path = %request.path))]
    pub async fn prepare_for_indexing(
        &self,
        request: IndexFileRequestDto,
    ) -> Result<PrepareForIndexingOutcome> {
        let source_path = ValidatedFilePath::new(PathBuf::from(&request.path))?;
        self.validate_supported_file(source_path.as_path())?;
        let content_hash = self
            .file_storage
            .compute_hash(source_path.as_path())
            .await?;
        let content_checksum = crate::domain::value_objects::Checksum::new(content_hash.clone())?;

        // Check for duplicates using pool-based repo (fast query, no transaction)
        if let Some(existing) = self
            .document_repo
            .find_by_checksum(&content_checksum)
            .await?
        {
            return Ok(PrepareForIndexingOutcome::Duplicate {
                document_id: existing.id().to_string(),
            });
        }

        let already_in_library = self.content_storage.exists_by_hash(&content_hash).await?;
        let (library_path, _) = self
            .content_storage
            .import_file(source_path.as_path())
            .await?;
        let imported_new = !already_in_library;

        // Heavy work (extraction + embeddings) - NO TRANSACTION
        let metadata = FileMetadataFactory::from_path(library_path.as_path())?;
        let checksum = ChecksumFactory::from_path(library_path.as_path())?;
        let extracted = self
            .content_extractor
            .extract_content(library_path.as_path())
            .await?;
        let content = extracted.text;
        self.validate_text_content(source_path.as_path(), &content)?;
        self.validate_text_content(source_path.as_path(), &content)?;

        let chunking_strategy =
            IndexingMapper::chunking_strategy_to_domain(request.chunking_strategy)
                .adapt_for_content(&content);
        let library_path_validated = ValidatedFilePath::new(library_path.clone())?;
        let mut document = Document::from_file(
            library_path_validated,
            metadata.clone(),
            checksum,
            content.clone(),
            chunking_strategy,
        )?;

        // Set metadata
        let metadata_extractor = MetadataExtractor::new();
        let file_ext = document
            .file_path()
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        document.set_language(metadata_extractor.detect_language(&content, file_ext));
        document.set_category(metadata_extractor.categorize(
            &content,
            metadata.file_name(),
            metadata.mime_type(),
        ));
        document.set_quality_score(metadata_extractor.calculate_quality_score(
            &content,
            metadata.size_bytes(),
            metadata.modified_at(),
        ));
        document.set_word_count(metadata_extractor.count_words(&content));

        for index in 0..document.chunks().len() {
            let chunk = document.chunks().get(index).ok_or_else(|| {
                AppError::InvalidInput(format!(
                    "Chunk index {} out of bounds in index_file_and_persist",
                    index
                ))
            })?;
            let chunk_content = chunk.content().to_string();
            document.set_chunk_token_count(index, metadata_extractor.count_tokens(chunk.content()));
            document.set_chunk_word_count(
                index,
                metadata_extractor.count_words(&chunk_content) as usize,
            );
            document.set_chunk_has_code(index, metadata_extractor.contains_code(&chunk_content));
            document.set_chunk_section(index, metadata_extractor.extract_section(&chunk_content));
        }

        // Generate embeddings
        let chunk_texts: Vec<String> = document
            .chunks()
            .iter()
            .map(|c| c.content().to_string())
            .collect();
        let embeddings = self.embedding_service.embed_batch(&chunk_texts).await?;

        // Prepare embedding entries
        let embedding_entries: Vec<(Embedding, Vec<f32>)> = document
            .chunks()
            .iter()
            .zip(embeddings.iter())
            .map(|(chunk, vec)| {
                let embedding = Embedding::new(
                    chunk.id().clone(),
                    DEFAULT_EMBEDDING_MODEL_NAME.to_string(),
                    vec.len(),
                );
                (embedding, vec.clone())
            })
            .collect();

        Ok(PrepareForIndexingOutcome::Prepared((
            document,
            embedding_entries,
            library_path.to_str().unwrap_or("").to_string(),
            imported_new,
        )))
    }

    pub async fn cleanup_library_file(&self, path: &str) -> Result<()> {
        if path.trim().is_empty() {
            return Ok(());
        }
        self.file_storage.delete_file(Path::new(path)).await
    }

    fn validate_supported_file(&self, path: &Path) -> Result<()> {
        if self.content_extractor.is_supported(path) {
            return Ok(());
        }

        let detected_type = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("unknown")
            .to_string();

        Err(AppError::UnsupportedFileType {
            path: path.display().to_string(),
            detected_type,
        })
    }

    fn validate_text_content(&self, path: &Path, content: &str) -> Result<()> {
        if !content.trim().is_empty() {
            return Ok(());
        }

        Err(AppError::InvalidInput(format!(
            "File contains no extractable text: {}",
            path.display()
        )))
    }

    #[instrument(skip(self, uow, request), fields(file_path = %request.path))]
    pub async fn execute_with_uow(
        &self,
        uow: &mut Box<dyn crate::domain::repositories::UnitOfWork + Send>,
        request: IndexFileRequestDto,
    ) -> Result<IndexFileResponseDto> {
        let total_start = Instant::now();

        // 1. Validate and import file (no transaction needed yet)
        let source_path = ValidatedFilePath::new(PathBuf::from(&request.path))?;
        self.validate_supported_file(source_path.as_path())?;
        let (library_path, content_hash) = self
            .content_storage
            .import_file(source_path.as_path())
            .await?;

        // 2. Get repositories from UoW (all share same transaction)
        let document_repo = uow.document_repository()?;
        let embedding_repo = uow.embedding_repository()?;

        // 3. Check for duplicates (uses UoW transaction)
        let content_checksum = crate::domain::value_objects::Checksum::new(content_hash.clone())?;
        if let Some(existing_doc) = document_repo.find_by_checksum(&content_checksum).await? {
            return Ok(IndexFileResponseDto {
                document_id: existing_doc.id().to_string(),
                chunks_created: existing_doc.chunks().len(),
                status: "already_indexed".to_string(),
                error: None,
                file_path: library_path.to_str().unwrap_or("").to_string(),
            });
        }

        // 4-10. File processing (same as execute())
        let metadata = FileMetadataFactory::from_path(library_path.as_path())?;
        let checksum = ChecksumFactory::from_path(library_path.as_path())?;

        let extraction_start = Instant::now();
        let extracted = self
            .content_extractor
            .extract_content(library_path.as_path())
            .await?;
        let content = extracted.text;
        let extraction_duration = extraction_start.elapsed();

        let chunking_strategy =
            IndexingMapper::chunking_strategy_to_domain(request.chunking_strategy)
                .adapt_for_content(&content);
        let library_path_validated = ValidatedFilePath::new(library_path.clone())?;
        let mut document = Document::from_file(
            library_path_validated,
            metadata.clone(),
            checksum,
            content.clone(),
            chunking_strategy,
        )?;

        // 11. Set document and chunk metadata
        let metadata_extractor = MetadataExtractor::new();
        let file_ext = document
            .file_path()
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        document.set_language(metadata_extractor.detect_language(&content, file_ext));
        document.set_category(metadata_extractor.categorize(
            &content,
            metadata.file_name(),
            metadata.mime_type(),
        ));
        document.set_quality_score(metadata_extractor.calculate_quality_score(
            &content,
            metadata.size_bytes(),
            metadata.modified_at(),
        ));
        document.set_word_count(metadata_extractor.count_words(&content));

        for index in 0..document.chunks().len() {
            let chunk = document.chunks().get(index).ok_or_else(|| {
                AppError::InvalidInput(format!(
                    "Chunk index {} out of bounds in index_file_no_persist",
                    index
                ))
            })?;
            let chunk_content = chunk.content().to_string();
            document.set_chunk_token_count(index, metadata_extractor.count_tokens(chunk.content()));
            document.set_chunk_word_count(
                index,
                metadata_extractor.count_words(&chunk_content) as usize,
            );
            document.set_chunk_has_code(index, metadata_extractor.contains_code(&chunk_content));
            document.set_chunk_section(index, metadata_extractor.extract_section(&chunk_content));
        }

        // 12. Generate embeddings (no transaction needed)
        let chunk_texts: Vec<String> = document
            .chunks()
            .iter()
            .map(|c| c.content().to_string())
            .collect();

        let embedding_start = Instant::now();
        let embeddings = self.embedding_service.embed_batch(&chunk_texts).await?;
        let embedding_duration = embedding_start.elapsed();

        // 13. Save document (PART 1 of atomic transaction)
        let db_save_start = Instant::now();
        document_repo.save(&document).await?;
        let db_save_duration = db_save_start.elapsed();

        // 14. Prepare and save embeddings (PART 2 of atomic transaction)
        let embedding_entries: Vec<(Embedding, Vec<f32>)> = document
            .chunks()
            .iter()
            .zip(embeddings.iter())
            .map(|(chunk, vec)| {
                (
                    Embedding::new(
                        chunk.id().clone(),
                        DEFAULT_EMBEDDING_MODEL_NAME.to_string(),
                        vec.len(),
                    ),
                    vec.clone(),
                )
            })
            .collect();

        let embedding_save_start = Instant::now();
        embedding_repo.save_batch(embedding_entries).await?;
        let embedding_save_duration = embedding_save_start.elapsed();

        // Update the shared in-memory vector index so newly indexed documents
        // are immediately searchable without requiring an app restart.
        if let Some(ref vs) = self.vector_search {
            let doc_id = document.document().id().to_string();
            let mut vs_added = 0usize;
            for (chunk, vec) in document.chunks().iter().zip(embeddings.iter()) {
                let embedding_id = format!("emb_{}", chunk.id().as_str());
                let content = chunk.content().to_string();
                let chunk_id = chunk.id().as_str().to_string();
                if vs
                    .add_embedding_with_content(
                        embedding_id,
                        vec.clone(),
                        content,
                        chunk_id,
                        doc_id.clone(),
                    )
                    .is_ok()
                {
                    vs_added += 1;
                }
            }
            tracing::debug!(
                vs_added,
                total_chunks = document.chunks().len(),
                "Updated in-memory vector index with new embeddings"
            );
        }

        // Both saves succeeded - caller must now commit the UoW transaction

        // Log performance metrics
        let total_duration = total_start.elapsed();
        info!(
            total_duration_ms = total_duration.as_millis(),
            extraction_pct = format!(
                "{:.1}%",
                (extraction_duration.as_secs_f64() / total_duration.as_secs_f64()) * 100.0
            ),
            embedding_generation_pct = format!(
                "{:.1}%",
                (embedding_duration.as_secs_f64() / total_duration.as_secs_f64()) * 100.0
            ),
            db_save_pct = format!(
                "{:.1}%",
                (db_save_duration.as_secs_f64() / total_duration.as_secs_f64()) * 100.0
            ),
            embedding_save_pct = format!(
                "{:.1}%",
                (embedding_save_duration.as_secs_f64() / total_duration.as_secs_f64()) * 100.0
            ),
            chunk_count = document.chunks().len(),
            transaction_mode = "single_uow",
            "File indexing completed (UoW) - awaiting commit"
        );

        Ok(IndexFileResponseDto {
            document_id: document.document().id().to_string(),
            chunks_created: document.chunks().len(),
            status: "imported".to_string(),
            error: None,
            file_path: library_path.to_str().unwrap_or("").to_string(),
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::{
        ContentAddressedStoragePort, EmbeddingRepositoryPort, FileMetadata, Filter, NoFilter,
    };
    use crate::features::embedding::entity::Embedding;
    use crate::domain::entities::Document;
    use crate::domain::repositories::UnitOfWorkFactory;
    use async_trait::async_trait;
    use std::path::Path;

    // Mock content-addressed storage
    struct MockContentStorage;

    #[async_trait]
    impl ContentAddressedStoragePort for MockContentStorage {
        async fn import_file(&self, source_path: &Path) -> Result<(std::path::PathBuf, String)> {
            // Return source path and valid SHA-256 hex string (64 characters)
            Ok((source_path.to_path_buf(), "a".repeat(64)))
        }

        async fn exists_by_hash(&self, _hash: &str) -> Result<bool> {
            Ok(false)
        }

        async fn get_path_by_hash(&self, _hash: &str) -> Result<Option<std::path::PathBuf>> {
            Ok(None)
        }
    }

    // Mock file storage
    struct MockFileStorage;

    #[async_trait]
    impl FileStoragePort for MockFileStorage {
        async fn read_file(&self, _path: &Path) -> Result<String> {
            Ok("This is test file content for indexing and chunking.".to_string())
        }

        async fn write_file(&self, _path: &Path, _content: &str) -> Result<()> {
            Ok(())
        }

        async fn delete_file(&self, _path: &Path) -> Result<()> {
            Ok(())
        }

        async fn read_file_bytes(&self, _path: &Path) -> Result<Vec<u8>> {
            Ok(b"Test content".to_vec())
        }

        async fn write_file_bytes(&self, _path: &Path, _content: &[u8]) -> Result<()> {
            Ok(())
        }

        async fn compute_hash(&self, _path: &Path) -> Result<String> {
            // Return valid SHA-256 hex string (64 characters)
            Ok("a".repeat(64))
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

    // Mock content extractor
    struct MockContentExtractor;

    #[async_trait]
    impl ContentExtractionPort for MockContentExtractor {
        async fn extract_content(
            &self,
            path: &Path,
        ) -> Result<crate::application::ports::ExtractedContentData> {
            use crate::application::ports::ExtractedContentData;
            // Read actual file content to support chunking tests
            let text = std::fs::read_to_string(path).unwrap_or_else(|_| {
                "This is test file content for indexing and chunking.".to_string()
            });
            let char_count = text.chars().count();
            let word_count = text.split_whitespace().count();

            Ok(ExtractedContentData {
                text,
                mime_type: "text/plain".to_string(),
                page_count: None,
                word_count,
                char_count,
            })
        }

        fn is_supported(&self, _path: &Path) -> bool {
            true
        }
    }

    // Mock embedder
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

    // Mock embedding repository
    struct MockEmbeddingRepo;

    #[async_trait]
    impl EmbeddingRepositoryPort for MockEmbeddingRepo {
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

    // Mock repository
    struct MockDocumentRepo;

    #[async_trait]
    impl DocumentRepositoryPort for MockDocumentRepo {
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

    #[async_trait]
    impl crate::application::ports::RepositoryPort<Document> for MockDocumentRepo {
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

        fn document_repository(&self) -> Result<Box<dyn DocumentRepositoryPort + Send + '_>> {
            Ok(Box::new(MockDocumentRepo))
        }

        fn embedding_repository(&self) -> Result<Box<dyn EmbeddingRepositoryPort + Send + '_>> {
            Ok(Box::new(MockEmbeddingRepo))
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

    #[tokio::test]
    #[ignore] // TODO: Modernize for RC2 (Refactor Drift)
    async fn test_index_file_execution() {
        use std::fs;
        use tempfile::TempDir;

        // Create temp file
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Test content").unwrap();

        let use_case = IndexFileUseCase::new(
            Arc::new(MockContentStorage),
            Arc::new(MockFileStorage),
            Arc::new(MockContentExtractor),
            Arc::new(MockEmbedder),
            Arc::new(MockDocumentRepo),
            Arc::new(MockEmbeddingRepo),
            Arc::new(MockUnitOfWorkFactory),
        );

        let request = IndexFileRequestDto {
            path: file_path.to_str().unwrap().to_string(),
            chunking_strategy:
                crate::features::indexing::dto::ChunkingStrategyDto::FixedSize { size: 512 },
            tags: None,
            metadata: None,
            space_id: None,
        };

        let response = use_case.execute(request).await.unwrap();

        assert!(!response.document_id.is_empty());
        // Mocks don't create real chunks - that's tested in integration tests
        assert_eq!(response.chunks_created, 0);
        assert_eq!(response.status, "imported");
        assert!(response.error.is_none());
        assert!(!response.file_path.is_empty());
    }

    #[tokio::test]
    #[ignore] // TODO: Modernize for RC2 (Refactor Drift)
    async fn test_index_file_with_chunking() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("long.txt");
        fs::write(&file_path, "A".repeat(1000)).unwrap();

        let use_case = IndexFileUseCase::new(
            Arc::new(MockContentStorage),
            Arc::new(MockFileStorage),
            Arc::new(MockContentExtractor),
            Arc::new(MockEmbedder),
            Arc::new(MockDocumentRepo),
            Arc::new(MockEmbeddingRepo),
            Arc::new(MockUnitOfWorkFactory),
        );

        let request = IndexFileRequestDto {
            path: file_path.to_str().unwrap().to_string(),
            chunking_strategy:
                crate::features::indexing::dto::ChunkingStrategyDto::FixedSize { size: 100 },
            tags: None,
            metadata: None,
            space_id: None,
        };

        let response = use_case.execute(request).await.unwrap();

        // Mocks don't create real chunks - that's tested in integration tests
        assert_eq!(response.chunks_created, 0);
    }
}
