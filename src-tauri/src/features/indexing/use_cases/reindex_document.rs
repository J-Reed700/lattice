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
//! use lattice_core::application::use_cases::indexing::reindex_document::ReindexDocumentUseCase;
//!
//! # async fn example(use_case: ReindexDocumentUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let response = use_case.execute("doc-123".to_string()).await?;
//! println!("Reindexed with {} chunks", response.chunks_created);
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use crate::features::embedding::entity::Embedding;
use crate::features::indexing::dto::IndexFileResponseDto;
use crate::infrastructure::services::metadata_extraction::MetadataExtractor;
use lattice_core::application::factories::FileMetadataFactory;
use lattice_core::application::ports::UnitOfWorkFactory;
use lattice_core::application::ports::{
    EmbeddingPort, FileStoragePort, RepositoryPort, SearchResultCachePort, VectorSearchPort,
};
use lattice_core::domain::entities::Document;
use lattice_core::domain::value_objects::chunking_strategy::ChunkingStrategy;
use lattice_core::shared::error::{AppError, Result};

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
    content_extractor: Option<Arc<dyn lattice_core::application::ports::ContentExtractionPort>>,
    embedding_service: Arc<dyn EmbeddingPort>,
    document_repo: Arc<dyn RepositoryPort<Document>>,
    uow_factory: Arc<dyn UnitOfWorkFactory>,
    /// The live vector index.
    ///
    /// Reindex used to update SQLite only. The in-memory USearch index kept
    /// the vectors of chunks that no longer existed, and never learned about
    /// the new ones — so after editing a file, semantic search returned
    /// dangling results for deleted chunks and never surfaced the new content.
    /// A restart did not repair it either, because the persisted index is only
    /// rebuilt when it is *empty*.
    vector_search: Arc<dyn VectorSearchPort>,
    /// Learned sparse postings. Reindexing replaces this document's chunk rows,
    /// which cascades the old postings away; without rewriting them the
    /// document would silently drop out of the sparse branch until it was
    /// imported again.
    sparse_term_store: Option<Arc<dyn lattice_core::application::ports::SparseTermStorePort>>,
    /// Search results cached against the corpus, dropped once the rebuilt
    /// chunks are live.
    query_cache: Option<Arc<dyn SearchResultCachePort>>,
}

impl ReindexDocumentUseCase {
    /// Persist learned sparse term weights alongside the rebuilt vectors.
    #[must_use]
    pub fn with_sparse_term_store(
        mut self,
        store: Arc<dyn lattice_core::application::ports::SparseTermStorePort>,
    ) -> Self {
        self.sparse_term_store = Some(store);
        self
    }

    pub fn with_content_extractor(
        mut self,
        extractor: Arc<dyn lattice_core::application::ports::ContentExtractionPort>,
    ) -> Self {
        self.content_extractor = Some(extractor);
        self
    }

    /// Invalidate `cache` whenever a document is rebuilt.
    #[must_use]
    pub fn with_query_cache(mut self, cache: Arc<dyn SearchResultCachePort>) -> Self {
        self.query_cache = Some(cache);
        self
    }

    fn invalidate_query_cache(&self) {
        if let Some(cache) = &self.query_cache {
            cache.invalidate_corpus();
        }
    }
    /// The text, its page ranges, and the pages OCR read.
    async fn extract(
        &self,
        path: &std::path::Path,
    ) -> Result<(String, Vec<(usize, usize, usize)>, Vec<u32>)> {
        match &self.content_extractor {
            Some(extractor) => {
                let data = extractor.extract_content(path).await?;
                Ok((data.text, data.page_ranges, data.ocr_pages))
            }
            None => Ok((
                self.file_storage.read_file(path).await?,
                Vec::new(),
                Vec::new(),
            )),
        }
    }

