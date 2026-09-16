//! Hybrid Search Service
//!
//! Combines vector similarity search with keyword BM25 search using
//! Reciprocal Rank Fusion for optimal relevance.

use crate::features::search::engine::recency::{RecencyConfig, RecencyScorer};
use crate::features::search::engine::reranker::{blend_rerank_scores, Reranker};
use crate::shared::error::{AppError, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;

/// Search mode configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum SearchMode {
    /// Vector similarity search only
    Vector,
    /// Keyword BM25 search only
    Keyword,
    /// Hybrid search combining vector and keyword
    #[default]
    Hybrid,
}

/// Search configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    /// Search mode
    pub mode: SearchMode,
    /// Weight for vector search results (0.0-1.0)
    pub vector_weight: f32,
    /// Weight for keyword search results (0.0-1.0)
    pub keyword_weight: f32,
    /// Minimum similarity score threshold
    pub min_score: f32,
    /// Enable reranking
    pub enable_reranking: bool,
    /// Recency boost factor
    pub recency_boost: f32,
    /// Maximum results to fetch before filtering
    pub max_results: usize,
    /// Run the experimental learned sparse branch when the loaded model supports it.
    ///
    /// This remains available for explicit experiments, but defaults off: the
    /// evaluation found no quality win over weighted vector + BM25 fusion.
    #[serde(default = "default_sparse_enabled")]
    pub sparse_enabled: bool,
}

/// Sparse retrieval is an evaluation-only option. Written as a function because
/// `serde`'s `default` attribute needs one and old configurations omit the field.
fn default_sparse_enabled() -> bool {
    false
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            mode: SearchMode::Hybrid,
            vector_weight: crate::shared::constants::DEFAULT_VECTOR_FUSION_WEIGHT,
            keyword_weight: crate::shared::constants::DEFAULT_KEYWORD_FUSION_WEIGHT,
            min_score: 0.5,
            enable_reranking: false,
            recency_boost: 0.1,
            max_results: 100,
            sparse_enabled: default_sparse_enabled(),
        }
    }
}

/// Hybrid search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSearchResult {
    /// Chunk ID
    pub chunk_id: String,
    /// Document ID
    pub document_id: String,
    /// Combined relevance score
    pub score: f32,
    /// Vector similarity score
    pub vector_score: Option<f32>,
    /// Keyword BM25 score
    pub keyword_score: Option<f32>,
    /// Chunk content
    pub content: String,
    /// Document metadata
    pub metadata: Option<serde_json::Value>,
    /// Result ID (same as chunk_id for compatibility)
    pub id: String,
    /// BM25 score (same as keyword_score for compatibility)
    pub bm25_score: Option<f32>,
    /// Vector search ranking position
    pub vector_rank: Option<usize>,
    /// BM25 search ranking position
    pub bm25_rank: Option<usize>,
    /// Learned sparse score, when the sparse branch ran and matched this chunk
    #[serde(default)]
    pub sparse_score: Option<f32>,
    /// Learned sparse ranking position
    #[serde(default)]
    pub sparse_rank: Option<usize>,
}

/// Per-branch `(rank, score)` for every candidate each branch returned.
///
/// Kept separately from the fused result because the fusion helper collapses
/// its inputs pairwise and cannot report a faithful per-branch rank; these maps
/// are built from the branch outputs themselves, so `vector_rank` still means
/// "position in the vector branch" no matter how many branches were fused.
#[derive(Debug, Default)]
struct BranchSignals {
    vector: HashMap<String, (usize, f32)>,
    bm25: HashMap<String, (usize, f32)>,
    sparse: HashMap<String, (usize, f32)>,
}

impl BranchSignals {
    fn index<'a>(
        results: impl IntoIterator<Item = (&'a str, f32)>,
    ) -> HashMap<String, (usize, f32)> {
        results
            .into_iter()
            .enumerate()
            .map(|(rank, (id, score))| (id.to_owned(), (rank, score)))
            .collect()
    }
}

