//! # Hybrid Search Use Case
//!
//! Combines vector-based semantic search with keyword-based BM25 search.
//!
//! This use case implements Reciprocal Rank Fusion (RRF) to merge results
//! from multiple search algorithms, providing the best of both worlds:
//! - Semantic understanding from vector search
//! - Precise keyword matching from BM25
//!
//! ## Algorithm
//!
//! 1. Perform vector search for semantic matches
//! 2. Perform BM25 search for keyword matches
//! 3. Merge results using Reciprocal Rank Fusion
//! 4. Return top-k merged results
//!
//! ## Example
//!
//! ```rust,no_run
//! use vault_desktop::application::use_cases::search::hybrid_search::HybridSearchUseCase;
//! use vault_desktop::application::dtos::search_dto::{SearchRequestDto, SearchModeDto};
//!
//! # async fn example(use_case: HybridSearchUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let request = SearchRequestDto {
//!     query: "neural network optimization".to_string(),
//!     limit: Some(10),
//!     threshold: Some(0.6),
//!     mode: SearchModeDto::Hybrid {
//!         vector_weight: 0.7,
//!         bm25_weight: 0.3,
//!     },
//! };
//!
//! let response = use_case.execute(request).await?;
//! println!("Found {} hybrid results", response.total);
//! # Ok(())
//! # }
//! ```

use std::collections::HashSet;
use std::sync::Arc;

use crate::features::search::dto::{SearchModeDto, SearchRequestDto, SearchResponseDto};
use crate::features::search::mapper::SearchMapper;
use crate::application::ports::{EmbeddingPort, TextSearchPort, VectorSearchPort};
use crate::domain::entities::search_result::SearchResult;
use crate::domain::services::SearchRankingService;
use crate::shared::error::Result;
use crate::shared::text_utils::safe_truncate;
use once_cell::sync::Lazy;
use rust_stemmers::{Algorithm, Stemmer};
use tracing::{debug, info};

/// Hybrid search use case combining vector and text search.
///
/// Uses Reciprocal Rank Fusion to merge results from:
/// - Vector similarity search (semantic understanding)
/// - BM25 keyword search (precise matching)
///
/// ## Dependencies
///
/// - `EmbeddingPort`: Generates query embeddings
/// - `VectorSearchPort`: Performs semantic search
/// - `TextSearchPort`: Performs BM25 keyword search
pub struct HybridSearchUseCase {
    embedding_service: Arc<dyn EmbeddingPort>,
    vector_search: Arc<dyn VectorSearchPort>,
    text_search: Arc<dyn TextSearchPort>,
    ranking_service: SearchRankingService,
}

impl HybridSearchUseCase {
    /// Create a new hybrid search use case.
    ///
    /// # Arguments
    ///
    /// * `embedding_service` - Service for generating text embeddings
    /// * `vector_search` - Service for vector similarity search
    /// * `text_search` - Service for BM25 keyword search
    pub fn new(
        embedding_service: Arc<dyn EmbeddingPort>,
        vector_search: Arc<dyn VectorSearchPort>,
        text_search: Arc<dyn TextSearchPort>,
    ) -> Self {
        Self {
            embedding_service,
            vector_search,
            text_search,
            ranking_service: SearchRankingService::new(),
        }
    }

    /// Execute hybrid search.
    ///
    /// # Arguments
    ///
    /// * `request` - Search request with query and parameters
    ///
    /// # Returns
    ///
    /// Search response with merged results from both search methods
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Embedding generation fails
    /// - Vector or text search fails
    /// - Query is invalid
    pub async fn execute(&self, request: SearchRequestDto) -> Result<SearchResponseDto> {
        self.execute_scoped(request, None, None).await
    }

