//! Search Ranking Service
//!
//! Domain service for ranking and merging search results using
//! Reciprocal Rank Fusion (RRF) algorithm.
//!
//! This service encapsulates the business logic for combining results
//! from multiple search strategies (vector, text, etc.) into a unified,
//! re-ranked result set.

use crate::domain::entities::search_result::SearchResult;
use std::collections::HashMap;

/// Service for ranking and merging search results.
///
/// Implements the Reciprocal Rank Fusion (RRF) algorithm to combine
/// results from different search strategies.
///
/// # Example
///
/// ```rust,no_run
/// use lattice::domain::services::SearchRankingService;
///
/// let ranking_service = SearchRankingService::new();
///
/// let merged = ranking_service.merge_with_rrf(
///     vector_results,
///     text_results,
///     10
/// );
/// ```
pub struct SearchRankingService {
    /// K parameter for RRF formula: 1/(K + rank)
    /// Higher values reduce the impact of high-ranking results
    rrf_k: f32,
}

impl SearchRankingService {
    /// Create a new search ranking service with default RRF K parameter.
    ///
    /// The default K value of 60.0 is commonly used in literature.
    pub fn new() -> Self {
        Self { rrf_k: 60.0 }
    }

    /// Create a new search ranking service with custom RRF K parameter.
    ///
    /// # Arguments
    ///
    /// * `rrf_k` - The K parameter for RRF formula
    ///
    /// # Example
    ///
    /// ```rust
    /// use lattice::domain::services::SearchRankingService;
    ///
    /// let service = SearchRankingService::with_rrf_k(30.0);
    /// ```
    pub fn with_rrf_k(rrf_k: f32) -> Self {
        Self { rrf_k }
    }

    /// Merge and re-rank results using Reciprocal Rank Fusion.
    ///
    /// Combines results from multiple search strategies by computing
    /// RRF scores based on rank position in each result set.
    ///
    /// # Arguments
    ///
    /// * `vector` - Results from vector/semantic search
    /// * `text` - Results from BM25 text search
    /// * `limit` - Maximum number of results to return
    ///
    /// # Returns
    ///
    /// Merged and re-ranked results, limited to the specified count
    ///
    /// # Algorithm
    ///
    /// For each result in both sets, computes RRF score:
    /// ```text
    /// score = sum(1/(K + rank)) for each result set where item appears
    /// ```
    ///
    /// Results are then sorted by descending RRF score.
    pub fn merge_with_rrf(
        &self,
        vector: Vec<SearchResult>,
        text: Vec<SearchResult>,
        limit: usize,
    ) -> Vec<SearchResult> {
        // Build score map using RRF
        let mut scores: HashMap<String, (f32, SearchResult)> = HashMap::new();

        // Add scores from vector search
        for (rank, result) in vector.into_iter().enumerate() {
            let rrf_score = 1.0 / (self.rrf_k + (rank + 1) as f32);
            let id = result.id().to_string();
            scores.insert(id, (rrf_score, result));
        }

        // Add scores from text search
        for (rank, result) in text.into_iter().enumerate() {
            let rrf_score = 1.0 / (self.rrf_k + (rank + 1) as f32);
            let id = result.id().to_string();

            scores
                .entry(id)
                .and_modify(|(score, _)| *score += rrf_score)
                .or_insert((rrf_score, result));
        }

        // Sort by combined RRF score and take top-k
        let mut merged: Vec<(f32, SearchResult)> = scores.into_values().collect();

        merged.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        merged
            .into_iter()
            .take(limit)
            .map(|(rrf_score, result)| {
                // Update result score to be the RRF score
                // RRF score is 1/(k+rank), mathematically guaranteed finite for k>=0, rank>=1
                SearchResult::with_metadata(
                    result.id().to_string(),
                    rrf_score,
                    result.snippet().map(|s| s.to_string()),
                    result.document_id().map(|s| s.to_string()),
                    result.file_path().map(|s| s.to_string()),
                    result.position(),
                )
                .unwrap_or(result) // Fallback to original result on validation error
            })
            .collect()
    }
}

impl Default for SearchRankingService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_result(id: &str, score: f32) -> SearchResult {
        SearchResult::with_metadata(
            id.to_string(),
            score,
            Some(format!("Snippet for {}", id)),
            Some(format!("doc_{}", id)),
            Some(format!("/path/{}.txt", id)),
            Some(0),
        )
        .expect("Test scores are always valid")
    }

    #[test]
    fn test_merge_with_rrf_combines_results() {
        let service = SearchRankingService::new();

        let vector = vec![create_test_result("a", 0.9), create_test_result("b", 0.8)];

        let text = vec![create_test_result("b", 0.95), create_test_result("c", 0.85)];

        let merged = service.merge_with_rrf(vector, text, 10);

        // Should have 3 unique results (a, b, c)
        assert_eq!(merged.len(), 3);

        // Result 'b' appears in both sets, so should have higher RRF score
        let result_b = merged.iter().find(|r| r.id() == "b").unwrap();
        let result_a = merged.iter().find(|r| r.id() == "a").unwrap();

        assert!(result_b.score() > result_a.score());
    }

    #[test]
    fn test_merge_with_rrf_respects_limit() {
        let service = SearchRankingService::new();

        let vector = vec![
            create_test_result("a", 0.9),
            create_test_result("b", 0.8),
            create_test_result("c", 0.7),
        ];

        let text = vec![create_test_result("d", 0.9), create_test_result("e", 0.8)];

        let merged = service.merge_with_rrf(vector, text, 3);

        assert_eq!(merged.len(), 3);
    }

    #[test]
    fn test_custom_rrf_k() {
        let service = SearchRankingService::with_rrf_k(30.0);

        let vector = vec![create_test_result("a", 0.9)];
        let text = vec![create_test_result("b", 0.8)];

        let merged = service.merge_with_rrf(vector, text, 10);

        assert_eq!(merged.len(), 2);
    }
}