impl From<HybridSearchResult> for crate::features::search::engine::service::SearchResult {
    fn from(result: HybridSearchResult) -> Self {
        Self {
            id: result.chunk_id.clone(),
            score: result.score,
            index: 0, // Hybrid search doesn't use index
            filename: None,
            mime_type: None,
            size_bytes: None,
            created_at: None,
            content: Some(result.content),
            file_id: Some(result.document_id),
            file_path: None,
            file_name: None,
            file_extension: None,
            file_category: None,
            is_indexed: None,
            document_id: None,
            snippet: None,
            chunk_index: None,
            updated_at: None,
        }
    }
}

impl HybridSearchResult {
    /// Get chunk ID
    pub fn chunk_id(&self) -> &str {
        &self.chunk_id
    }

    /// Get document ID
    pub fn document_id(&self) -> &str {
        &self.document_id
    }

    /// Get combined relevance score
    pub fn score(&self) -> f32 {
        self.score
    }

    /// Get vector similarity score
    pub fn vector_score(&self) -> Option<f32> {
        self.vector_score
    }

    /// Get keyword BM25 score
    pub fn keyword_score(&self) -> Option<f32> {
        self.keyword_score
    }

    /// Get chunk content
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Get document metadata
    pub fn metadata(&self) -> Option<&serde_json::Value> {
        self.metadata.as_ref()
    }

    /// Get result ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get BM25 score
    pub fn bm25_score(&self) -> Option<f32> {
        self.bm25_score
    }

    /// Get vector search ranking position
    pub fn vector_rank(&self) -> Option<usize> {
        self.vector_rank
    }

    /// Get BM25 search ranking position
    pub fn bm25_rank(&self) -> Option<usize> {
        self.bm25_rank
    }

    /// Get the learned sparse score
    pub fn sparse_score(&self) -> Option<f32> {
        self.sparse_score
    }

    /// Get the learned sparse ranking position
    pub fn sparse_rank(&self) -> Option<usize> {
        self.sparse_rank
    }
}

/// Hybrid search service combining vector and keyword search
///
/// Combines USearch vector search (semantic search) with BM25Search (keyword search)
/// using Reciprocal Rank Fusion for optimal relevance.
///
/// # Architecture
///
/// The service composes independent search strategies:
/// - **Delegates** to SearchServiceTrait (USearch) and BM25Search (composition)
/// - **Fuses** results using ReciprocalRankFusion algorithm
/// - **Enriches** results with document metadata from database
/// - **Configurable** search modes: Vector, Keyword, or Hybrid
///
/// # Example
///
/// ```rust
/// let service = HybridSearchService::new(
///     vector_search,
///     bm25_search,
///     pool,
///     enrichment,
///     SearchConfig::default(),
/// );
///
/// let results = service.search(
///     "machine learning",
///     &query_embedding,
///     10,
///     SearchMode::Hybrid
/// ).await?;
/// ```
pub struct HybridSearchService {
    vector_search: Arc<dyn crate::features::search::SearchServiceTrait>,
    bm25_search: Arc<dyn crate::features::search::BM25SearchTrait>,
    // reason: retained from the public `new`/`with_rrf_k` signature used by DI and
    // tests/hybrid_search_integration_test.rs; all SQL now runs through the sub-services.
    #[allow(dead_code)]
    pool: SqlitePool,
    enrichment: Arc<dyn crate::infrastructure::services::traits::SearchEnrichmentServiceTrait>,
    config: SearchConfig,
    rrf_k: f32,
    reranker: Option<Arc<dyn Reranker>>,
    /// Learned sparse retrieval, when the composition root wired one. Absent
    /// (or present but reporting unavailable) means the existing two-way
    /// vector + BM25 fusion, unchanged.
    sparse_search: Option<Arc<dyn crate::features::search::SparseSearchTrait>>,
}