    /// Execute hybrid search with optional hard scope.
    pub async fn execute_scoped(
        &self,
        request: SearchRequestDto,
        space_id: Option<&str>,
        allowed_document_ids: Option<&HashSet<String>>,
    ) -> Result<SearchResponseDto> {
        let start = std::time::Instant::now();
        let limit = request.limit.unwrap_or(10);
        let mode = request.mode.clone();
        info!(
            query_len = request.query.len(),
            limit = limit,
            mode = ?mode,
            threshold = request.threshold,
            space_scoped = space_id.is_some() || allowed_document_ids.is_some(),
            "HybridSearchUseCase execute"
        );

        let query_time_ms = start.elapsed().as_millis() as u64;
        match mode {
            SearchModeDto::Vector => {
                let threshold = request.threshold.unwrap_or(0.5);
                let query_embedding = self.embedding_service.embed_single(&request.query).await?;
                let vector_port_dtos = self.vector_search.search_scoped(
                    &query_embedding,
                    limit,
                    threshold,
                    allowed_document_ids,
                )?;
                let vector_results = SearchMapper::port_dtos_to_domain(vector_port_dtos);
                debug!(
                    result_count = vector_results.len(),
                    preview = Self::summarize_domain_results(&vector_results, 8),
                    "Vector-only search results"
                );
                Ok(SearchMapper::to_response_dto(vector_results, query_time_ms))
            }
            SearchModeDto::BM25 => {
                let text_port_dtos = self
                    .text_search
                    .search_scoped(&request.query, limit, space_id, allowed_document_ids)
                    .await?;
                let text_results = SearchMapper::port_dtos_to_domain(text_port_dtos);
                debug!(
                    result_count = text_results.len(),
                    preview = Self::summarize_domain_results(&text_results, 8),
                    "BM25-only search results"
                );
                Ok(SearchMapper::to_response_dto(text_results, query_time_ms))
            }
            SearchModeDto::Hybrid {
                vector_weight,
                bm25_weight,
            } => {
                // 1. Perform vector search (returns port DTOs)
                let query_embedding = self.embedding_service.embed_single(&request.query).await?;
                let vector_port_dtos = self.vector_search.search_scoped(
                    &query_embedding,
                    limit * 2,
                    0.3,
                    allowed_document_ids,
                )?; // Fetch more for fusion

                // 2. Perform text search (BM25, returns port DTOs)
                let text_port_dtos = self
                    .text_search
                    .search_scoped(&request.query, limit * 2, space_id, allowed_document_ids)
                    .await?;

                // 3. Map port DTOs to domain entities
                let vector_results = SearchMapper::port_dtos_to_domain(vector_port_dtos);
                let text_results = SearchMapper::port_dtos_to_domain(text_port_dtos);
                debug!(
                    vector_count = vector_results.len(),
                    bm25_count = text_results.len(),
                    vector_preview = Self::summarize_domain_results(&vector_results, 6),
                    bm25_preview = Self::summarize_domain_results(&text_results, 6),
                    "Hybrid pre-merge candidate results"
                );

                // 4. Merge results using weighted Reciprocal Rank Fusion
                let merged = self.merge_with_weighted_rrf(
                    vector_results,
                    text_results,
                    limit,
                    vector_weight,
                    bm25_weight,
                    &request.query,
                );
                debug!(
                    merged_count = merged.len(),
                    merged_preview = Self::summarize_domain_results(&merged, 8),
                    "Hybrid merged results"
                );

                Ok(SearchMapper::to_response_dto(merged, query_time_ms))
            }
        }
    }