    /// Create a new reindex document use case.
    ///
    /// # Arguments
    ///
    /// * `file_storage` - Service for reading file content
    /// * `embedding_service` - Service for generating embeddings
    /// * `document_repo` - Repository for document persistence
    /// * `embedding_repo` - Repository for persisting embeddings
    /// * `vector_search` - Live vector index, kept in step with the database
    pub fn new(
        file_storage: Arc<dyn FileStoragePort>,
        embedding_service: Arc<dyn EmbeddingPort>,
        document_repo: Arc<dyn RepositoryPort<Document>>,
        uow_factory: Arc<dyn UnitOfWorkFactory>,
        vector_search: Arc<dyn VectorSearchPort>,
    ) -> Self {
        Self {
            file_storage,
            content_extractor: None,
            embedding_service,
            document_repo,
            uow_factory,
            vector_search,
            sparse_term_store: None,
            query_cache: None,
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
        self.execute_internal(document_id, None).await
    }

    async fn execute_internal(
        &self,
        document_id: String,
        requested_strategy: Option<ChunkingStrategy>,
    ) -> Result<IndexFileResponseDto> {
        // 1. Retrieve existing document
        let mut document = self
            .document_repo
            .find_by_id(&document_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Document not found: {}", document_id)))?;

        // Capture the pre-reindex chunk ids. `document.reindex` replaces the
        // chunk set, so after that call there is no way to know which vectors
        // to evict from the index.
        let stale_chunk_ids: Vec<String> = document
            .chunks()
            .iter()
            .map(|c| c.id().to_string())
            .collect();

        // 2. Read current file content (clone to avoid borrow conflict)
        let file_path = document.file_path().to_path_buf();
        let (content, pages, ocr_pages) = self.extract(&file_path).await?;
        // The import path refuses a file with no text; a reindex must too, or
        // it replaces every chunk and vector with nothing and still marks the
        // document indexed. Failing here leaves the old chunks in place.
        if content.trim().is_empty() {
            return Err(AppError::InvalidInput(format!(
                "File contains no extractable text: {}",
                file_path.display()
            )));
        }

        // 3. Reindex with an inferred strategy based on existing chunk sizes,
        // then adapt for new content length.
        let strategy = requested_strategy
            .unwrap_or_else(|| ChunkingStrategy::infer_from_existing_chunks(document.chunks()))
            .adapt_for_content(&content);
        document.reindex(content.clone(), strategy)?;

        // 3a. Extract and set metadata
        let metadata_extractor = MetadataExtractor::new();

        let file_ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

        let file_metadata = FileMetadataFactory::from_path(&file_path)?;

        // Detect language
        let language = metadata_extractor.detect_language(&content, file_ext);

        // Categorize
        let category = metadata_extractor.categorize(
            &content,
            file_metadata.file_name(),
            file_metadata.mime_type(),
        );

        let quality_score = metadata_extractor.calculate_quality_score(
            &content,
            file_metadata.size_bytes(),
            file_metadata.modified_at(),
        );

        // Count words
        let word_count = metadata_extractor.count_words(&content);

        document.set_language(language.clone());
        document.set_category(category);
        document.set_quality_score(quality_score);
        document.set_word_count(word_count);

        let chunk_count = document.chunks().len();
        for index in 0..chunk_count {
            let chunk = document.chunks().get(index).ok_or_else(|| {
                AppError::InvalidInput(format!("Chunk index {} out of bounds", index))
            })?;
            let token_count = metadata_extractor.count_tokens(chunk.content());
            document.set_chunk_token_count(index, token_count);
        }

        // 4. Generate new embeddings
        //
        // Span-grouped, like the import path: with late chunking each structure
        // span is embedded once and pooled per chunk, and otherwise this is the
        // same per-chunk batch as before. Embedding chunk-first here while the
        // importer late-chunks would write two vector spaces into one
        // generation — the stored chunk text and its offsets are identical
        // either way, but the vectors are not comparable.
        let (mut document, spans) = super::embedding_input::prepare_structured_with_spans(
            document,
            self.embedding_service.as_ref(),
            &pages,
        )?;
        // From this extraction alone: a page that has gained a text layer since
        // the last run loses its mark.
        document.mark_ocr_pages(&ocr_pages);
        let model_identity = self.embedding_service.model_identity();

        // Same bargain as the import path: with a sparse head and a store
        // wired, one forward pass yields both representations over exactly the
        // texts the chunk-first path would have embedded. Late chunking pools
        // dense vectors across a span and has no equivalent for term weights,
        // so there the sparse head is read from the chunk texts separately.
        let sparse_wanted =
            self.sparse_term_store.is_some() && self.embedding_service.supports_sparse();
        let (embeddings, sparse_terms) =
            if sparse_wanted && !self.embedding_service.uses_late_chunking() {
                let chunk_texts: Vec<String> = document
                    .chunks()
                    .iter()
                    .map(|chunk| chunk.embedding_text())
                    .collect();
                self.embedding_service
                    .embed_batch_with_sparse(&chunk_texts)
                    .await?
            } else {
                let dense = super::embedding_input::embed_prepared_chunks(
                    &document,
                    self.embedding_service.as_ref(),
                    &spans,
                )
                .await?;
                let sparse = if sparse_wanted {
                    let chunk_texts: Vec<String> = document
                        .chunks()
                        .iter()
                        .map(|chunk| chunk.embedding_text())
                        .collect();
                    self.embedding_service
                        .embed_sparse_batch(&chunk_texts)
                        .await?
                } else {
                    Vec::new()
                };
                (dense, sparse)
            };
        if self.embedding_service.model_identity() != model_identity
            || embeddings.len() != document.chunks().len()
        {
            return Err(AppError::InvalidState(
                "Embedding model changed or returned an incomplete batch; retry indexing".into(),
            ));
        }

        // 5. Create and persist new embeddings
        let embedding_entries: Vec<(Embedding, Vec<f32>)> = document
            .chunks()
            .iter()
            .zip(embeddings.iter())
            .map(|(chunk, embedding_vec)| {
                let embedding_metadata = Embedding::new(
                    chunk.id().clone(),
                    model_identity.clone(),
                    embedding_vec.len(),
                );
                (embedding_metadata, embedding_vec.clone())
            })
            .collect();

        document.mark_indexed();
        let mut uow = self.uow_factory.create().await?;
        let db_result = {
            let document_repo = uow.document_repository()?;
            let embedding_repo = uow.embedding_repository()?;

            embedding_repo
                .delete_by_document_id(document.id().as_str())
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

        // After the commit: the postings reference chunk ids that only exist
        // once `document_repo.save` has replaced this document's chunk rows.
        if let Some(store) = self.sparse_term_store.as_ref() {
            if sparse_terms.len() == document.chunks().len() {
                let entries: Vec<lattice_core::application::ports::ChunkSparseTerms> = document
                    .chunks()
                    .iter()
                    .zip(sparse_terms.iter())
                    .map(|(chunk, sparse)| (chunk.id().to_string(), sparse.clone()))
                    .collect();
                if let Err(error) = store.replace_chunk_terms(&model_identity, &entries).await {
                    tracing::warn!(
                        %error,
                        "Could not persist learned sparse terms during reindex; this document will be found by the dense and BM25 branches only"
                    );
                }
            } else if !sparse_terms.is_empty() {
                tracing::warn!(
                    expected = document.chunks().len(),
                    received = sparse_terms.len(),
                    "Sparse head returned an incomplete batch; reindexing without sparse terms"
                );
            }
        }

        // Publish complete source metadata in batches, away from the async runtime.
        // SQLite has committed; report publication failures instead of claiming success.
        let stale_keys: Vec<_> = stale_chunk_ids
            .iter()
            .map(|id| lattice_core::shared::persistence::embedding_encoding::vector_key(id))
            .collect();
        let entries = document
            .chunks()
            .iter()
            .zip(embeddings)
            .map(|(chunk, embedding)| {
                lattice_core::application::ports::vector_search_port::VectorIndexEntry {
                    id: lattice_core::shared::persistence::embedding_encoding::vector_key(
                        chunk.id().as_str(),
                    ),
                    embedding,
                    content: chunk.content().to_owned(),
                    chunk_id: chunk.id().to_string(),
                    document_id: document_id.clone(),
                }
            })
            .collect();
        let search = Arc::clone(&self.vector_search);
        let publication = tokio::task::spawn_blocking(move || {
            search.remove_embeddings(&stale_keys)?;
            search.publish_embeddings(entries)
        })
        .await
        .map_err(|e| AppError::Other(format!("Search indexing task failed: {e}")))?;
        self.invalidate_query_cache();
        publication.map_err(|e| {
            AppError::Other(format!(
                "Document saved, but search indexing failed: {e}. Retry reindexing this document."
            ))
        })?;

        // The corpus changed; cached search results are stale.
        self.invalidate_query_cache();

        // 8. Build response
        let file_path_str = document
            .file_path()
            .to_str()
            .ok_or_else(|| AppError::InvalidInput("Invalid file path encoding".to_string()))?;

        Ok(IndexFileResponseDto {
            document_id: document.id().to_string(),
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
        self.execute_internal(document_id, Some(strategy)).await
    }
}

#[cfg(test)]
mod tests;