impl HybridSearchService {
    /// Create a new hybrid search service
    ///
    /// # Arguments
    ///
    /// * `vector_search` - Vector similarity search service
    /// * `bm25_search` - BM25 keyword search service
    /// * `pool` - SQLite database connection pool
    /// * `enrichment` - Service for enriching results with metadata
    /// * `config` - Search configuration
    pub fn new(
        vector_search: Arc<dyn crate::features::search::SearchServiceTrait>,
        bm25_search: Arc<dyn crate::features::search::BM25SearchTrait>,
        pool: SqlitePool,
        enrichment: Arc<dyn crate::infrastructure::services::traits::SearchEnrichmentServiceTrait>,
        config: SearchConfig,
    ) -> Self {
        Self {
            vector_search,
            bm25_search,
            pool,
            enrichment,
            config,
            rrf_k: crate::shared::constants::DEFAULT_RRF_K,
            reranker: None,
            sparse_search: None,
        }
    }

    /// Create a new hybrid search service with custom RRF k parameter
    pub fn with_rrf_k(
        vector_search: Arc<dyn crate::features::search::SearchServiceTrait>,
        bm25_search: Arc<dyn crate::features::search::BM25SearchTrait>,
        pool: SqlitePool,
        enrichment: Arc<dyn crate::infrastructure::services::traits::SearchEnrichmentServiceTrait>,
        rrf_k: f32,
    ) -> Self {
        Self {
            vector_search,
            bm25_search,
            pool,
            enrichment,
            config: SearchConfig::default(),
            rrf_k,
            reranker: None,
            sparse_search: None,
        }
    }

    /// Override the reciprocal-rank-fusion constant after construction.
    ///
    /// [`Self::with_rrf_k`] exists but discards the caller's `SearchConfig`;
    /// this keeps it, so an evaluation can sweep `k` over the same wiring the
    /// application uses.
    pub fn rrf_k(mut self, rrf_k: f32) -> Self {
        self.rrf_k = rrf_k;
        self
    }

    /// Add the shared reranker used by chat and direct search.
    pub fn with_reranker(mut self, reranker: Arc<dyn Reranker>) -> Self {
        self.reranker = Some(reranker);
        self
    }

    /// Add the learned sparse retrieval branch.
    ///
    /// Wiring it is not the same as turning it on: the branch still only runs
    /// when [`SearchConfig::sparse_enabled`] is set *and* the service reports
    /// [`SparseSearchTrait::is_available`], which tracks whether the currently
    /// loaded embedding model has a sparse head. That means the composition
    /// root can wire this unconditionally and a user switching from BGE-M3 to a
    /// dense-only model simply goes back to two-way fusion mid-session.
    ///
    /// [`SparseSearchTrait::is_available`]: crate::features::search::SparseSearchTrait::is_available
    #[must_use]
    pub fn with_sparse_search(
        mut self,
        sparse_search: Arc<dyn crate::features::search::SparseSearchTrait>,
    ) -> Self {
        self.sparse_search = Some(sparse_search);
        self
    }

    /// The sparse branch to run for this query, or `None` to keep two-way
    /// fusion.
    fn active_sparse_branch(&self) -> Option<&Arc<dyn crate::features::search::SparseSearchTrait>> {
        self.sparse_search
            .as_ref()
            .filter(|_| self.config.sparse_enabled)
            .filter(|sparse| sparse.is_available())
    }

    fn candidate_limit(&self, top_k: usize) -> usize {
        if top_k == 0 {
            return 0;
        }
        let can_rerank = self.config.enable_reranking
            && self
                .reranker
                .as_ref()
                .is_some_and(|reranker| reranker.is_available());
        if !can_rerank {
            return top_k;
        }
        top_k
            .saturating_mul(2)
            .min(self.config.max_results.max(top_k))
    }

