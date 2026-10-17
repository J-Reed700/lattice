//! The library search a caller outside the search feature drives.

use std::collections::HashSet;

use async_trait::async_trait;

use crate::application::contracts::search::{
    QueryBranches, RankedBranch, RerankOptions, Reranked, SearchRequestDto, SearchResponseDto,
    SearchResultDto,
};
use crate::shared::error::Result;

/// One scoped query, its separate branches and their fusion, and the
/// cross-encoder stage.
///
/// Implemented by the search feature's `HybridSearchUseCase`, so every caller
/// ranks the library the way the search command does.
#[async_trait]
pub trait LibrarySearchTrait: Send + Sync {
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