    fn merge_with_weighted_rrf(
        &self,
        vector: Vec<SearchResult>,
        text: Vec<SearchResult>,
        limit: usize,
        vector_weight: f32,
        bm25_weight: f32,
        query_text: &str,
    ) -> Vec<SearchResult> {
        use std::collections::HashMap;

        // If caller provides degenerate weights, preserve previous balanced behavior.
        let (mut vw, mut bw) = (vector_weight.max(0.0), bm25_weight.max(0.0));
        let total = vw + bw;
        if total <= f32::EPSILON {
            return self.ranking_service.merge_with_rrf(vector, text, limit);
        }
        vw /= total;
        bw /= total;
        debug!(
            normalized_vector_weight = vw,
            normalized_bm25_weight = bw,
            query_len = query_text.len(),
            "Weighted RRF configuration"
        );
        let query_signal = Self::build_query_signal(query_text, &vector, &text);

        let mut scores: HashMap<String, (f32, f32, f32, SearchResult)> = HashMap::new();
        let rrf_k = 60.0_f32;

        for (rank, result) in vector.into_iter().enumerate() {
            let weighted_score = vw * (1.0 / (rrf_k + (rank + 1) as f32));
            let id = result.id().to_string();
            scores.insert(id, (weighted_score, weighted_score, 0.0, result));
        }

        for (rank, result) in text.into_iter().enumerate() {
            let weighted_score = bw * (1.0 / (rrf_k + (rank + 1) as f32));
            let id = result.id().to_string();
            scores
                .entry(id)
                .and_modify(|(score, _vector_score, bm25_score, _)| {
                    *score += weighted_score;
                    *bm25_score += weighted_score;
                })
                .or_insert((weighted_score, 0.0, weighted_score, result));
        }

        let bm25_heavy_mode = bw >= 0.7;
        let candidate_count = scores.len();
        let mut filtered_out = 0usize;
        let mut merged: Vec<(f32, f32, SearchResult)> = Vec::with_capacity(candidate_count);
        for (score, vector_score, bm25_score, result) in scores.into_values() {
            // In BM25-heavy mode, suppress vector-only hits that have no lexical support.
            let keep = !bm25_heavy_mode
                || (bm25_score > 0.0 && Self::has_meaningful_query_overlap(&result, &query_signal));
            if keep {
                merged.push((score, vector_score, result));
            } else {
                filtered_out += 1;
            }
        }
        debug!(
            candidate_count = candidate_count,
            kept_count = merged.len(),
            filtered_out = filtered_out,
            bm25_heavy_mode = bm25_heavy_mode,
            "Weighted RRF candidate filtering"
        );

        merged.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        merged
            .into_iter()
            .take(limit)
            .map(|(score, _vector_score, result)| {
                SearchResult::with_metadata(
                    result.id().to_string(),
                    score,
                    result.snippet().map(|s| s.to_string()),
                    result.document_id().map(|s| s.to_string()),
                    result.file_path().map(|s| s.to_string()),
                    result.position(),
                )
                .unwrap_or(result)
            })
            .collect()
    }

    fn summarize_domain_results(results: &[SearchResult], max_items: usize) -> String {
        if results.is_empty() {
            return "<none>".to_string();
        }

        results
            .iter()
            .take(max_items)
            .map(|r| {
                let doc_id = r.document_id().unwrap_or("-");
                let snippet = safe_truncate(r.snippet().unwrap_or(""), 48);
                format!(
                    "{}|score={:.4}|doc={}|snippet=\"{}\"",
                    r.id(),
                    r.score(),
                    doc_id,
                    snippet
                )
            })
            .collect::<Vec<_>>()
            .join("; ")
    }