    async fn finalize_results(
        &self,
        query_text: &str,
        mut results: Vec<HybridSearchResult>,
        top_k: usize,
    ) -> Vec<HybridSearchResult> {
        let candidate_limit = self.candidate_limit(top_k);
        results.truncate(candidate_limit);
        if results.len() <= 1 || query_text.trim().is_empty() {
            results.truncate(top_k);
            return results;
        }

        let Some(reranker) = self
            .reranker
            .as_ref()
            .filter(|_| self.config.enable_reranking)
            .filter(|reranker| reranker.is_available())
        else {
            results.truncate(top_k);
            return results;
        };

        let documents: Vec<String> = results
            .iter()
            .map(|result| result.content.clone())
            .collect();
        let original_scores: Vec<f32> = results.iter().map(|result| result.score).collect();
        let rerank_result = reranker
            .rerank(query_text, documents, results.len())
            .await
            .and_then(|ranked| blend_rerank_scores(&original_scores, &ranked));

        match rerank_result {
            Ok(scores) => {
                for (result, score) in results.iter_mut().zip(scores) {
                    result.score = score;
                }
                results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
                tracing::debug!(
                    candidate_count = results.len(),
                    result_count = top_k.min(results.len()),
                    "Applied shared cross-encoder reranker"
                );
            }
            Err(error) => {
                tracing::warn!(error = %error, "Cross-encoder reranking failed; preserving first-stage order");
            }
        }

        results.truncate(top_k);
        results
    }

    /// Perform hybrid search (legacy method for backward compatibility)
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<HybridSearchResult>> {
        let empty_embedding = vec![];
        <Self as crate::features::search::HybridSearchTrait>::search(
            self,
            query,
            &empty_embedding,
            limit,
            SearchMode::Hybrid,
        )
        .await
    }

    /// Convert vector search results to chunk IDs and scores
    fn vector_results_to_tuples(
        &self,
        results: Vec<crate::features::search::engine::service::SearchResult>,
    ) -> Vec<(String, f32)> {
        results.into_iter().map(|r| (r.id, r.score)).collect()
    }

    /// Convert BM25 search results to chunk IDs and scores
    fn bm25_results_to_tuples(
        &self,
        results: Vec<crate::features::search::engine::bm25::BM25Result>,
    ) -> Vec<(String, f32)> {
        results.into_iter().map(|r| (r.chunk_id, r.score)).collect()
    }

    /// Fuse vector and BM25 results using Reciprocal Rank Fusion
    fn fuse_results(
        &self,
        vector_results: Vec<(String, f32)>,
        bm25_results: Vec<(String, f32)>,
    ) -> Vec<crate::features::search::engine::fusion::FusionResult> {
        let rrf = crate::features::search::engine::fusion::ReciprocalRankFusion::new(self.rrf_k);
        rrf.fuse_weighted(
            vector_results,
            bm25_results,
            self.config.vector_weight,
            self.config.keyword_weight,
        )
    }

    /// Enrich fused results with document metadata
    ///
    /// Per-branch ranks come from `signals`, not from the fusion result: with
    /// three branches the fusion helper folds vector and BM25 together before
    /// it ever sees sparse, so its own `vector_rank`/`bm25_rank` describe
    /// intermediate lists rather than the branches a caller means.
    async fn enrich_fused_results(
        &self,
        fused_results: Vec<crate::features::search::engine::fusion::FusionResult>,
        signals: &BranchSignals,
    ) -> Result<Vec<HybridSearchResult>> {
        if fused_results.is_empty() {
            return Ok(vec![]);
        }

        let chunk_ids: Vec<String> = fused_results.iter().map(|r| r.id.clone()).collect();

        let metadata_map = self.enrichment.enrich_results(&chunk_ids).await?;

        let mut enriched = Vec::new();
        for fusion_result in fused_results {
            let metadata = metadata_map.get(&fusion_result.id);
            let document_id = metadata
                .map(|m| m.document_id.clone())
                .unwrap_or_else(|| fusion_result.id.clone());
            let vector = signals.vector.get(&fusion_result.id).copied();
            let bm25 = signals.bm25.get(&fusion_result.id).copied();
            let sparse = signals.sparse.get(&fusion_result.id).copied();
            let keyword_score = bm25.map(|(_, score)| score);

            enriched.push(HybridSearchResult {
                chunk_id: fusion_result.id.clone(),
                document_id,
                score: fusion_result.score,
                vector_score: vector.map(|(_, score)| score),
                keyword_score,
                content: metadata
                    .map(|m| m.snippet.clone())
                    .unwrap_or_else(String::new),
                metadata: metadata.and_then(|m| serde_json::to_value(&m.metadata).ok()),
                id: fusion_result.id.clone(),
                bm25_score: keyword_score,
                vector_rank: vector.map(|(rank, _)| rank),
                bm25_rank: bm25.map(|(rank, _)| rank),
                sparse_score: sparse.map(|(_, score)| score),
                sparse_rank: sparse.map(|(rank, _)| rank),
            });
        }

        Ok(enriched)
    }

