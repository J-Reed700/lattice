//! # Recency Search Use Case
//!
//! Time-aware search that boosts recent documents in results.
//!
//! This use case combines semantic/hybrid search with recency weighting,
//! making it ideal for scenarios where fresh content is preferred.
//!
//! ## Algorithm
//!
//! 1. Perform hybrid search (vector + BM25)
//! 2. Apply time-decay function to boost recent documents
//! 3. Re-rank results by combined (relevance + recency) score
//! 4. Return top-k results
//!
//! ## Use Cases
//!
//! - News and current events search
//! - Recent updates and changes
//! - Time-sensitive information retrieval
//!
//! ## Example
//!
//! ```rust,no_run
//! use vault_desktop::application::use_cases::search::recency_search::RecencySearchUseCase;
//! use vault_desktop::application::dtos::search_dto::RecencySearchOptions;
//!
//! # async fn example(use_case: RecencySearchUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let options = RecencySearchOptions {
//!     query: "recent developments".to_string(),
//!     limit: Some(10),
//!     recency_weight: Some(0.3), // 30% weight for recency
//!     max_age_days: Some(90),    // Only consider last 90 days
//! };
//!
//! let results = use_case.execute(options).await?;
//! println!("Found {} time-aware results", results.len());
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use crate::features::search::dto::{RecencySearchOptions, SearchResultDto};
use crate::application::ports::EmbeddingPort;
use crate::infrastructure::services::traits::HybridSearchTrait;
use crate::shared::error::{AppError, Result};

/// Recency search use case with time-aware ranking.
///
/// Boosts recent documents in search results using a time-decay function.
///
/// ## Dependencies
///
/// - `EmbeddingPort`: Generates query embeddings
/// - `HybridSearchTrait`: Provides recency-aware hybrid search
pub struct RecencySearchUseCase {
    embedding_service: Arc<dyn EmbeddingPort>,
    hybrid_search: Arc<dyn HybridSearchTrait>,
}

impl RecencySearchUseCase {
    /// Create a new recency search use case.
    ///
    /// # Arguments
    ///
    /// * `embedding_service` - Service for generating text embeddings
    /// * `hybrid_search` - Service for hybrid search with recency support
    pub fn new(
        embedding_service: Arc<dyn EmbeddingPort>,
        hybrid_search: Arc<dyn HybridSearchTrait>,
    ) -> Self {
        Self {
            embedding_service,
            hybrid_search,
        }
    }

    /// Execute recency-aware search.
    ///
    /// # Arguments
    ///
    /// * `options` - Search options including recency parameters
    ///
    /// # Returns
    ///
    /// Vector of search results with recency-boosted scores
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Query is empty or invalid
    /// - Recency weight is not in range [0.0, 1.0]
    /// - Max age days is not positive
    /// - Embedding generation fails
    /// - Search operation fails
    ///
    /// # Validation Rules
    ///
    /// - Query must be non-empty after trimming
    /// - Recency weight must be between 0.0 and 1.0 (inclusive)
    /// - Max age days must be positive
    /// - Limit must be positive (default: 10)
    pub async fn execute(&self, options: RecencySearchOptions) -> Result<Vec<SearchResultDto>> {
        // Validate query
        let query = options.query.trim();
        if query.is_empty() {
            return Err(AppError::InvalidInput(
                "Search query cannot be empty".to_string(),
            ));
        }

        // Validate and set defaults
        let limit = options.limit.unwrap_or(10);
        if limit == 0 {
            return Err(AppError::InvalidInput("Limit must be positive".to_string()));
        }

        let recency_weight = options.recency_weight.unwrap_or(0.1);
        if !(0.0..=1.0).contains(&recency_weight) {
            return Err(AppError::InvalidInput(
                "Recency weight must be between 0.0 and 1.0".to_string(),
            ));
        }

        let max_age_days = options.max_age_days.unwrap_or(730); // Default: 2 years
        if max_age_days <= 0 {
            return Err(AppError::InvalidInput(
                "Max age days must be positive".to_string(),
            ));
        }

        // Generate embedding
        let query_embedding = self.embedding_service.embed_single(query).await?;

        // Perform recency-aware search
        let results = self
            .hybrid_search
            .search_with_recency(query, &query_embedding, limit, recency_weight, max_age_days)
            .await?;

        // Map infrastructure results to DTOs
        let dtos = results
            .into_iter()
            .map(|r| {
                let metadata = r
                    .metadata
                    .as_ref()
                    .and_then(|value| value.as_object())
                    .map(|object| {
                        object
                            .iter()
                            .map(|(key, value)| (key.clone(), value.clone()))
                            .collect::<std::collections::HashMap<String, serde_json::Value>>()
                    })
                    .unwrap_or_default();

                let title = metadata
                    .get("title")
                    .and_then(|value| value.as_str())
                    .or_else(|| metadata.get("filename").and_then(|value| value.as_str()))
                    .unwrap_or("")
                    .to_string();

                let path = metadata
                    .get("path")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string();

                SearchResultDto {
                    id: r.chunk_id().to_string(),
                    title,
                    content: r.content.clone(),
                    score: r.score,
                    path: Some(path),
                    document_id: Some(r.document_id().to_string()),
                    position: None,
                    vector_score: r.vector_score,
                    bm25_score: r.bm25_score.or(r.keyword_score),
                    vector_rank: r.vector_rank,
                    bm25_rank: r.bm25_rank,
                    metadata,
                }
            })
            .collect();

        Ok(dtos)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::search::SearchMode;
    use crate::search::hybrid::HybridSearchResult;
    use async_trait::async_trait;

    struct MockEmbedder;

    #[async_trait]
    impl EmbeddingPort for MockEmbedder {
        async fn embed_single(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![0.1, 0.2, 0.3])
        }

        async fn embed_batch(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>> {
            Ok(vec![vec![0.1, 0.2, 0.3]])
        }

        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }

        fn dimension(&self) -> usize {
            3
        }
    }

