//! Service trait definitions
//!
//! This module defines trait interfaces for dependency injection.

use crate::features::search::enrichment_service::DocumentMetadata;
use crate::features::search::use_cases::HybridSearchUseCase;
use async_trait::async_trait;
use lattice_core::application::contracts::search::{
    QueryBranches, RankedBranch, RerankOptions, Reranked, SearchRequestDto, SearchResponseDto,
    SearchResultDto,
};
// Temporary: chat's test runtime still names the trait at this path.
#[cfg(test)]
pub(crate) use lattice_core::application::ports::library_search::LibrarySearchTrait;
use lattice_core::shared::error::Result;

#[async_trait]
impl lattice_core::application::ports::library_search::LibrarySearchTrait for HybridSearchUseCase {
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

/// Trait for enriching search results with document metadata
#[async_trait]
pub trait SearchEnrichmentServiceTrait: Send + Sync {
    /// Enrich search results with document metadata
    ///
    /// Given chunk IDs from search results, fetches associated metadata including
    /// document titles, file paths, creation dates, and content snippets.
    ///
    /// # Performance
    /// - Processes chunks in batches of 900 (SQLite parameter limit is 999)
    /// - Uses parallel batch processing with concurrency=4
    /// - Expected speedup: 3-4x over sequential processing
    ///
    /// # Arguments
    /// * `chunk_ids` - Slice of chunk IDs to enrich (can be any size)
    ///
    /// # Returns
    /// HashMap mapping chunk ID to DocumentMetadata containing:
    /// - snippet: Content preview (max 200 chars)
    /// - metadata: Hash map with filename, file_type, file_size, created_at, updated_at
    ///
    /// # Errors
    /// - `AppError::Database` if database query fails
    ///
    /// # Example
    /// ```rust
    /// let chunk_ids: Vec<String> = search_results.iter().map(|r| r.id.clone()).collect();
    /// let enriched = service.enrich_results(&chunk_ids).await?;
    ///
    /// for result in search_results {
    ///     if let Some(metadata) = enriched.get(&result.id) {
    ///         println!("Snippet: {}", metadata.snippet);
    ///     }
    /// }
    /// ```
    async fn enrich_results(
        &self,
        chunk_ids: &[String],
    ) -> Result<std::collections::HashMap<String, DocumentMetadata>>;
}
