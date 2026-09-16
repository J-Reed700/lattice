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
//! use lattice::application::use_cases::indexing::index_file::IndexFileUseCase;
//! use lattice::application::dtos::indexing_dto::{IndexFileRequestDto, ChunkingStrategyDto};
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

use crate::application::factories::{ChecksumFactory, FileMetadataFactory};
use crate::application::ports::UnitOfWorkFactory;
use crate::application::ports::{
    ChunkSparseTerms, ContentAddressedStoragePort, ContentExtractionPort, DocumentRepositoryPort,
    EmbeddingPort, EmbeddingRepositoryPort, FileStoragePort, SparseTermStorePort, VectorSearchPort,
};
use crate::domain::entities::Document;
use crate::domain::value_objects::SparseEmbedding;
use crate::features::embedding::entity::Embedding;
use crate::features::indexing::dto::{IndexFileRequestDto, IndexFileResponseDto};
use crate::features::indexing::mapper::IndexingMapper;
use crate::infrastructure::services::metadata_extraction::MetadataExtractor;
use crate::shared::domain_types::ValidatedFilePath;
use crate::shared::error::{AppError, Result};
use tracing::instrument;

// Type aliases for complex return types
/// Type alias for embedding with its vector representation.
pub type EmbeddingWithVector = (Embedding, Vec<f32>);

/// Type alias for prepared document ready for indexing.
/// (document, embeddings, library_path, imported_new, sparse_terms)
///
/// `sparse_terms` is one [`SparseEmbedding`] per chunk, in chunk order, and is
/// empty whenever the loaded model has no learned sparse head or no sparse
/// term store is wired. It travels with the dense vectors because both come
/// out of the same forward pass and must be committed for the same chunk ids.
pub type PreparedDocument = (
    Document,
    Vec<EmbeddingWithVector>,
    String,
    bool,
    Vec<SparseEmbedding>,
);

pub enum PrepareForIndexingOutcome {
    Prepared(Box<PreparedDocument>),
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
    /// Learned sparse term postings, written alongside the dense vectors.
    /// `None` leaves indexing byte-for-byte as it was.
    sparse_term_store: Option<Arc<dyn SparseTermStorePort>>,
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
            sparse_term_store: None,
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

    /// Persist learned sparse term weights beside the dense vectors.
    ///
    /// Wiring the store is not the same as producing rows: the sparse head is
    /// only read when the loaded embedding model actually has one
    /// (`EmbeddingPort::supports_sparse`), so this can be wired
    /// unconditionally and simply does nothing for a dense-only model.
    #[must_use]
    pub fn with_sparse_term_store(mut self, store: Arc<dyn SparseTermStorePort>) -> Self {
        self.sparse_term_store = Some(store);
        self
    }

    /// True when this indexing run should also produce sparse postings.
    fn sparse_indexing_enabled(&self) -> bool {
        self.sparse_term_store.is_some() && self.embedding_service.supports_sparse()
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
    /// # use lattice::application::use_cases::indexing::index_file::IndexFileUseCase;
    /// # use lattice::application::dtos::indexing_dto::{IndexFileRequestDto, ChunkingStrategyDto};
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
        match self.prepare_for_indexing(request).await? {
            PrepareForIndexingOutcome::Duplicate { document_id } => {
                let document = self
                    .document_repo
                    .find_by_id(&document_id)
                    .await?
                    .ok_or_else(|| {
                        AppError::NotFound(format!("Saved document {document_id} not found"))
                    })?;
                Ok(IndexFileResponseDto {
                    document_id,
                    chunks_created: document.chunks().len(),
                    status: "already_indexed".into(),
                    error: None,
                    file_path: document.file_path().display().to_string(),
                })
            }
            PrepareForIndexingOutcome::Prepared(prepared) => {
                let chunks_created = prepared.0.chunks().len();
                let file_path = prepared.2.clone();
                let document_id = self
                    .commit_prepared(*prepared, self.uow_factory.as_ref())
                    .await?;
                Ok(IndexFileResponseDto {
                    document_id,
                    chunks_created,
                    file_path,
                    status: "imported".into(),
                    error: None,
                })
            }
        }
    }

