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
//! use lattice_core::application::use_cases::indexing::index_file::IndexFileUseCase;
//! use lattice_core::application::dtos::indexing_dto::{IndexFileRequestDto, ChunkingStrategyDto};
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

use std::sync::Arc;

use crate::features::indexing::dto::{IndexFileRequestDto, IndexFileResponseDto};
use crate::features::indexing::LibraryGc;
use lattice_core::application::ports::UnitOfWorkFactory;
use lattice_core::application::ports::{
    BlobLease, ContentAddressedStoragePort, ContentExtractionPort, DocumentIndexObserver,
    DocumentRepositoryPort, EmbeddingPort, EmbeddingRepositoryPort, FileStoragePort,
    SearchResultCachePort, VectorSearchPort,
};
use lattice_core::application::services::live_index::{
    publish_chunks_to_live_index, PublishedChunk,
};
use lattice_core::domain::entities::Document;
use lattice_core::domain::entities::Embedding;
use lattice_core::shared::error::{AppError, Result};
use tracing::instrument;

mod prepare;

// Type aliases for complex return types
/// Type alias for embedding with its vector representation.
pub type EmbeddingWithVector = (Embedding, Vec<f32>);

/// A document prepared for indexing: everything the commit needs, plus the
/// lease that keeps its library blob alive until that commit lands.
pub struct PreparedDocument {
    /// The aggregate, chunked and annotated, not yet marked indexed.
    pub document: Document,
    /// One dense vector per chunk, in chunk order.
    pub embeddings: Vec<EmbeddingWithVector>,
    /// Where the blob lives in the library.
    pub library_path: String,
    /// True when this import copied the blob into the library for the first
    /// time, which is what makes a failure from here on leave an orphan.
    pub imported_new: bool,
    /// Held from the copy until the document row commits. Dropping it any
    /// earlier lets a concurrent delete or sweep remove the blob this
    /// document is about to point at.
    pub lease: BlobLease,
}

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
    /// Search results cached against the corpus, dropped once a document
    /// lands so a repeated query sees it.
    query_cache: Option<Arc<dyn SearchResultCachePort>>,
    /// Told when a document is committed; the summary tier queues it.
    index_observer: Option<Arc<dyn DocumentIndexObserver>>,
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
            query_cache: None,
            index_observer: None,
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

    /// Invalidate `cache` whenever a document becomes searchable.
    #[must_use]
    pub fn with_query_cache(mut self, cache: Arc<dyn SearchResultCachePort>) -> Self {
        self.query_cache = Some(cache);
        self
    }

    /// Tell `observer` about every document this use case commits.
    #[must_use]
    pub fn with_index_observer(mut self, observer: Arc<dyn DocumentIndexObserver>) -> Self {
        self.index_observer = Some(observer);
        self
    }

    fn invalidate_query_cache(&self) {
        if let Some(cache) = &self.query_cache {
            cache.invalidate_corpus();
        }
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
    /// # use lattice_core::application::use_cases::indexing::index_file::IndexFileUseCase;
    /// # use lattice_core::application::dtos::indexing_dto::{IndexFileRequestDto, ChunkingStrategyDto};
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
                let chunks_created = prepared.document.chunks().len();
                let file_path = prepared.library_path.clone();
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

    /// Hand back the blob an import copied but never referenced.
    ///
    /// Best effort by design: the import has already failed, and a blob left
    /// behind is collected by the next startup sweep. Runs the same
    /// [`LibraryGc`] path as deletion, so a blob another document shares is
    /// still refused.
    async fn release_failed_import(&self, hash: &str) {
        let gc = LibraryGc::new(
            Arc::clone(&self.content_storage),
            Arc::clone(&self.document_repo),
        );
        match gc.release(hash).await {
            Ok(removal) => tracing::info!(
                %hash,
                ?removal,
                "Rolled back the library blob of a failed import"
            ),
            Err(error) => tracing::warn!(
                %hash,
                %error,
                "Could not roll back a failed import's blob; the next startup sweep will collect it"
            ),
        }
    }

    /// Abandon a prepared document that will never be committed.
    ///
    /// Cancellation lands between preparation and the commit, and from there
    /// nothing will ever reference the blob this import copied. Hand it back
    /// through the same [`LibraryGc`] path a failed commit uses, rather than
    /// leaving an orphan for the next startup sweep.
    pub async fn discard_prepared(&self, prepared: PreparedDocument) {
        let PreparedDocument {
            imported_new,
            lease,
            ..
        } = prepared;
        let library_hash = lease.hash().to_string();
        // Release the lease first, or the collector would refuse its own
        // caller's blob as still being imported.
        drop(lease);
        if imported_new {
            self.release_failed_import(&library_hash).await;
        }
    }

    /// Commit a prepared document atomically, then publish it to runtime search.
    /// Preparation never holds a database transaction or another file's data.
    pub async fn commit_prepared(
        &self,
        prepared: PreparedDocument,
        factory: &dyn UnitOfWorkFactory,
    ) -> Result<String> {
        let PreparedDocument {
            mut document,
            embeddings,
            library_path: _,
            imported_new,
            lease,
        } = prepared;
        let library_hash = lease.hash().to_string();
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
            let rollback = uow.rollback().await;
            // Nothing will reference the blob now; release it before the
            // error propagates.
            drop(lease);
            if imported_new {
                self.release_failed_import(&library_hash).await;
            }
            rollback?;
            return Err(error);
        }
        uow.commit().await?;
        // The row is durable, so the document itself now protects the blob and
        // this import's lease has done its job.
        drop(lease);
        if let Some(search) = self.vector_search.clone() {
            let stale_keys: Vec<_> = stale_chunk_ids
                .iter()
                .map(|id| lattice_core::shared::persistence::embedding_encoding::vector_key(id))
                .collect();
            tokio::task::spawn_blocking(move || search.remove_embeddings(&stale_keys))
                .await
                .map_err(|e| AppError::Other(format!("Search cleanup task failed: {e}")))??;
        }
        self.publish_document(&document, embeddings).await?;
        self.invalidate_query_cache();
        // Summaries are a background tier: this returns before the model is
        // even consulted, and does nothing at all while the tier is off.
        if let Some(observer) = &self.index_observer {
            observer.document_indexed(&document_id).await;
        }
        Ok(document_id)
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
        self.invalidate_query_cache();
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
        let chunks: Result<Vec<_>> = document
            .chunks()
            .iter()
            .map(|chunk| {
                let embedding = vectors.remove(chunk.id().as_str()).ok_or_else(|| {
                    AppError::InvalidState(format!(
                        "Saved document is missing an embedding for chunk {}",
                        chunk.id()
                    ))
                })?;
                Ok(PublishedChunk {
                    chunk_id: chunk.id().to_string(),
                    content: chunk.content().to_owned(),
                    embedding,
                })
            })
            .collect();
        publish_chunks_to_live_index(
            search,
            self.query_cache.as_deref(),
            &document.id().to_string(),
            chunks?,
        )
        .await
    }
}

#[cfg(test)]
mod tests;