    struct MockHybridSearch;

    #[async_trait]
    impl HybridSearchTrait for MockHybridSearch {
        async fn search(
            &self,
            _query_text: &str,
            _query_embedding: &[f32],
            top_k: usize,
            _mode: SearchMode,
        ) -> Result<Vec<HybridSearchResult>> {
            Ok((0..top_k.min(5))
                .map(|i| HybridSearchResult {
                    chunk_id: format!("chunk-{}", i),
                    document_id: format!("doc-{}", i),
                    id: format!("result-{}", i),
                    score: 0.9 - (i as f32 * 0.1),
                    vector_score: Some(0.85),
                    bm25_score: Some(0.75),
                    keyword_score: Some(0.75),
                    vector_rank: Some(i),
                    bm25_rank: Some(i + 1),
                    content: "test content".to_string(),
                    metadata: None,
                })
                .collect())
        }

        async fn batch_search(
            &self,
            queries: Vec<(String, Vec<f32>)>,
            top_k: usize,
            mode: SearchMode,
        ) -> Result<Vec<Vec<HybridSearchResult>>> {
            let mut results = Vec::new();
            for (query_text, query_embedding) in queries {
                results.push(
                    self.search(&query_text, &query_embedding, top_k, mode)
                        .await?,
                );
            }
            Ok(results)
        }

        async fn search_with_recency(
            &self,
            _query_text: &str,
            _query_embedding: &[f32],
            top_k: usize,
            recency_weight: f32,
            _max_age_days: i64,
        ) -> Result<Vec<HybridSearchResult>> {
            // Simulate recency boost
            Ok((0..top_k.min(5))
                .map(|i| {
                    let base_score = 0.9 - (i as f32 * 0.1);
                    let recency_boost = recency_weight * 0.1; // Simulate boost
                    HybridSearchResult {
                        chunk_id: format!("chunk-{}", i),
                        document_id: format!("doc-{}", i),
                        id: format!("recent-{}", i),
                        score: base_score + recency_boost,
                        vector_score: Some(0.85),
                        bm25_score: Some(0.75),
                        keyword_score: Some(0.75),
                        vector_rank: Some(i),
                        bm25_rank: Some(i + 1),
                        content: "test content".to_string(),
                        metadata: None,
                    }
                })
                .collect())
        }
    }

    #[tokio::test]
    async fn test_recency_search_success() {
        let use_case =
            RecencySearchUseCase::new(Arc::new(MockEmbedder), Arc::new(MockHybridSearch));

        let options = RecencySearchOptions {
            query: "test query".to_string(),
            limit: Some(10),
            recency_weight: Some(0.3),
            max_age_days: Some(90),
        };

        let results = use_case.execute(options).await.unwrap();

        assert!(!results.is_empty());
        assert!(results.len() <= 10);
    }

    #[tokio::test]
    async fn test_recency_search_empty_query() {
        let use_case =
            RecencySearchUseCase::new(Arc::new(MockEmbedder), Arc::new(MockHybridSearch));

        let options = RecencySearchOptions {
            query: "   ".to_string(),
            limit: Some(10),
            recency_weight: Some(0.3),
            max_age_days: Some(90),
        };

        let result = use_case.execute(options).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_recency_search_invalid_weight() {
        let use_case =
            RecencySearchUseCase::new(Arc::new(MockEmbedder), Arc::new(MockHybridSearch));

        let options = RecencySearchOptions {
            query: "test".to_string(),
            limit: Some(10),
            recency_weight: Some(1.5), // Invalid: > 1.0
            max_age_days: Some(90),
        };

        let result = use_case.execute(options).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_recency_search_invalid_max_age() {
        let use_case =
            RecencySearchUseCase::new(Arc::new(MockEmbedder), Arc::new(MockHybridSearch));

        let options = RecencySearchOptions {
            query: "test".to_string(),
            limit: Some(10),
            recency_weight: Some(0.3),
            max_age_days: Some(-1), // Invalid: negative
        };

        let result = use_case.execute(options).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_recency_search_defaults() {
        let use_case =
            RecencySearchUseCase::new(Arc::new(MockEmbedder), Arc::new(MockHybridSearch));

        let options = RecencySearchOptions {
            query: "test".to_string(),
            limit: None,          // Should default to 10
            recency_weight: None, // Should default to 0.1
            max_age_days: None,   // Should default to 730
        };

        let results = use_case.execute(options).await.unwrap();
        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_recency_search_zero_limit() {
        let use_case =
            RecencySearchUseCase::new(Arc::new(MockEmbedder), Arc::new(MockHybridSearch));

        let options = RecencySearchOptions {
            query: "test".to_string(),
            limit: Some(0), // Invalid
            recency_weight: Some(0.3),
            max_age_days: Some(90),
        };

        let result = use_case.execute(options).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::InvalidInput(_)));
    }
}