    /// Prepare file for indexing (extraction + embeddings) WITHOUT opening transaction.
    /// Returns prepared aggregate and embeddings ready to be saved.
    ///
    /// This is the preparation step of the batch operation.
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

        let requested_context: Option<crate::domain::value_objects::source_context::SourceContext> =
            request
                .metadata
                .as_ref()
                .and_then(|m| m.get("source_context"))
                .map(|s| serde_json::from_str(s))
                .transpose()?;
        if let Some(context) = &requested_context {
            context.group.validate()?;
        }
        let rebuild = request
            .metadata
            .as_ref()
            .and_then(|m| m.get("rebuild_existing"))
            .is_some_and(|s| s == "true");
        let existing = self
            .document_repo
            .find_by_checksum(&content_checksum)
            .await?;
        if let Some(existing) = &existing {
            if !rebuild
                && requested_context
                    .as_ref()
                    .is_none_or(|context| Some(context) == existing.source_context())
            {
                // The previous attempt may have committed SQLite and then failed
                // while publishing to runtime search. Finish that step on retry.
                self.publish_stored_document(existing.id().as_str()).await?;
                return Ok(PrepareForIndexingOutcome::Duplicate {
                    document_id: existing.id().to_string(),
                });
            }
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
        let page_ranges = extracted.page_ranges;
        let content = extracted.text;
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

        if let Some(mut saved) = existing {
            saved.reindex(content.clone(), chunking_strategy)?;
            document = saved;
        }
        if let Some(context) = requested_context {
            document.set_source_context(Some(context));
        }
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

        let (document, spans) = super::embedding_input::prepare_structured_with_spans(
            document,
            self.embedding_service.as_ref(),
            &page_ranges,
        )?;
        let model_identity = self.embedding_service.model_identity();
        // With late chunking each structure span is embedded once and pooled
        // per chunk; otherwise this is the per-chunk batch as before. Either
        // way it yields one vector per chunk, in chunk order.
        //
        // When the model has a sparse head and is *not* late chunking, both
        // representations come from one forward pass over exactly the texts
        // `embed_prepared_chunks` would have used, so the dense vectors are
        // unchanged. Late chunking pools dense vectors over a whole span and
        // has no equivalent pooling for term weights, so there the sparse head
        // is read from the per-chunk texts in a second pass instead.
        let (embeddings, sparse_terms) = if self.sparse_indexing_enabled() {
            let chunk_texts: Vec<String> = document
                .chunks()
                .iter()
                .map(|chunk| chunk.embedding_text())
                .collect();
            if self.embedding_service.uses_late_chunking() {
                let dense = super::embedding_input::embed_prepared_chunks(
                    &document,
                    self.embedding_service.as_ref(),
                    &spans,
                )
                .await?;
                (
                    dense,
                    self.embedding_service
                        .embed_sparse_batch(&chunk_texts)
                        .await?,
                )
            } else {
                self.embedding_service
                    .embed_batch_with_sparse(&chunk_texts)
                    .await?
            }
        } else {
            (
                super::embedding_input::embed_prepared_chunks(
                    &document,
                    self.embedding_service.as_ref(),
                    &spans,
                )
                .await?,
                Vec::new(),
            )
        };
        if self.embedding_service.model_identity() != model_identity
            || embeddings.len() != document.chunks().len()
        {
            return Err(AppError::InvalidState(
                "Embedding model changed or returned an incomplete batch; retry indexing".into(),
            ));
        }
        // A short sparse batch would silently key one chunk's terms to another
        // chunk's id, which is worse than having no sparse rows at all.
        let sparse_terms = if sparse_terms.len() == document.chunks().len() {
            sparse_terms
        } else {
            if !sparse_terms.is_empty() {
                tracing::warn!(
                    expected = document.chunks().len(),
                    received = sparse_terms.len(),
                    "Sparse head returned an incomplete batch; indexing this document without sparse terms"
                );
            }
            Vec::new()
        };

        // Prepare embedding entries
        let embedding_entries: Vec<(Embedding, Vec<f32>)> = document
            .chunks()
            .iter()
            .zip(embeddings.iter())
            .map(|(chunk, vec)| {
                let embedding =
                    Embedding::new(chunk.id().clone(), model_identity.clone(), vec.len());
                (embedding, vec.clone())
            })
            .collect();

        Ok(PrepareForIndexingOutcome::Prepared(Box::new((
            document,
            embedding_entries,
            library_path.to_str().unwrap_or("").to_string(),
            imported_new,
            sparse_terms,
        ))))
    }

