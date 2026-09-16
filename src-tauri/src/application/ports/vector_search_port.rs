//! Vector similarity search port for the application layer.
//!
//! This port defines the interface for vector similarity search engines.
//! The production implementation uses USearch for persistent HNSW indexing.
//!
//! # Purpose
//!
//! - Abstracts vector search implementation details
//! - Allows switching between in-memory and persistent indexes
//! - Enables testing with mock search results
//! - Supports efficient nearest-neighbor search
//!
//! # Infrastructure Implementations
//!
//! - `USearchVectorIndex` - Persistent USearch HNSW index (production)
//!
//! # Example Usage
//!
//! ```rust
//! use crate::application::ports::VectorSearchPort;
//! use crate::application::contracts::search::SearchResultRecord;
//!
//! fn find_similar(
//!     searcher: &impl VectorSearchPort,
//!     query_embedding: &[f32],
//! ) -> Result<Vec<SearchResultRecord>> {
//!     searcher.search(query_embedding, 10, 0.7)
//! }
//! ```

use crate::application::contracts::search::SearchResultRecord as SearchResultPortDto;
use crate::shared::result::Result;
use std::collections::HashSet;

/// A persisted vector and the source metadata needed by runtime search.
pub struct VectorIndexEntry {
    pub id: String,
    pub embedding: Vec<f32>,
    pub content: String,
    pub chunk_id: String,
    pub document_id: String,
}

/// Port for vector similarity search operations.
///
/// Implementations must:
/// - Perform efficient k-nearest-neighbor search
/// - Support threshold-based filtering
/// - Handle concurrent searches safely
/// - Be thread-safe (`Send + Sync`)
pub trait VectorSearchPort: Send + Sync {
    /// Search for the most similar vectors to a query embedding.
    ///
    /// Uses cosine similarity or dot product to find nearest neighbors.
    /// Results are returned in descending order of similarity.
    ///
    /// # Arguments
    ///
    /// * `query_embedding` - The embedding vector to search for (length must match index dimension)
    /// * `top_k` - Maximum number of results to return
    /// * `threshold` - Minimum similarity score (0.0 to 1.0). Results below threshold are excluded.
    ///
    /// # Returns
    ///
    /// A vector of `SearchResultPortDto` objects ordered by similarity (highest first).
    /// May return fewer than `top_k` results if threshold filters many out.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if query_embedding has wrong dimension
    /// - `AppError::InvalidInput` if top_k is 0 or threshold is invalid
    /// - `AppError::SearchFailed` if index is corrupted or search fails
    ///
    /// # Example
    ///
    /// ```rust
    /// let results = searcher.search(&query_embedding, 10, 0.7)?;
    /// for result in results {
    ///     println!("Score: {}, ID: {}", result.score, result.doc_id);
    /// }
    /// ```
    fn search(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        threshold: f32,
    ) -> Result<Vec<SearchResultPortDto>>;

    /// Search restricted to an explicit set of documents.
    ///
    /// `allowed_document_ids` is a hard scope: `None` searches everything, and
    /// `Some(set)` returns only chunks whose `doc_id` is in `set` (an empty set
    /// allows nothing). At most `top_k` results come back, ordered by descending
    /// similarity, exactly as for [`VectorSearchPort::search`].
    ///
    /// There is deliberately no default body. Post-filtering an unscoped
    /// `search` silently loses recall — an ANN index has to widen its candidate
    /// window *before* retrieval to still return `top_k` in-scope hits — so
    /// implementations must apply the scope during retrieval where they can,
    /// and must opt into filtering after the fact where they cannot.
    ///
    /// # Errors
    ///
    /// Same conditions as [`VectorSearchPort::search`].
    fn search_scoped(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        threshold: f32,
        allowed_document_ids: Option<&HashSet<String>>,
    ) -> Result<Vec<SearchResultPortDto>>;

