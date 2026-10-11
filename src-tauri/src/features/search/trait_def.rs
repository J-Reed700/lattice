//! Service trait definitions
//!
//! This module defines trait interfaces for dependency injection.

use crate::features::search::dto::{SearchRequestDto, SearchResponseDto, SearchResultDto};
use crate::features::search::engine::service::SearchResult;
use crate::features::search::use_cases::{
    HybridSearchUseCase, QueryBranches, RankedBranch, RerankOptions, Reranked,
};
use crate::shared::error::Result;
use async_trait::async_trait;

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
    async fn search(
        &self,
        query: &str,
        top_k: usize,
    ) -> Result<Vec<crate::features::search::engine::bm25::BM25Result>>;

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
    ) -> Result<Vec<crate::features::search::engine::bm25::BM25Result>>;

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

/// Learned sparse retrieval — the third fusion branch beside dense vectors and
/// BM25.
///
/// Implementations embed the query with the loaded model's sparse head and
/// score stored per-chunk term postings by dot product. When the loaded model
/// has no sparse head, [`SparseSearchTrait::is_available`] is `false` and the
/// branch is never started; calling `search_scoped` anyway returns the port's
/// "not supported" error, which callers must treat as a missing capability
/// rather than a failed search.
#[async_trait]
pub trait SparseSearchTrait: Send + Sync {
    /// True when the loaded embedding model can produce sparse term weights.
    ///
    /// This is the flag every caller checks before spending a branch on it, so
    /// switching from BGE-M3 to a dense-only model silently drops back to
    /// two-way fusion instead of erroring on every query.
    fn is_available(&self) -> bool;

    /// Sparse search under the same hard scope the BM25 branch uses.
    ///
    /// `space_id` restricts to a space's document memberships and
    /// `allowed_document_ids` to an explicit set; an empty set allows nothing.
    /// Both are applied before the limit.
    async fn search_scoped(
        &self,
        query: &str,
        top_k: usize,
        space_id: Option<&str>,
        allowed_document_ids: Option<&std::collections::HashSet<String>>,
    ) -> Result<Vec<crate::features::search::dto::SearchResultPortDto>>;
}

/// The library search a caller outside this feature drives: one scoped query,
/// its separate branches and their fusion, and the cross-encoder stage.
///
/// Implemented by [`HybridSearchUseCase`], so every caller ranks the library
/// the way the search command does.
#[async_trait]
pub trait LibrarySearchTrait: Send + Sync {
    /// One query under an optional hard scope; see
    /// [`HybridSearchUseCase::execute_scoped`].
    async fn execute_scoped(
        &self,
        request: SearchRequestDto,
        space_id: Option<&str>,
        allowed_document_ids: Option<&std::collections::HashSet<String>>,
    ) -> Result<SearchResponseDto>;

    /// Every branch for `query`, unfused.
    async fn search_branches(
        &self,
        query: &str,
        space_id: Option<&str>,
        allowed_document_ids: Option<&std::collections::HashSet<String>>,
        limit: usize,
        vector_weight: f32,
        bm25_weight: f32,
    ) -> QueryBranches;

    fn fuse(&self, branches: Vec<RankedBranch>, limit: usize) -> Vec<SearchResultDto>;

    async fn rerank(&self, response: SearchResponseDto, options: &RerankOptions) -> Reranked;
}

#[async_trait]
impl LibrarySearchTrait for HybridSearchUseCase {
    async fn execute_scoped(
        &self,
        request: SearchRequestDto,
        space_id: Option<&str>,
        allowed_document_ids: Option<&std::collections::HashSet<String>>,
    ) -> Result<SearchResponseDto> {
        HybridSearchUseCase::execute_scoped(self, request, space_id, allowed_document_ids).await
    }

    async fn search_branches(
        &self,
        query: &str,
        space_id: Option<&str>,
        allowed_document_ids: Option<&std::collections::HashSet<String>>,
        limit: usize,
        vector_weight: f32,
        bm25_weight: f32,
    ) -> QueryBranches {
        HybridSearchUseCase::search_branches(
            self,
            query,
            space_id,
            allowed_document_ids,
            limit,
            vector_weight,
            bm25_weight,
        )
        .await
    }

    fn fuse(&self, branches: Vec<RankedBranch>, limit: usize) -> Vec<SearchResultDto> {
        HybridSearchUseCase::fuse(self, branches, limit)
    }

    async fn rerank(&self, response: SearchResponseDto, options: &RerankOptions) -> Reranked {
        HybridSearchUseCase::rerank(self, response, options).await
    }
}
