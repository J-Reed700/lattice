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
//! use lattice::application::use_cases::indexing::reindex_document::ReindexDocumentUseCase;
//!
//! # async fn example(use_case: ReindexDocumentUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let response = use_case.execute("doc-123".to_string()).await?;
//! println!("Reindexed with {} chunks", response.chunks_created);
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use crate::application::factories::FileMetadataFactory;
use crate::application::ports::UnitOfWorkFactory;
use crate::application::ports::{EmbeddingPort, FileStoragePort, RepositoryPort, VectorSearchPort};
use crate::domain::entities::Document;
use crate::domain::value_objects::chunking_strategy::ChunkingStrategy;
use crate::features::embedding::entity::Embedding;
use crate::features::indexing::dto::IndexFileResponseDto;
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
    content_extractor: Option<Arc<dyn crate::application::ports::ContentExtractionPort>>,
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
    sparse_term_store: Option<Arc<dyn crate::application::ports::SparseTermStorePort>>,
}

impl ReindexDocumentUseCase {
    /// Persist learned sparse term weights alongside the rebuilt vectors.
    #[must_use]
    pub fn with_sparse_term_store(
        mut self,
        store: Arc<dyn crate::application::ports::SparseTermStorePort>,
    ) -> Self {
        self.sparse_term_store = Some(store);
        self
    }

