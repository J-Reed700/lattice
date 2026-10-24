//! Dense vector and BM25 keyword search engine traits.

use async_trait::async_trait;

use crate::application::contracts::search::engine::{BM25Result, SearchResult};
use crate::shared::error::Result;

/// Service for dense vector similarity search.
#[async_trait]
pub trait SearchServiceTrait: Send + Sync {
    /// Perform vector similarity search ordered by similarity descending.
    fn search(&self, query_embedding: &[f32], top_k: usize) -> Result<Vec<SearchResult>>;

    /// Search with metadata enrichment from database.
    async fn search_with_metadata(
        &self,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<SearchResult>>;

    /// Search with similarity threshold filtering.
    fn search_with_threshold(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        threshold: f32,
    ) -> Result<Vec<SearchResult>>;

    /// Batch search for multiple query embeddings.
    fn batch_search(
        &self,
        query_embeddings: &[Vec<f32>],
        top_k: usize,
    ) -> Result<Vec<Vec<SearchResult>>>;
}

/// Service for BM25 keyword-based search.
#[async_trait]
pub trait BM25SearchTrait: Send + Sync {
    /// Perform BM25 keyword search ordered by descending score.
    async fn search(&self, query: &str, top_k: usize) -> Result<Vec<BM25Result>>;

    /// Perform BM25 search with minimum score filter.
    async fn search_with_filter(
        &self,
        query: &str,
        top_k: usize,
        min_score: f32,
    ) -> Result<Vec<BM25Result>>;

    /// Optimize the FTS5 index for better performance.
    async fn optimize_index(&self) -> Result<()>;

    /// Rebuild the entire FTS5 index from scratch.
    async fn rebuild_index(&self) -> Result<()>;
}