    /// Convert vector-only results to HybridSearchResult
    async fn convert_vector_results(
        &self,
        results: Vec<crate::features::search::engine::service::SearchResult>,
    ) -> Result<Vec<HybridSearchResult>> {
        if results.is_empty() {
            return Ok(vec![]);
        }

        let chunk_ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
        let metadata_map = self.enrichment.enrich_results(&chunk_ids).await?;

        let mut enriched = Vec::new();
        for (rank, result) in results.into_iter().enumerate() {
            let metadata = metadata_map.get(&result.id);
            let document_id = metadata
                .map(|m| m.document_id.clone())
                .unwrap_or_else(|| result.id.clone());

            enriched.push(HybridSearchResult {
                chunk_id: result.id.clone(),
                document_id,
                score: result.score,
                vector_score: Some(result.score),
                keyword_score: None,
                content: metadata
                    .map(|m| m.snippet.clone())
                    .unwrap_or_else(String::new),
                metadata: metadata.and_then(|m| serde_json::to_value(&m.metadata).ok()),
                id: result.id.clone(),
                bm25_score: None,
                vector_rank: Some(rank),
                bm25_rank: None,
                sparse_score: None,
                sparse_rank: None,
            });
        }

        Ok(enriched)
    }

    /// Convert BM25-only results to HybridSearchResult
    async fn convert_bm25_results(
        &self,
        results: Vec<crate::features::search::engine::bm25::BM25Result>,
    ) -> Result<Vec<HybridSearchResult>> {
        if results.is_empty() {
            return Ok(vec![]);
        }

        let chunk_ids: Vec<String> = results.iter().map(|r| r.chunk_id.clone()).collect();
        let metadata_map = self.enrichment.enrich_results(&chunk_ids).await?;

        let mut enriched = Vec::new();
        for (rank, result) in results.into_iter().enumerate() {
            let metadata = metadata_map.get(&result.chunk_id);
            let document_id = metadata
                .map(|m| m.document_id.clone())
                .unwrap_or_else(|| result.document_id.clone());

            enriched.push(HybridSearchResult {
                chunk_id: result.chunk_id.clone(),
                document_id,
                score: result.score,
                vector_score: None,
                keyword_score: Some(result.score),
                content: metadata
                    .map(|m| m.snippet.clone())
                    .unwrap_or_else(String::new),
                metadata: metadata.and_then(|m| serde_json::to_value(&m.metadata).ok()),
                id: result.chunk_id.clone(),
                bm25_score: Some(result.score),
                vector_rank: None,
                bm25_rank: Some(rank),
                sparse_score: None,
                sparse_rank: None,
            });
        }

        Ok(enriched)
    }
}

use crate::features::search::HybridSearchTrait;

