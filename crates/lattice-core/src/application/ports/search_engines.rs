//! The two retrieval engines library search fuses: dense vectors and BM25
//! keywords.

use async_trait::async_trait;

use crate::application::contracts::search::engine::{BM25Result, SearchResult};
use crate::shared::error::Result;

#[async_trait]
pub trait SearchServiceTrait: Send + Sync {
    /// Perform vector similarity search
    fn search(&self, query_embedding: &[f32], top_k: usize) -> Result<Vec<SearchResult>>;

    /// Search with metadata enrichment
    ///
    /// Like `search`, but fetches additional document metadata from database.
    async fn search_with_metadata(
        &self,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<SearchResult>>;

    /// Search with similarity threshold filtering
    ///
    /// Only returns results above the specified similarity score.
    fn search_with_threshold(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        threshold: f32,
    ) -> Result<Vec<SearchResult>>;

    /// Batch search for multiple queries
    ///
    /// More efficient than calling `search` multiple times.
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
#[async_trait]
pub trait BM25SearchTrait: Send + Sync {
    /// Perform BM25 keyword search
    ///
    /// Searches the FTS5 index for documents matching the query terms.
    /// Results are ranked by BM25 score (higher is better).
    async fn search(&self, query: &str, top_k: usize) -> Result<Vec<BM25Result>>;

    /// Perform BM25 search with minimum score filter
    ///
    /// Returns only results that meet or exceed the minimum score threshold.
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
    async fn optimize_index(&self) -> Result<()>;

    /// Rebuild the entire FTS5 index from scratch
    ///
    /// Useful after schema changes or data corruption.
    /// More expensive than optimize_index().
    ///
    /// # Side Effects
    /// - Drops and recreates the FTS5 index
    /// - May take significant time for large datasets
    async fn rebuild_index(&self) -> Result<()>;
}