    fn build_query_signal(
        query: &str,
        vector_candidates: &[SearchResult],
        lexical_candidates: &[SearchResult],
    ) -> (
        std::collections::HashSet<String>,
        std::collections::HashSet<String>,
    ) {
        use std::collections::HashSet;

        let mut primary = std::collections::HashSet::new();
        let mut stem_signal = std::collections::HashSet::new();

        let mut seen_query_terms = HashSet::new();
        let query_terms: Vec<String> = Self::tokenize_terms(query)
            .into_iter()
            .filter_map(|term| Self::normalize_signal_term(&term))
            .filter(|term| seen_query_terms.insert(term.clone()))
            .collect();

        if query_terms.is_empty() {
            return (primary, stem_signal);
        }

        let mut doc_terms: Vec<HashSet<String>> = Vec::new();
        let mut doc_stems: Vec<HashSet<String>> = Vec::new();
        for result in lexical_candidates
            .iter()
            .chain(vector_candidates.iter())
            .take(24)
        {
            let snippet = result.snippet().unwrap_or("");
            if snippet.trim().is_empty() {
                continue;
            }
            let terms: HashSet<String> = Self::tokenize_terms(snippet)
                .into_iter()
                .filter_map(|term| Self::normalize_signal_term(&term))
                .collect();
            if terms.is_empty() {
                continue;
            }
            let stems: HashSet<String> = terms
                .iter()
                .map(|term| Self::stem_term(term))
                .filter(|stem| stem.len() >= 3)
                .collect();
            doc_terms.push(terms);
            doc_stems.push(stems);
        }

        let selected_terms: Vec<String> = if doc_terms.is_empty() {
            Self::select_terms_by_entropy(query_terms, 8)
        } else {
            let total_docs = doc_terms.len() as f32;
            let mut scored: Vec<(String, f32, f32)> = query_terms
                .into_iter()
                .map(|term| {
                    let stem = Self::stem_term(&term);
                    let exact_df = doc_terms
                        .iter()
                        .filter(|terms| terms.contains(term.as_str()))
                        .count() as f32;
                    let stem_df = if stem.is_empty() {
                        0.0
                    } else {
                        doc_stems
                            .iter()
                            .filter(|stems| stems.contains(stem.as_str()))
                            .count() as f32
                    };
                    let effective_df = exact_df.max(stem_df);
                    let coverage = if total_docs > 0.0 {
                        effective_df / total_docs
                    } else {
                        0.0
                    };
                    let idf = ((total_docs + 1.0) / (effective_df + 1.0)).ln() + 1.0;
                    let entropy = Self::term_entropy(&term);
                    let length_factor = 1.0 + 0.08 * ((term.len() as f32) + 1.0).ln();
                    let score = idf * (0.6 + 0.4 * entropy) * length_factor;
                    (term, score, coverage)
                })
                .collect();

            scored.sort_by(|a, b| {
                b.1.partial_cmp(&a.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.0.cmp(&b.0))
            });

            let best_score = scored.first().map(|(_, score, _)| *score).unwrap_or(0.0);
            if best_score <= f32::EPSILON {
                scored
                    .into_iter()
                    .take(3)
                    .map(|(term, _, _)| term)
                    .collect()
            } else {
                let mut selected = Vec::new();
                for (term, score, coverage) in scored.into_iter() {
                    let within_relative_band = score >= best_score * 0.35;
                    let high_coverage_requires_stronger_signal =
                        coverage <= 0.74 || score >= best_score * 0.78;
                    if within_relative_band && high_coverage_requires_stronger_signal {
                        selected.push(term);
                        if selected.len() >= 8 {
                            break;
                        }
                    }
                }
                if selected.is_empty() {
                    selected = Self::select_terms_by_entropy(
                        Self::tokenize_terms(query)
                            .into_iter()
                            .filter_map(|term| Self::normalize_signal_term(&term))
                            .collect(),
                        3,
                    );
                }
                selected
            }
        };

        for term in selected_terms {
            primary.insert(term.clone());
            let stem = Self::stem_term(&term);
            if stem.len() >= 3 {
                stem_signal.insert(stem);
            }
        }

        (primary, stem_signal)
    }

    fn has_meaningful_query_overlap(
        result: &SearchResult,
        query_signal: &(
            std::collections::HashSet<String>,
            std::collections::HashSet<String>,
        ),
    ) -> bool {
        let (primary_terms, expanded_terms) = query_signal;
        if primary_terms.is_empty() && expanded_terms.is_empty() {
            return true;
        }

        let snippet = result.snippet().unwrap_or("");
        if snippet.trim().is_empty() {
            return false;
        }

        let snippet_terms: std::collections::HashSet<String> = Self::tokenize_terms(snippet)
            .into_iter()
            .filter_map(|term| Self::normalize_signal_term(&term))
            .collect();

        let primary_hits = primary_terms
            .iter()
            .filter(|term| snippet_terms.contains(term.as_str()))
            .count();
        if primary_hits >= 1 {
            return true;
        }

        let snippet_stems: std::collections::HashSet<String> = snippet_terms
            .iter()
            .map(|term| Self::stem_term(term))
            .filter(|stem| stem.len() >= 3)
            .collect();

        let stem_hits = expanded_terms
            .iter()
            .filter(|term| snippet_stems.contains(term.as_str()))
            .count();

        if primary_terms.len() <= 1 {
            stem_hits >= 1
        } else {
            stem_hits >= 2
        }
    }

