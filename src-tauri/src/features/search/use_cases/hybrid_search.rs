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
//! use lattice::application::use_cases::search::hybrid_search::HybridSearchUseCase;
//! use lattice::application::dtos::search_dto::{SearchRequestDto, SearchModeDto};
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

use crate::application::ports::{EmbeddingPort, TextSearchPort, VectorSearchPort};
use crate::domain::entities::search_result::SearchResult;
use crate::features::search::dto::{SearchModeDto, SearchRequestDto, SearchResponseDto};
use crate::features::search::engine::fusion::{ReciprocalRankFusion, WeightedRanking};
use crate::features::search::mapper::SearchMapper;
use crate::features::search::SparseSearchTrait;
use crate::shared::error::{AppError, Result};
use crate::shared::text_utils::safe_truncate;
use once_cell::sync::Lazy;
use rust_stemmers::{Algorithm, Stemmer};
use tracing::{debug, info, warn};

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
    /// Learned sparse retrieval, when the composition root wired one and the
    /// caller asked for it. Wired the same way as direct search, and gated the
    /// same way: an explicit switch *and* the loaded model having a sparse head.
    sparse_search: Option<Arc<dyn SparseSearchTrait>>,
    sparse_enabled: bool,
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
            sparse_search: None,
            sparse_enabled: false,
        }
    }

    /// Attach the learned sparse branch, mirroring
    /// `HybridSearchService::with_sparse_search`.
    ///
    /// Wiring it is not the same as turning it on: `enabled` is the product
    /// switch and [`SparseSearchTrait::is_available`] is the capability check,
    /// so a user switching from BGE-M3 to a dense-only model simply goes back
    /// to two-way fusion mid-session.
    #[must_use]
    pub fn with_sparse_search(
        mut self,
        sparse_search: Arc<dyn SparseSearchTrait>,
        enabled: bool,
    ) -> Self {
        self.sparse_search = Some(sparse_search);
        self.sparse_enabled = enabled;
        self
    }

    /// The sparse branch to run for this query, or `None` to keep two-way fusion.
    fn active_sparse_branch(&self) -> Option<&Arc<dyn SparseSearchTrait>> {
        self.sparse_search
            .as_ref()
            .filter(|_| self.sparse_enabled)
            .filter(|sparse| sparse.is_available())
    }

    /// Run the vector branch without blocking the executor.
    ///
    /// The USearch query is synchronous, CPU-bound and can take tens of
    /// milliseconds over a large index. Awaiting it inline inside a `join!`
    /// stalls the whole runtime thread — including the lexical branch that was
    /// supposed to be running beside it.
    async fn vector_branch(
        &self,
        query_embedding: Vec<f32>,
        top_k: usize,
        threshold: f32,
        allowed_document_ids: Option<HashSet<String>>,
    ) -> Result<Vec<crate::features::search::dto::SearchResultPortDto>> {
        let vector_search = Arc::clone(&self.vector_search);
        tokio::task::spawn_blocking(move || {
            vector_search.search_scoped(
                &query_embedding,
                top_k,
                threshold,
                allowed_document_ids.as_ref(),
            )
        })
        .await
        .map_err(|error| AppError::InternalError(format!("Vector search task failed: {error}")))?
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
                let query_embedding = self.embedding_service.embed_query(&request.query).await?;
                let vector_port_dtos = self
                    .vector_branch(
                        query_embedding,
                        limit,
                        threshold,
                        allowed_document_ids.cloned(),
                    )
                    .await?;
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
                // 1. Embed the query, then run every branch at once. They share
                //    no state, and the lexical branch used to wait for the
                //    whole vector search before it even issued its SQL.
                let query_embedding = self.embedding_service.embed_query(&request.query).await?;
                let candidate_limit = limit * 2; // Fetch more for fusion
                let sparse_branch = self.active_sparse_branch();
                let (core, sparse_outcome) = tokio::join!(
                    async {
                        tokio::try_join!(
                            // `min_score` here is a cosine-similarity floor, the
                            // same scale `search_with_threshold` filters on — not
                            // a fused score.
                            self.vector_branch(
                                query_embedding,
                                candidate_limit,
                                0.3,
                                allowed_document_ids.cloned(),
                            ),
                            self.text_search.search_scoped(
                                &request.query,
                                candidate_limit,
                                space_id,
                                allowed_document_ids,
                            )
                        )
                    },
                    async {
                        match sparse_branch {
                            Some(sparse) => {
                                sparse
                                    .search_scoped(
                                        &request.query,
                                        candidate_limit,
                                        space_id,
                                        allowed_document_ids,
                                    )
                                    .await
                            }
                            None => Ok(Vec::new()),
                        }
                    }
                );
                let (vector_port_dtos, text_port_dtos) = core?;
                // A sparse failure costs this query its third branch, never its
                // results, exactly as in direct search.
                let sparse_port_dtos = sparse_outcome.unwrap_or_else(|error| {
                    warn!(%error, "Sparse retrieval branch failed; fusing the other two");
                    Vec::new()
                });

                // 2. Map port DTOs to domain entities
                let vector_results = SearchMapper::port_dtos_to_domain(vector_port_dtos);
                let text_results = SearchMapper::port_dtos_to_domain(text_port_dtos);
                let sparse_results = SearchMapper::port_dtos_to_domain(sparse_port_dtos);
                debug!(
                    vector_count = vector_results.len(),
                    bm25_count = text_results.len(),
                    sparse_count = sparse_results.len(),
                    vector_preview = Self::summarize_domain_results(&vector_results, 6),
                    bm25_preview = Self::summarize_domain_results(&text_results, 6),
                    "Hybrid pre-merge candidate results"
                );

                // 3. Merge results using weighted Reciprocal Rank Fusion
                let merged = Self::merge_with_weighted_rrf(
                    vector_results,
                    text_results,
                    sparse_results,
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

    /// Fuse the branches through the canonical weighted RRF, then apply this
    /// path's own lexical-support filter.
    ///
    /// The fusion arithmetic is not reimplemented here: `ReciprocalRankFusion`
    /// owns it, and this function only decides the weights, keeps the payloads
    /// and drops unsupported candidates.
    #[allow(clippy::too_many_arguments)]
    fn merge_with_weighted_rrf(
        vector: Vec<SearchResult>,
        text: Vec<SearchResult>,
        sparse: Vec<SearchResult>,
        limit: usize,
        vector_weight: f32,
        bm25_weight: f32,
        query_text: &str,
    ) -> Vec<SearchResult> {
        use std::collections::HashMap;

        // Degenerate weights mean "no preference", not "no fusion": fall back
        // to the equal weights that reproduce unweighted RRF.
        let (mut vw, mut bw) = (vector_weight.max(0.0), bm25_weight.max(0.0));
        let total = vw + bw;
        if total <= f32::EPSILON {
            vw = 0.5;
            bw = 0.5;
        } else {
            vw /= total;
            bw /= total;
        }
        debug!(
            normalized_vector_weight = vw,
            normalized_bm25_weight = bw,
            sparse_count = sparse.len(),
            query_len = query_text.len(),
            "Weighted RRF configuration"
        );
        let query_signal = Self::build_query_signal(query_text, &vector, &text);

        // Sparse and BM25 are both lexical signals over the same text, so they
        // share the lexical weight instead of each getting a vote of its own.
        let (bm25_branch_weight, sparse_branch_weight) = if sparse.is_empty() {
            (bw, 0.0)
        } else {
            (bw / 2.0, bw / 2.0)
        };

        // Keep one payload per id — the first branch that offered it wins, in
        // vector, BM25, sparse order — and hand the fusion only the rankings.
        let mut payloads: HashMap<String, SearchResult> = HashMap::new();
        let mut branches = Vec::with_capacity(3);
        for (weight, results) in [
            (vw, vector),
            (bm25_branch_weight, text),
            (sparse_branch_weight, sparse),
        ] {
            let mut ids = Vec::with_capacity(results.len());
            for result in results {
                let id = result.id().to_string();
                payloads.entry(id.clone()).or_insert(result);
                ids.push(id);
            }
            branches.push(WeightedRanking::new(weight, ids));
        }

        let lexical_branches = [1usize, 2];
        let fused = ReciprocalRankFusion::with_default().fuse_ranked(branches);

        let bm25_heavy_mode = bw >= 0.7;
        let candidate_count = fused.len();
        let mut filtered_out = 0usize;
        let mut merged: Vec<SearchResult> = Vec::with_capacity(candidate_count.min(limit));
        for entry in fused {
            let Some(result) = payloads.remove(&entry.id) else {
                continue;
            };
            // In BM25-heavy mode, suppress vector-only hits that have no lexical support.
            let has_lexical_rank = lexical_branches
                .iter()
                .any(|branch| entry.branch_rank(*branch).is_some());
            let keep = !bm25_heavy_mode
                || (has_lexical_rank && Self::has_meaningful_query_overlap(&result, &query_signal));
            if !keep {
                filtered_out += 1;
                continue;
            }
            merged.push(
                SearchResult::with_metadata(
                    result.id().to_string(),
                    entry.score,
                    result.snippet().map(|s| s.to_string()),
                    result.document_id().map(|s| s.to_string()),
                    result.file_path().map(|s| s.to_string()),
                    result.position(),
                )
                .unwrap_or(result),
            );
            if merged.len() >= limit {
                break;
            }
        }
        debug!(
            candidate_count = candidate_count,
            kept_count = merged.len(),
            filtered_out = filtered_out,
            bm25_heavy_mode = bm25_heavy_mode,
            "Weighted RRF candidate filtering"
        );

        merged
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

    fn merged_ids(
        vector: Vec<SearchResult>,
        text: Vec<SearchResult>,
        sparse: Vec<SearchResult>,
        vector_weight: f32,
        bm25_weight: f32,
    ) -> Vec<String> {
        HybridSearchUseCase::merge_with_weighted_rrf(
            vector,
            text,
            sparse,
            10,
            vector_weight,
            bm25_weight,
            "retention policy",
        )
        .into_iter()
        .map(|r| r.id().to_string())
        .collect()
    }

    /// Turning the sparse branch on must not move the vector/lexical balance:
    /// the two lexical branches share the keyword weight.
    #[test]
    fn a_sparse_branch_shares_the_lexical_weight_rather_than_adding_one() {
        let vector = vec![make_result("dense", "retention policy overview", 0.9)];
        let lexical = || vec![make_result("lexical", "retention policy appendix", 5.0)];

        let without = merged_ids(vector.clone(), lexical(), Vec::new(), 0.7, 0.3);
        assert_eq!(without.first().map(String::as_str), Some("dense"));

        // Both lexical branches agree on the same chunk and still do not
        // outvote one confident dense hit.
        let with = merged_ids(vector, lexical(), lexical(), 0.7, 0.3);
        assert_eq!(with.first().map(String::as_str), Some("dense"));
        assert!(with.contains(&"lexical".to_string()));
    }

    #[test]
    fn a_sparse_only_hit_still_reaches_the_merged_list() {
        let merged = merged_ids(
            vec![make_result("dense", "retention policy overview", 0.9)],
            vec![make_result("lexical", "retention policy appendix", 5.0)],
            vec![make_result("sparse", "policy for retained records", 3.0)],
            0.7,
            0.3,
        );
        assert!(merged.contains(&"sparse".to_string()), "{merged:?}");
    }

    /// Degenerate weights used to fall through to a second, unweighted copy of
    /// RRF. They now mean "no preference" inside the one implementation.
    #[test]
    fn degenerate_weights_fuse_with_equal_weights() {
        let merged = merged_ids(
            vec![make_result("a", "retention policy overview", 0.9)],
            vec![make_result("b", "retention policy appendix", 5.0)],
            Vec::new(),
            0.0,
            0.0,
        );
        assert_eq!(merged.len(), 2);
    }
}