    /// Commit a prepared document atomically, then publish it to runtime search.
    /// Preparation never holds a database transaction or another file's data.
    pub async fn commit_prepared(
        &self,
        prepared: PreparedDocument,
        factory: &dyn UnitOfWorkFactory,
    ) -> Result<String> {
        let (mut document, embeddings, _library_path, _imported_new, sparse_terms) = prepared;
        // Status and the complete chunk/vector set commit in the same transaction.
        document.mark_indexed();
        let document_id = document.id().to_string();
        let stale_chunk_ids: Vec<_> = self
            .embedding_repo
            .find_by_document_id(&document_id)
            .await?
            .into_iter()
            .map(|(embedding, _)| embedding.chunk_id().to_string())
            .collect();
        let mut uow = factory.create().await?;
        let save_result: Result<()> = async {
            let documents = uow.document_repository()?;
            let vectors = uow.embedding_repository()?;
            documents.save(&document).await?;
            vectors.save_batch(embeddings.clone()).await?;
            Ok(())
        }
        .await;
        if let Err(error) = save_result {
            uow.rollback().await?;
            return Err(error);
        }
        uow.commit().await?;
        // After the commit, never inside it: the postings reference
        // `text_chunks(id)`, so the chunk rows have to exist first. A failure
        // here costs this document its sparse branch and nothing else — the
        // dense vectors are already durable and the other two branches still
        // find it.
        self.store_sparse_terms(&document, &sparse_terms).await;
        if let Some(search) = self.vector_search.clone() {
            let stale_keys: Vec<_> = stale_chunk_ids
                .iter()
                .map(|id| crate::features::embedding::encoding::vector_key(id))
                .collect();
            tokio::task::spawn_blocking(move || search.remove_embeddings(&stale_keys))
                .await
                .map_err(|e| AppError::Other(format!("Search cleanup task failed: {e}")))??;
        }
        self.publish_document(&document, embeddings).await?;
        crate::features::cache::query_cache::invalidate_query_cache();
        // Summaries are a background tier: this returns before the model is
        // even consulted, and does nothing at all while the tier is off.
        crate::features::summaries::notify_document_indexed(&document_id);
        Ok(document_id)
    }

    /// Write this document's learned sparse postings, replacing whatever the
    /// previous indexing run left behind for the same chunks.
    ///
    /// Deliberately infallible. Sparse retrieval is one of three branches; a
    /// store that is unavailable must degrade the ranking, not fail an import
    /// whose document, chunks and vectors have already been committed.
    async fn store_sparse_terms(&self, document: &Document, sparse_terms: &[SparseEmbedding]) {
        let Some(store) = self.sparse_term_store.as_ref() else {
            return;
        };
        if sparse_terms.is_empty() {
            return;
        }
        let entries: Vec<ChunkSparseTerms> = document
            .chunks()
            .iter()
            .zip(sparse_terms.iter())
            .map(|(chunk, sparse)| (chunk.id().to_string(), sparse.clone()))
            .collect();
        let model_identity = self.embedding_service.model_identity();
        if let Err(error) = store.replace_chunk_terms(&model_identity, &entries).await {
            tracing::warn!(
                document_id = %document.id(),
                %error,
                "Could not persist learned sparse terms; this document will be found by the dense and BM25 branches only"
            );
        }
    }