    /// Add a new embedding to the search index.
    ///
    /// This operation may trigger index rebuilding or updates depending
    /// on the implementation strategy.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier for this embedding (typically document or chunk ID)
    /// * `embedding` - The embedding vector (length must match index dimension)
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if embedding has wrong dimension
    /// - `AppError::DuplicateId` if id already exists in index
    /// - `AppError::SearchFailed` if index update fails
    ///
    /// # Example
    ///
    /// ```rust
    /// let embedding = embedder.embed_single("New document").await?;
    /// searcher.add_embedding("doc-123".into(), embedding)?;
    /// ```
    fn add_embedding(&self, id: String, embedding: Vec<f32>) -> Result<()>;

    /// Add a new embedding with associated content metadata to the search index.
    ///
    /// This enriched variant stores chunk text alongside the embedding so that
    /// search results can include actual content for RAG context injection.
    /// Implementations that don't support content storage should fall back to
    /// `add_embedding` (the default implementation does this).
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier for this embedding (typically `text_embeddings.id`)
    /// * `embedding` - The embedding vector
    /// * `content` - The chunk text content
    /// * `chunk_id` - The chunk identifier (from `text_chunks.id`)
    /// * `document_id` - The parent document identifier
    fn add_embedding_with_content(
        &self,
        id: String,
        embedding: Vec<f32>,
        content: String,
        chunk_id: String,
        document_id: String,
    ) -> Result<()> {
        // Default: ignore content, just store the embedding
        let _ = (content, chunk_id, document_id);
        self.add_embedding(id, embedding)
    }

    /// Publish a document's persisted vectors in one batch. Implementations
    /// should persist once and accept repeated publication of the same IDs so
    /// an interrupted import can finish without generating embeddings again.
    fn publish_embeddings(&self, entries: Vec<VectorIndexEntry>) -> Result<()> {
        for entry in entries {
            self.add_embedding_with_content(
                entry.id,
                entry.embedding,
                entry.content,
                entry.chunk_id,
                entry.document_id,
            )?;
        }
        Ok(())
    }

    /// Remove an embedding from the search index.
    ///
    /// If the ID does not exist, this is a no-op (returns Ok).
    ///
    /// # Arguments
    ///
    /// * `id` - The unique identifier of the embedding to remove
    ///
    /// # Errors
    ///
    /// - `AppError::SearchFailed` if index update fails
    ///
    /// # Example
    ///
    /// ```rust
    /// searcher.remove_embedding("doc-123")?;
    /// ```
    fn remove_embedding(&self, id: &str) -> Result<()>;

    /// Remove a set of embeddings, persisting once when supported.
    fn remove_embeddings(&self, ids: &[String]) -> Result<()> {
        for id in ids {
            self.remove_embedding(id)?;
        }
        Ok(())
    }

    /// Clear all embeddings from the index.
    ///
    /// This resets the index to an empty state.
    ///
    /// # Errors
    ///
    /// - `AppError::SearchFailed` if index clear operation fails
    ///
    /// # Example
    ///
    /// ```rust
    /// searcher.clear()?;
    /// assert_eq!(searcher.count(), 0);
    /// ```
    fn clear(&self) -> Result<()>;

    /// Get the number of embeddings in the index.
    ///
    /// # Returns
    ///
    /// The total number of indexed embeddings.
    ///
    /// # Example
    ///
    /// ```rust
    /// let count = searcher.count();
    /// println!("Index contains {} embeddings", count);
    /// ```
    fn count(&self) -> usize;

    /// Get the expected dimension of embeddings for this index.
    ///
    /// All embeddings added to this index must have this dimension.
    ///
    /// # Returns
    ///
    /// The embedding dimension (e.g., DEFAULT_EMBEDDING_DIM)
    ///
    /// # Example
    ///
    /// ```rust
    /// let dim = searcher.dimension();
    /// assert_eq!(embedding.len(), dim);
    /// ```
    fn dimension(&self) -> usize;
}