    fn tokenize_terms(text: &str) -> Vec<String> {
        let mut terms: Vec<String> = Vec::new();
        let mut current = String::new();
        for ch in text.chars() {
            if ch.is_alphanumeric() {
                for lower in ch.to_lowercase() {
                    current.push(lower);
                }
            } else if !current.is_empty() {
                terms.push(current);
                current = String::new();
            }
        }
        if !current.is_empty() {
            terms.push(current);
        }
        terms
    }

    fn normalize_signal_term(term: &str) -> Option<String> {
        let trimmed = term.trim();
        if trimmed.len() < 3 {
            return None;
        }
        if !trimmed.chars().any(|ch| ch.is_ascii_alphabetic()) {
            return None;
        }
        Some(trimmed.to_string())
    }

    fn stem_term(term: &str) -> String {
        static EN_STEMMER: Lazy<Stemmer> = Lazy::new(|| Stemmer::create(Algorithm::English));
        EN_STEMMER.stem(term).to_string()
    }

    fn term_entropy(term: &str) -> f32 {
        use std::collections::HashMap;

        let len = term.len();
        if len == 0 {
            return 0.0;
        }

        let mut counts: HashMap<char, usize> = HashMap::new();
        for ch in term.chars() {
            *counts.entry(ch).or_insert(0) += 1;
        }
        let denom = len as f32;
        let entropy = counts.values().fold(0.0_f32, |acc, count| {
            let p = (*count as f32) / denom;
            if p <= f32::EPSILON {
                acc
            } else {
                acc - p * p.log2()
            }
        });
        (entropy / (denom + 1.0).ln()).max(0.0)
    }

    fn select_terms_by_entropy(mut terms: Vec<String>, max_terms: usize) -> Vec<String> {
        terms.sort_by(|a, b| {
            Self::term_entropy(b)
                .partial_cmp(&Self::term_entropy(a))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.cmp(b))
        });

        let best = terms
            .first()
            .map(|term| Self::term_entropy(term))
            .unwrap_or(0.0);
        if best <= f32::EPSILON {
            return terms.into_iter().take(max_terms).collect();
        }

        let filtered: Vec<String> = terms
            .into_iter()
            .filter(|term| Self::term_entropy(term) >= best * 0.45)
            .take(max_terms)
            .collect();
        if filtered.is_empty() {
            Vec::new()
        } else {
            filtered
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::HybridSearchUseCase;
    use crate::domain::entities::search_result::SearchResult;

    fn make_result(id: &str, snippet: &str, score: f32) -> SearchResult {
        SearchResult::with_metadata(
            id.to_string(),
            score,
            Some(snippet.to_string()),
            Some(format!("doc-{id}")),
            None,
            None,
        )
        .expect("valid result")
    }

    #[test]
    fn build_query_signal_drops_ubiquitous_low_information_terms() {
        let lexical = vec![
            make_result("1", "there blueberry study showed yield gains", 1.0),
            make_result("2", "there study measured anthocyanin levels", 0.9),
            make_result("3", "there methods section and controls", 0.8),
        ];
        let (primary, _stems) =
            HybridSearchUseCase::build_query_signal("there are blueberry studies", &[], &lexical);

        assert!(primary.contains("blueberry") || primary.contains("studies"));
        assert!(!primary.contains("there"));
    }

    #[test]
    fn overlap_accepts_stem_match_when_primary_is_single_term() {
        let query_signal = HybridSearchUseCase::build_query_signal("correlations", &[], &[]);
        let result = make_result("stem", "correlation analysis found significance", 0.7);

        assert!(HybridSearchUseCase::has_meaningful_query_overlap(
            &result,
            &query_signal
        ));
    }
}
