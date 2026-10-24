//! The one hybrid search: dense and BM25 branches fused by weighted
//! reciprocal rank, with the vector floor and exact repeats dropped.
//!
//! The library's instance is what chat, the agents and the search page call.
//! A passage index on disk (a folder's, a course's) gets an instance of the
//! same orchestrator over its own two stores through
//! [`HybridSearchFactory`], so every index is ranked by one implementation
//! rather than a fusion of its own.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;

use crate::application::contracts::search::{
    QueryBranches, RankedBranch, RerankOptions, Reranked, SearchRequestDto, SearchResponseDto,
    SearchResultDto,
};
use crate::application::ports::{EmbeddingPort, TextSearchPort, VectorSearchPort};
use crate::shared::error::Result;

/// One scoped query, its separate branches and their fusion, and the
/// cross-encoder stage.
///
/// Implemented by the search feature's `HybridSearchUseCase`, so every caller
/// ranks the library the way the search command does, and a passage index
/// is ranked the way the library is.
#[async_trait]
pub trait HybridSearchTrait: Send + Sync {
    /// One query under an optional hard scope; see
    /// `HybridSearchUseCase::execute_scoped`.
    async fn execute_scoped(
        &self,
        request: SearchRequestDto,
        space_id: Option<&str>,
        allowed_document_ids: Option<&HashSet<String>>,
    ) -> Result<SearchResponseDto>;

    /// Every branch for `query`, unfused.
    async fn search_branches(
        &self,
        query: &str,
        space_id: Option<&str>,
        allowed_document_ids: Option<&HashSet<String>>,
        limit: usize,
        vector_weight: f32,
        bm25_weight: f32,
    ) -> QueryBranches;

    fn fuse(&self, branches: Vec<RankedBranch>, limit: usize) -> Vec<SearchResultDto>;

    async fn rerank(&self, response: SearchResponseDto, options: &RerankOptions) -> Reranked;
}

/// Builds the hybrid search over an index's own storage: its embedder, its
/// vectors and its full-text store.
///
/// The search feature implements it with the library's orchestrator and none
/// of the library's own stages (reranker, chunk repository),
/// which read `lattice.db`. A feature that keeps a passage index takes this
/// port from the composition root, so it searches through the shared fusion
/// without depending on the search feature.
pub trait HybridSearchFactory: Send + Sync {
    fn hybrid(
        &self,
        embedder: Arc<dyn EmbeddingPort>,
        vectors: Arc<dyn VectorSearchPort>,
        text: Arc<dyn TextSearchPort>,
    ) -> Arc<dyn HybridSearchTrait>;
}
