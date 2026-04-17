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
//! use crate::features::search::dto::SearchResultPortDto;
//!
//! fn find_similar(
//!     searcher: &impl VectorSearchPort,
//!     query_embedding: &[f32],
//! ) -> Result<Vec<SearchResultPortDto>> {
//!     searcher.search(query_embedding, 10, 0.7)
//! }
//! ```

use crate::features::search::dto::SearchResultPortDto;
use crate::shared::result::Result;
use std::collections::HashSet;

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

    /// Search with an optional document scope.
    ///
    /// When `allowed_document_ids` is provided, implementations should prefer
    /// applying the scope during retrieval rather than post-filtering.
    ///
    /// The default implementation preserves backward compatibility by delegating
    /// to `search` and filtering returned results.
    fn search_scoped(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        threshold: f32,
        allowed_document_ids: Option<&HashSet<String>>,
    ) -> Result<Vec<SearchResultPortDto>> {
        let mut results = self.search(query_embedding, top_k, threshold)?;
        if let Some(scope) = allowed_document_ids {
            results.retain(|result| scope.contains(&result.doc_id));
            if results.len() > top_k {
                results.truncate(top_k);
            }
        }
        Ok(results)
    }

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