    pub fn with_content_extractor(
        mut self,
        extractor: Arc<dyn crate::application::ports::ContentExtractionPort>,
    ) -> Self {
        self.content_extractor = Some(extractor);
        self
    }
    async fn extract(
        &self,
        path: &std::path::Path,
    ) -> Result<(String, Vec<(usize, usize, usize)>)> {
        match &self.content_extractor {
            Some(extractor) => {
                let data = extractor.extract_content(path).await?;
                Ok((data.text, data.page_ranges))
            }
            None => Ok((self.file_storage.read_file(path).await?, Vec::new())),
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
        let (content, pages) = self.extract(&file_path).await?;

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
                let entries: Vec<crate::application::ports::ChunkSparseTerms> = document
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
            .map(|id| crate::features::embedding::encoding::vector_key(id))
            .collect();
        let entries = document
            .chunks()
            .iter()
            .zip(embeddings)
            .map(|(chunk, embedding)| {
                crate::application::ports::vector_search_port::VectorIndexEntry {
                    id: crate::features::embedding::encoding::vector_key(chunk.id().as_str()),
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
        crate::features::cache::query_cache::invalidate_query_cache();
        publication.map_err(|e| {
            AppError::Other(format!(
                "Document saved, but search indexing failed: {e}. Retry reindexing this document."
            ))
        })?;

        // The corpus changed; cached search results are stale.
        crate::features::cache::query_cache::invalidate_query_cache();

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
mod tests {
    use super::*;
    use crate::application::ports::UnitOfWorkFactory;
    use crate::application::ports::{EmbeddingRepositoryPort, FileMetadata, Filter};
    use crate::domain::entities::Document;
    use crate::features::embedding::entity::Embedding;
    use crate::shared::domain_types::ValidatedFilePath;
    use async_trait::async_trait;
    use std::fs;
    use std::path::Path;
    use std::sync::Mutex;
    use tempfile::TempDir;

    /// Records what reindex asks the vector index to do, so tests can assert
    /// the index is kept in step with the database rather than silently
    /// diverging from it.
    #[derive(Default)]
    struct MockVectorSearch {
        added: Mutex<Vec<String>>,
        removed: Mutex<Vec<String>>,
        batches: Mutex<(usize, usize)>,
        published: Mutex<Vec<(String, String)>>,
    }

    impl VectorSearchPort for MockVectorSearch {
        fn search(
            &self,
            _query_embedding: &[f32],
            _top_k: usize,
            _threshold: f32,
        ) -> Result<Vec<crate::features::search::dto::SearchResultPortDto>> {
            Ok(Vec::new())
        }

        fn search_scoped(
            &self,
            query_embedding: &[f32],
            top_k: usize,
            threshold: f32,
            allowed_document_ids: Option<&std::collections::HashSet<String>>,
        ) -> Result<Vec<crate::features::search::dto::SearchResultPortDto>> {
            let mut results = self.search(query_embedding, top_k, threshold)?;
            if let Some(scope) = allowed_document_ids {
                results.retain(|result| scope.contains(&result.doc_id));
                results.truncate(top_k);
            }
            Ok(results)
        }

        fn add_embedding(&self, id: String, _embedding: Vec<f32>) -> Result<()> {
            self.added.lock().unwrap().push(id);
            Ok(())
        }

        fn remove_embedding(&self, id: &str) -> Result<()> {
            self.removed.lock().unwrap().push(id.to_string());
            Ok(())
        }

        fn remove_embeddings(&self, ids: &[String]) -> Result<()> {
            self.batches.lock().unwrap().0 += 1;
            self.removed.lock().unwrap().extend_from_slice(ids);
            Ok(())
        }

        fn publish_embeddings(
            &self,
            entries: Vec<crate::application::ports::vector_search_port::VectorIndexEntry>,
        ) -> Result<()> {
            self.batches.lock().unwrap().1 += 1;
            for entry in entries {
                self.added.lock().unwrap().push(entry.id);
                self.published
                    .lock()
                    .unwrap()
                    .push((entry.document_id, entry.content));
            }
            Ok(())
        }

        fn clear(&self) -> Result<()> {
            Ok(())
        }

        fn count(&self) -> usize {
            self.added.lock().unwrap().len()
        }

        fn dimension(&self) -> usize {
            384
        }
    }

    struct MockFileStorage;

    #[async_trait]
    impl FileStoragePort for MockFileStorage {
        async fn read_file(&self, _path: &Path) -> Result<String> {
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

        async fn find_all_paginated(&self, _limit: usize) -> Result<Vec<Document>> {
            Ok(vec![])
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
    impl crate::application::ports::UnitOfWork for MockUnitOfWork {
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
        let _original_chunks = aggregate.chunks().len();

        repo.set_document(aggregate);

        let use_case = ReindexDocumentUseCase::new(
            Arc::new(MockFileStorage),
            Arc::new(MockEmbedder),
            repo.clone(),
            Arc::new(MockUnitOfWorkFactory),
            Arc::new(MockVectorSearch::default()),
        );

        let response = use_case.execute(doc_id.clone()).await.unwrap();

        assert_eq!(response.document_id, doc_id);
        assert_eq!(response.status, "reindexed");
        assert!(response.error.is_none());
        // Chunks may differ due to different content
        assert!(response.chunks_created > 0);
    }

    /// Reindex must keep the live vector index in step with the database.
    /// Updating SQLite alone left the index holding vectors for chunks that
    /// no longer existed, and never containing the new ones — so semantic
    /// search returned dangling hits and missed the edited content, and a
    /// restart did not repair it because the persisted index is only rebuilt
    /// when empty.
    #[tokio::test]
    async fn reindex_evicts_stale_vectors_and_adds_new_ones() {
        let repo = Arc::new(MockDocumentRepo::new());
        let (aggregate, _temp_dir) = create_test_aggregate();
        let doc_id = aggregate.id().to_string();

        let stale_keys: Vec<String> = aggregate
            .chunks()
            .iter()
            .map(|c| crate::features::embedding::encoding::vector_key(&c.id().to_string()))
            .collect();
        assert!(!stale_keys.is_empty(), "fixture must start with chunks");

        repo.set_document(aggregate);

        let vector_search = Arc::new(MockVectorSearch::default());
        let use_case = ReindexDocumentUseCase::new(
            Arc::new(MockFileStorage),
            Arc::new(MockEmbedder),
            repo.clone(),
            Arc::new(MockUnitOfWorkFactory),
            vector_search.clone(),
        );

        let response = use_case.execute(doc_id).await.unwrap();

        let removed = vector_search.removed.lock().unwrap().clone();
        for key in &stale_keys {
            assert!(
                removed.contains(key),
                "stale vector {} was not evicted from the index",
                key
            );
        }

        assert_eq!(*vector_search.batches.lock().unwrap(), (1, 1));
        assert!(vector_search
            .published
            .lock()
            .unwrap()
            .iter()
            .all(|(id, text)| !id.is_empty() && !text.is_empty()));
        let added = vector_search.added.lock().unwrap().clone();
        assert_eq!(
            added.len(),
            response.chunks_created,
            "every new chunk must be added to the vector index"
        );
        assert!(
            added.iter().all(|k| k.starts_with("emb_")),
            "added keys must use the canonical scheme: {:?}",
            added
        );
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
            Arc::new(MockVectorSearch::default()),
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

        let vector_search = Arc::new(MockVectorSearch::default());
        let use_case = ReindexDocumentUseCase::new(
            Arc::new(MockFileStorage),
            Arc::new(MockEmbedder),
            repo,
            Arc::new(MockUnitOfWorkFactory),
            vector_search.clone(),
        );

        // Reindex with different chunk size
        let new_strategy = ChunkingStrategy::FixedSize { size: 5 };
        let response = use_case
            .execute_with_strategy(doc_id, new_strategy)
            .await
            .unwrap();

        assert!(response.chunks_created > 0);
        assert_eq!(response.status, "reindexed");
        assert_eq!(*vector_search.batches.lock().unwrap(), (1, 1));
        assert_eq!(
            vector_search.published.lock().unwrap().len(),
            response.chunks_created
        );
    }
}