#[async_trait::async_trait]
impl HybridSearchTrait for HybridSearchService {
    async fn search(
        &self,
        query_text: &str,
        query_embedding: &[f32],
        top_k: usize,
        mode: SearchMode,
    ) -> Result<Vec<HybridSearchResult>> {
        match mode {
            SearchMode::Vector => {
                // Vector search only
                tracing::debug!("Performing vector-only search with {} results", top_k);

                if query_embedding.is_empty() {
                    return Err(AppError::InvalidInput(
                        "Query embedding required for vector search".to_string(),
                    ));
                }

                let vector_results = self.vector_search.search_with_threshold(
                    query_embedding,
                    self.candidate_limit(top_k),
                    self.config.min_score,
                )?;
                let results = self.convert_vector_results(vector_results).await?;
                Ok(self.finalize_results(query_text, results, top_k).await)
            }
            SearchMode::Keyword => {
                // BM25 keyword search only
                tracing::debug!("Performing keyword-only search with {} results", top_k);

                if query_text.trim().is_empty() {
                    return Err(AppError::InvalidInput(
                        "Query text required for keyword search".to_string(),
                    ));
                }

                let bm25_results = self
                    .bm25_search
                    .search(query_text, self.candidate_limit(top_k))
                    .await?;
                let results = self.convert_bm25_results(bm25_results).await?;
                Ok(self.finalize_results(query_text, results, top_k).await)
            }
            SearchMode::Hybrid => {
                // Hybrid search combining both
                tracing::debug!(
                    "Performing hybrid search with {} results (RRF k={})",
                    top_k,
                    self.rrf_k
                );

                if query_embedding.is_empty() {
                    // Fall back to keyword-only if no embedding
                    tracing::warn!("No embedding provided, falling back to keyword-only search");
                    let bm25_results = self
                        .bm25_search
                        .search(query_text, self.candidate_limit(top_k))
                        .await?;
                    let results = self.convert_bm25_results(bm25_results).await?;
                    return Ok(self.finalize_results(query_text, results, top_k).await);
                }

                if query_text.trim().is_empty() {
                    // Fall back to vector-only if no text
                    tracing::warn!("No query text provided, falling back to vector-only search");
                    let vector_results = self.vector_search.search_with_threshold(
                        query_embedding,
                        self.candidate_limit(top_k),
                        self.config.min_score,
                    )?;
                    let results = self.convert_vector_results(vector_results).await?;
                    return Ok(self.finalize_results(query_text, results, top_k).await);
                }

                let candidate_limit = self.candidate_limit(top_k);
                let expanded_limit = candidate_limit
                    .saturating_mul(2)
                    .min(self.config.max_results.max(candidate_limit));

                // The sparse branch races the other two rather than joining
                // their `try_join!`: a sparse failure must cost this query its
                // third branch, never its results. Vector and BM25 keep the
                // fail-fast semantics they already had.
                let sparse_branch = self.active_sparse_branch();
                let (core, sparse_outcome) = tokio::join!(
                    async {
                        tokio::try_join!(
                            async {
                                self.vector_search.search_with_threshold(
                                    query_embedding,
                                    expanded_limit,
                                    self.config.min_score,
                                )
                            },
                            async { self.bm25_search.search(query_text, expanded_limit).await }
                        )
                    },
                    async {
                        match sparse_branch {
                            Some(sparse) => {
                                sparse
                                    .search_scoped(query_text, expanded_limit, None, None)
                                    .await
                            }
                            None => Ok(Vec::new()),
                        }
                    }
                );
                let (vector_results, bm25_results) = core?;
                let sparse_results = sparse_outcome.unwrap_or_else(|error| {
                    tracing::warn!(%error, "Sparse retrieval branch failed; fusing the other two");
                    Vec::new()
                });

                tracing::debug!(
                    vector_results = vector_results.len(),
                    bm25_results = bm25_results.len(),
                    sparse_results = sparse_results.len(),
                    sparse_branch_active = sparse_branch.is_some(),
                    "Retrieval branches complete, fusing with RRF"
                );

                let signals = BranchSignals {
                    vector: BranchSignals::index(
                        vector_results.iter().map(|r| (r.id.as_str(), r.score)),
                    ),
                    bm25: BranchSignals::index(
                        bm25_results.iter().map(|r| (r.chunk_id.as_str(), r.score)),
                    ),
                    sparse: BranchSignals::index(
                        sparse_results
                            .iter()
                            .map(|r| (r.chunk_id.as_str(), r.score)),
                    ),
                };
                let vector_tuples = self.vector_results_to_tuples(vector_results);
                let bm25_tuples = self.bm25_results_to_tuples(bm25_results);
                let sparse_tuples: Vec<(String, f32)> = sparse_results
                    .into_iter()
                    .map(|result| (result.chunk_id, result.score))
                    .collect();

                // Fuse results using RRF. BM25 stays in the fusion even when
                // sparse runs: the sparse head can only activate terms that are
                // in the model's vocabulary, so exact identifiers, code symbols
                // and quoted strings still need a literal branch.
                let fused_results = if sparse_tuples.is_empty() {
                    self.fuse_results(vector_tuples, bm25_tuples)
                } else {
                    crate::features::search::engine::fusion::ReciprocalRankFusion::new(self.rrf_k)
                        .fuse_three_sources(
                            vector_tuples,
                            bm25_tuples,
                            sparse_tuples,
                            expanded_limit.max(candidate_limit),
                        )
                };

                // Enrich a wider first-stage pool so the cross-encoder can promote
                // a candidate that was not already in the final top K.
                let top_results: Vec<_> = fused_results.into_iter().take(candidate_limit).collect();

                tracing::debug!(
                    "Fusion complete, enriching {} final results",
                    top_results.len()
                );

                let results = self.enrich_fused_results(top_results, &signals).await?;
                Ok(self.finalize_results(query_text, results, top_k).await)
            }
        }
    }