    async fn publish_stored_document(&self, document_id: &str) -> Result<()> {
        if self.vector_search.is_none() {
            return Ok(());
        }
        let document = self
            .document_repo
            .find_by_id(document_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Saved document {document_id} not found")))?;
        let embeddings = self.embedding_repo.find_by_document_id(document_id).await?;
        self.publish_document(&document, embeddings).await?;
        crate::features::cache::query_cache::invalidate_query_cache();
        Ok(())
    }

    async fn publish_document(
        &self,
        document: &Document,
        embeddings: Vec<EmbeddingWithVector>,
    ) -> Result<()> {
        let Some(search) = self.vector_search.clone() else {
            return Ok(());
        };
        // Match by ID: repository ordering need not match chunk order.
        let mut vectors: std::collections::HashMap<_, _> = embeddings
            .into_iter()
            .map(|(embedding, vector)| (embedding.chunk_id().to_string(), vector))
            .collect();
        let entries: Result<Vec<_>> = document
            .chunks()
            .iter()
            .map(|chunk| {
                let embedding = vectors.remove(chunk.id().as_str()).ok_or_else(|| {
                    AppError::InvalidState(format!(
                        "Saved document is missing an embedding for chunk {}",
                        chunk.id()
                    ))
                })?;
                Ok(
                    crate::application::ports::vector_search_port::VectorIndexEntry {
                        id: format!("emb_{}", chunk.id()),
                        embedding,
                        content: chunk.content().to_owned(),
                        chunk_id: chunk.id().to_string(),
                        document_id: document.id().to_string(),
                    },
                )
            })
            .collect();
        let entries = entries?;
        // HNSW updates and disk serialization must not block async DB/chat work.
        tokio::task::spawn_blocking(move || search.publish_embeddings(entries)).await
            .map_err(|error| AppError::Other(format!("Search indexing task failed: {error}")))?
            .map_err(|error| AppError::Other(format!("Document saved, but search indexing failed: {error}. Retry this file to finish indexing.")))
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::UnitOfWorkFactory;
    use crate::application::ports::{ContentAddressedStoragePort, EmbeddingRepositoryPort};
    use crate::domain::entities::Document;
    use crate::features::embedding::entity::Embedding;
    use async_trait::async_trait;
    use std::path::Path;

    struct MockContentStorage;

    #[async_trait]
    impl ContentAddressedStoragePort for MockContentStorage {
        async fn import_file(&self, source_path: &Path) -> Result<(std::path::PathBuf, String)> {
            Ok((source_path.to_path_buf(), "a".repeat(64)))
        }

        async fn exists_by_hash(&self, _hash: &str) -> Result<bool> {
            Ok(false)
        }

        async fn get_path_by_hash(&self, _hash: &str) -> Result<Option<std::path::PathBuf>> {
            Ok(None)
        }
    }

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

    struct MockContentExtractor;

    #[async_trait]
    impl ContentExtractionPort for MockContentExtractor {
        async fn extract_content(
            &self,
            path: &Path,
        ) -> Result<crate::application::ports::ExtractedContentData> {
            use crate::application::ports::ExtractedContentData;
            let text = std::fs::read_to_string(path).unwrap_or_else(|_| {
                "This is test file content for indexing and chunking.".to_string()
            });
            let char_count = text.chars().count();
            let word_count = text.split_whitespace().count();

            Ok(ExtractedContentData {
                text,
                mime_type: "text/plain".to_string(),
                page_count: None,
                page_ranges: Vec::new(),
                word_count,
                char_count,
            })
        }

        fn is_supported(&self, _path: &Path) -> bool {
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

    /// Embedder with a real input budget: it splits a passage into fixed-width
    /// windows. `embedding_input::prepare_structured_with_spans` rebuilds the chunk list
    /// from `EmbeddingPort::split_text`, so this is what actually decides how
    /// many chunks an indexed document ends up with.
    struct MockSplittingEmbedder {
        window: usize,
    }

    #[async_trait]
    impl EmbeddingPort for MockSplittingEmbedder {
        async fn embed_single(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![0.1, 0.2, 0.3])
        }

        async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![0.1, 0.2, 0.3]).collect())
        }

        fn split_text(
            &self,
            text: &str,
            _prefix: &str,
        ) -> Result<Vec<crate::application::ports::embedding_port::EmbeddingTextChunk>> {
            use crate::application::ports::embedding_port::EmbeddingTextChunk;

            let mut parts = Vec::new();
            let mut start = 0usize;
            while start < text.len() {
                let mut end = (start + self.window).min(text.len());
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                parts.push(EmbeddingTextChunk {
                    text: text[start..end].to_string(),
                    start,
                    end,
                    // Keeps the context-prefix budget loop in `prepare_structured_with_spans`
                    // from shrinking the prefix.
                    token_count: 0,
                });
                start = end;
            }
            Ok(parts)
        }

        fn dimension(&self) -> usize {
            3
        }

        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }

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

