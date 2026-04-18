//! Hybrid Search Service
//!
//! Combines vector similarity search with keyword BM25 search using
//! Reciprocal Rank Fusion for optimal relevance.

use crate::infrastructure::search::recency::{RecencyConfig, RecencyScorer};
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
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            mode: SearchMode::Hybrid,
            vector_weight: 0.7,
            keyword_weight: 0.3,
            min_score: 0.5,
            enable_reranking: false,
            recency_boost: 0.1,
            max_results: 100,
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
}

impl From<HybridSearchResult> for crate::infrastructure::search::service::SearchResult {
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
}

/// Hybrid search service combining vector and keyword search
///
/// Combines USearch vector search (semantic search) with BM25Search (keyword search)
/// using Reciprocal Rank Fusion for optimal relevance.
///
/// # Architecture
///
/// Following the "bricks and studs" philosophy:
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
    pool: SqlitePool,
    enrichment: Arc<dyn crate::infrastructure::services::traits::SearchEnrichmentServiceTrait>,
    config: SearchConfig,
    rrf_k: f32,
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
            rrf_k: 60.0, // Default RRF k parameter
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
        }
    }

    /// Add reranker to the hybrid search service (placeholder for future)
    pub fn with_reranker<R>(self, _reranker: R) -> Self
    where
        R: std::fmt::Debug,
    {
        self
    }

    /// Perform hybrid search (legacy method for backward compatibility)
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<HybridSearchResult>> {
        // Use empty embedding for legacy calls
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
        results: Vec<crate::infrastructure::search::service::SearchResult>,
    ) -> Vec<(String, f32)> {
        results.into_iter().map(|r| (r.id, r.score)).collect()
    }

    /// Convert BM25 search results to chunk IDs and scores
    fn bm25_results_to_tuples(
        &self,
        results: Vec<crate::infrastructure::search::bm25::BM25Result>,
    ) -> Vec<(String, f32)> {
        results.into_iter().map(|r| (r.chunk_id, r.score)).collect()
    }

    /// Fuse vector and BM25 results using Reciprocal Rank Fusion
    fn fuse_results(
        &self,
        vector_results: Vec<(String, f32)>,
        bm25_results: Vec<(String, f32)>,
    ) -> Vec<crate::infrastructure::search::fusion::FusionResult> {
        let rrf = crate::infrastructure::search::fusion::ReciprocalRankFusion::new(self.rrf_k);
        rrf.fuse(vector_results, bm25_results)
    }

    /// Enrich fused results with document metadata
    async fn enrich_fused_results(
        &self,
        fused_results: Vec<crate::infrastructure::search::fusion::FusionResult>,
        vector_scores: &HashMap<String, f32>,
        bm25_scores: &HashMap<String, f32>,
    ) -> Result<Vec<HybridSearchResult>> {
        if fused_results.is_empty() {
            return Ok(vec![]);
        }

        // Extract chunk IDs for enrichment
        let chunk_ids: Vec<String> = fused_results.iter().map(|r| r.id.clone()).collect();

        // Fetch metadata
        let metadata_map = self.enrichment.enrich_results(&chunk_ids).await?;

        // Convert to HybridSearchResult with metadata
        let mut enriched = Vec::new();
        for fusion_result in fused_results {
            let metadata = metadata_map.get(&fusion_result.id);
            let document_id = metadata
                .map(|m| m.document_id.clone())
                .unwrap_or_else(|| fusion_result.id.clone());
            let vector_score = vector_scores.get(&fusion_result.id).copied();
            let keyword_score = bm25_scores.get(&fusion_result.id).copied();

            enriched.push(HybridSearchResult {
                chunk_id: fusion_result.id.clone(),
                document_id,
                score: fusion_result.score,
                vector_score,
                keyword_score,
                content: metadata
                    .map(|m| m.snippet.clone())
                    .unwrap_or_else(String::new),
                metadata: metadata.and_then(|m| serde_json::to_value(&m.metadata).ok()),
                id: fusion_result.id.clone(),
                bm25_score: keyword_score,
                vector_rank: fusion_result.vector_rank,
                bm25_rank: fusion_result.bm25_rank,
            });
        }

        Ok(enriched)
    }

    /// Convert vector-only results to HybridSearchResult
    async fn convert_vector_results(
        &self,
        results: Vec<crate::infrastructure::search::service::SearchResult>,
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
            });
        }

        Ok(enriched)
    }

    /// Convert BM25-only results to HybridSearchResult
    async fn convert_bm25_results(
        &self,
        results: Vec<crate::infrastructure::search::bm25::BM25Result>,
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
            });
        }

        Ok(enriched)
    }
}

// ============================================================================
// HybridSearchTrait Implementation
// ============================================================================

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
                    top_k,
                    self.config.min_score,
                )?;
                self.convert_vector_results(vector_results).await
            }
            SearchMode::Keyword => {
                // BM25 keyword search only
                tracing::debug!("Performing keyword-only search with {} results", top_k);

                if query_text.trim().is_empty() {
                    return Err(AppError::InvalidInput(
                        "Query text required for keyword search".to_string(),
                    ));
                }

                let bm25_results = self.bm25_search.search(query_text, top_k).await?;
                self.convert_bm25_results(bm25_results).await
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
                    let bm25_results = self.bm25_search.search(query_text, top_k).await?;
                    return self.convert_bm25_results(bm25_results).await;
                }

                if query_text.trim().is_empty() {
                    // Fall back to vector-only if no text
                    tracing::warn!("No query text provided, falling back to vector-only search");
                    let vector_results = self.vector_search.search_with_threshold(
                        query_embedding,
                        top_k,
                        self.config.min_score,
                    )?;
                    return self.convert_vector_results(vector_results).await;
                }

                // Run both searches in parallel (fetch more to allow for fusion)
                let expanded_limit = top_k * 2;

                let (vector_results, bm25_results) = tokio::try_join!(
                    async {
                        self.vector_search.search_with_threshold(
                            query_embedding,
                            expanded_limit,
                            self.config.min_score,
                        )
                    },
                    async { self.bm25_search.search(query_text, expanded_limit).await }
                )?;

                tracing::debug!(
                    "Got {} vector results and {} BM25 results, fusing with RRF",
                    vector_results.len(),
                    bm25_results.len()
                );

                // Convert to tuples for fusion
                let vector_score_map = self.vector_score_map(&vector_results);
                let bm25_score_map = self.bm25_score_map(&bm25_results);
                let vector_tuples = self.vector_results_to_tuples(vector_results);
                let bm25_tuples = self.bm25_results_to_tuples(bm25_results);

                // Fuse results using RRF
                let fused_results = self.fuse_results(vector_tuples, bm25_tuples);

                // Take top K and enrich with metadata
                let top_results: Vec<_> = fused_results.into_iter().take(top_k).collect();

                tracing::debug!(
                    "Fusion complete, enriching {} final results",
                    top_results.len()
                );

                self.enrich_fused_results(top_results, &vector_score_map, &bm25_score_map)
                    .await
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

impl HybridSearchService {
    fn vector_score_map(
        &self,
        results: &[crate::infrastructure::search::service::SearchResult],
    ) -> HashMap<String, f32> {
        results
            .iter()
            .map(|result| (result.id.clone(), result.score))
            .collect()
    }

    fn bm25_score_map(
        &self,
        results: &[crate::infrastructure::search::bm25::BM25Result],
    ) -> HashMap<String, f32> {
        results
            .iter()
            .map(|result| (result.chunk_id.clone(), result.score))
            .collect()
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