    async fn batch_search(
        &self,
        queries: Vec<(String, Vec<f32>)>,
        top_k: usize,
        mode: SearchMode,
    ) -> Result<Vec<Vec<HybridSearchResult>>> {
        tracing::debug!(
            "Performing batch hybrid search for {} queries",
            queries.len()
        );

        let mut results = Vec::new();
        for (query_text, query_embedding) in queries {
            let search_results = <Self as HybridSearchTrait>::search(
                self,
                &query_text,
                &query_embedding,
                top_k,
                mode,
            )
            .await?;
            results.push(search_results);
        }

        tracing::debug!("Batch search complete, {} result sets", results.len());
        Ok(results)
    }

    async fn search_with_recency(
        &self,
        query_text: &str,
        query_embedding: &[f32],
        top_k: usize,
        recency_weight: f32,
        max_age_days: i64,
    ) -> Result<Vec<HybridSearchResult>> {
        let mut results = <Self as HybridSearchTrait>::search(
            self,
            query_text,
            query_embedding,
            top_k,
            SearchMode::Hybrid,
        )
        .await?;

        if results.is_empty() {
            return Ok(results);
        }

        let recency_weight = recency_weight.clamp(0.0, 1.0);
        if recency_weight == 0.0 {
            return Ok(results);
        }

        let scorer = RecencyScorer::new(RecencyConfig::with_max_age_days(max_age_days));
        let now = Utc::now();

        for result in &mut results {
            let updated_at = result
                .metadata
                .as_ref()
                .and_then(|meta| meta.get("updated_at"))
                .and_then(|value| value.as_str())
                .and_then(parse_sqlite_datetime);

            if let Some(updated_at) = updated_at {
                let age_days = scorer.calculate_age_days(updated_at, now);
                let recency_score = scorer.calculate_recency_score(age_days);
                result.score = scorer.boost_score(result.score, recency_score, recency_weight);
            }
        }

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));

        Ok(results)
    }
}

fn parse_sqlite_datetime(value: &str) -> Option<DateTime<Utc>> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
        return Some(parsed.with_timezone(&Utc));
    }

    NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S"))
        .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f"))
        .ok()
        .map(|naive| DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
}