        async fn find_all_paginated(&self, _limit: usize) -> Result<Vec<Document>> {
            Ok(vec![])
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
    impl crate::application::ports::UnitOfWork for MockUnitOfWork {
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
        ) -> Result<Box<dyn crate::application::ports::unit_of_work::ModelRepositoryPort + '_>>
        {
            Err(AppError::InvalidState(
                "Model repository not used in test".to_string(),
            ))
        }

        fn model_file_repository(
            &self,
        ) -> Result<Box<dyn crate::application::ports::unit_of_work::ModelFileRepositoryPort + '_>>
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
        async fn create(&self) -> Result<Box<dyn crate::application::ports::UnitOfWork + Send>> {
            Ok(Box::new(MockUnitOfWork))
        }
    }

    #[tokio::test]
    async fn test_index_file_execution() {
        use std::fs;
        use tempfile::TempDir;

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
            chunking_strategy: crate::features::indexing::dto::ChunkingStrategyDto::FixedSize {
                size: 512,
            },
            tags: None,
            metadata: None,
            space_id: None,
        };

        let response = use_case.execute(request).await.unwrap();

        assert!(!response.document_id.is_empty());
        // `prepare_structured_with_spans` discards the strategy-produced chunks and rebuilds
        // them from `EmbeddingPort::split_text`. `MockEmbedder` keeps the port's
        // default, which returns the whole passage as a single part, and this
        // short unstructured file is one structure span, so exactly one chunk.
        assert_eq!(response.chunks_created, 1);
        assert_eq!(response.status, "imported");
        assert!(response.error.is_none());
        assert!(!response.file_path.is_empty());
    }

    #[tokio::test]
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
            Arc::new(MockSplittingEmbedder { window: 100 }),
            Arc::new(MockDocumentRepo),
            Arc::new(MockEmbeddingRepo),
            Arc::new(MockUnitOfWorkFactory),
        );

        let request = IndexFileRequestDto {
            path: file_path.to_str().unwrap().to_string(),
            chunking_strategy: crate::features::indexing::dto::ChunkingStrategyDto::FixedSize {
                size: 100,
            },
            tags: None,
            metadata: None,
            space_id: None,
        };

        let response = use_case.execute(request).await.unwrap();

        // The 1000-byte file has no headings, so it is one structure span that the
        // embedder splits into 1000 / 100 windows.
        assert_eq!(response.chunks_created, 10);
    }
}
