//! The two retrieval engines library search fuses: dense vectors and BM25
//! keywords.

use async_trait::async_trait;

use crate::application::contracts::search::engine::{BM25Result, SearchResult};
use crate::shared::error::Result;

#[async_trait]
pub trait SearchServiceTrait: Send + Sync {
    /// Perform vector similarity search
    ///
    /// # Arguments
    /// * `query_embedding` - The query vector to search for
    /// * `top_k` - Number of top results to return
    ///
    /// # Returns
    /// Vector of search results ordered by similarity (highest first)
    ///
    /// # Example
    /// ```rust
    /// let query_emb = vec![0.1, 0.2, 0.3, ...];
    /// let results = service.search(&query_emb, 10)?;
    /// for result in results {
    ///     println!("{}: {:.4}", result.id, result.score);
    /// }
    /// ```
    fn search(&self, query_embedding: &[f32], top_k: usize) -> Result<Vec<SearchResult>>;

    /// Search with metadata enrichment
    ///
    /// Like `search`, but fetches additional document metadata from database.
    ///
    /// # Arguments
    /// * `query_embedding` - The query vector
    /// * `top_k` - Number of results
    ///
    /// # Returns
    /// Search results with populated metadata fields (filename, mime_type, etc.)
    async fn search_with_metadata(
        &self,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<SearchResult>>;

    /// Search with similarity threshold filtering
    ///
    /// Only returns results above the specified similarity score.
    ///
    /// # Arguments
    /// * `query_embedding` - The query vector
    /// * `top_k` - Maximum number of results
    /// * `threshold` - Minimum similarity score (0.0 to 1.0)
    ///
    /// # Returns
    /// Search results with score >= threshold, ordered by similarity
    ///
    /// # Example
    /// ```rust
    /// // Only return results with >70% similarity
    /// let results = service.search_with_threshold(&query_emb, 100, 0.7)?;
    /// ```
    fn search_with_threshold(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        threshold: f32,
    ) -> Result<Vec<SearchResult>>;

    /// Batch search for multiple queries
    ///
    /// More efficient than calling `search` multiple times.
    ///
    /// # Arguments
    /// * `query_embeddings` - Slice of query vectors
    /// * `top_k` - Number of results per query
    ///
    /// # Returns
    /// Vector of result vectors (one per query)
    fn batch_search(
        &self,
        query_embeddings: &[Vec<f32>],
        top_k: usize,
    ) -> Result<Vec<Vec<SearchResult>>>;
}

/// Trait for BM25 text-based search services
///
/// Provides keyword-based search using BM25 ranking algorithm.
/// BM25 is effective for exact keyword matching and traditional search scenarios.
///
/// # Implementations
/// - `BM25Search`: Production implementation with SQLite FTS5
/// - `MockBM25Search`: In-memory mock for testing
///
/// # Example
/// ```rust
/// let results = bm25_service.search("rust programming", 10).await?;
/// for result in results {
///     println!("Document: {} (score: {})", result.document_id, result.score);
/// }
/// ```
#[async_trait]
pub trait BM25SearchTrait: Send + Sync {
    /// Perform BM25 keyword search
    ///
    /// Searches the FTS5 index for documents matching the query terms.
    /// Results are ranked by BM25 score (higher is better).
    ///
    /// # Arguments
    /// * `query` - Search query string
    /// * `top_k` - Maximum number of results to return
    ///
    /// # Returns
    /// Vector of search results ordered by descending score
    ///
    /// # Example
    /// ```rust
    /// let results = service.search("machine learning", 5).await?;
    /// assert!(results.len() <= 5);
    /// ```
    async fn search(&self, query: &str, top_k: usize) -> Result<Vec<BM25Result>>;

    /// Perform BM25 search with minimum score filter
    ///
    /// Returns only results that meet or exceed the minimum score threshold.
    ///
    /// # Arguments
    /// * `query` - Search query string
    /// * `top_k` - Maximum number of results to return
    /// * `min_score` - Minimum BM25 score (higher = stricter)
    ///
    /// # Returns
    /// Filtered results with score >= min_score, ordered by score
    ///
    /// # Example
    /// ```rust
    /// // Only return high-quality matches
    /// let results = service.search_with_filter("rust", 10, 5.0).await?;
    /// ```
    async fn search_with_filter(
        &self,
        query: &str,
        top_k: usize,
        min_score: f32,
    ) -> Result<Vec<BM25Result>>;

    /// Optimize the FTS5 index for better performance
    ///
    /// Merges index segments and removes deleted entries.
    /// Should be called periodically during maintenance.
    ///
    /// # Side Effects
    /// - Optimizes the documents_fts table
    /// - May temporarily increase memory usage
    ///
    /// # Example
    /// ```rust
    /// service.optimize_index().await?;
    /// ```
    async fn optimize_index(&self) -> Result<()>;

    /// Rebuild the entire FTS5 index from scratch
    ///
    /// Useful after schema changes or data corruption.
    /// More expensive than optimize_index().
    ///
    /// # Side Effects
    /// - Drops and recreates the FTS5 index
    /// - May take significant time for large datasets
    ///
    /// # Example
    /// ```rust
    /// service.rebuild_index().await?;
    /// ```
    async fn rebuild_index(&self) -> Result<()>;
}
