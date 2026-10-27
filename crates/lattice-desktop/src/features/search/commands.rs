//! # Search Commands (DDD-Aligned)
//!
//! Command handlers for search operations following Domain-Driven Design.
//!
//! This module provides 6 search commands:
//! 1. `search_documents` - Advanced search with caching
//! 2. `search_fast` - Lightweight search without metadata
//! 3. `semantic_search` - Pure vector similarity search (DDD use case)
//! 4. `hybrid_search` - Combined vector + BM25 search (DDD use case)
//! 5. `find_similar` - Find similar documents
//! 6. `search_with_recency` - Time-aware search
//!
//! ## Architecture
//!
//! Commands are thin controllers that:
//! - Validate inputs (security)
//! - Delegate to use cases (business logic)
//! - Handle caching (performance)

use crate::interfaces::di::container::Container;
use lattice_core::application::contracts::search::{
    EnhancedSearchResponse, RecencySearchOptions, SearchOptions, SearchRequestDto,
    SearchResponseDto, SearchResultDto,
};
use lattice_core::shared::error::AppError;
use lattice_core::shared::ipc::ApiError;
use serde::{Deserialize, Serialize};
use tauri::State;

mod enrichment;
mod recency;
mod searches;

use enrichment::{enrich_hybrid_results, enrich_vector_results, LibraryMode};
pub use recency::search_with_recency_impl;
use searches::search_fast_impl;
pub use searches::{
    find_similar_impl, hybrid_search_impl, search_documents_impl, semantic_search_impl,
};

/// Advanced document search with intelligent caching
///
/// Performs semantic, keyword, or hybrid search across indexed documents with automatic
/// query result caching. Supports multiple search modes and returns enriched results with
/// metadata, snippets, and ranking information. Cache hits dramatically improve response times.
///
/// # Errors
///
/// * `AppError::InvalidInput` - Query validation failed (empty, too long, malicious content)
/// * `AppError::Other` - Embedding generation or search execution failed
///
/// # Security
///
/// - **Input Validation**: Sanitizes query to prevent injection attacks
/// - **Access Tracking**: Records document access asynchronously for usage analytics
///
/// # Search Modes
///
/// - **Semantic**: Pure embedding similarity search
/// - **Keyword**: Traditional BM25 full-text search
/// - **Hybrid**: Combines vector + BM25 with RRF ranking, exactly as chat searches
///
/// # Caching Strategy
///
/// - **Cache Key**: Query + filters + limit + search mode
/// - **Cache Hit**: Returns cached results, bypasses expensive embedding/search
/// - **Cache Miss**: Executes search, stores result with timestamp and execution time
/// - **TTL**: Cached results automatically expire based on LRU eviction
#[tauri::command]
#[specta::specta]
pub async fn search_documents(
    container: State<'_, Container>,
    options: SearchOptions,
) -> Result<EnhancedSearchResponse, ApiError> {
    search_documents_impl(&container, options)
        .await
        .map_err(ApiError::from)
}

/// Fast lightweight search returning minimal data (IDs and scores only)
///
/// Optimized search that returns only chunk IDs and similarity scores without expensive
/// metadata enrichment. Ideal for autocomplete, quick lookups, or when only IDs are needed.
/// Does not use caching (results too lightweight to justify cache overhead).
///
/// # Security
///
/// - **Input Validation**: Sanitizes query
#[tauri::command]
#[specta::specta]
pub async fn search_fast(
    container: State<'_, Container>,
    query: String,
    limit: usize,
) -> Result<Vec<(String, f32)>, ApiError> {
    search_fast_impl(&container, query, limit)
        .await
        .into_result()
}

#[tauri::command]
#[specta::specta]
pub async fn semantic_search(
    container: State<'_, Container>,
    request: SearchRequestDto,
) -> Result<SearchResponseDto, ApiError> {
    semantic_search_impl(&container, request)
        .await
        .into_result()
}

#[derive(Debug, Deserialize, specta::Type)]
pub struct HybridSearchArgs {
    pub query: String,
    pub limit: usize,
    #[serde(rename = "search_mode", alias = "searchMode")]
    pub search_mode: String,
}

#[tauri::command]
#[specta::specta]
pub async fn hybrid_search(
    container: State<'_, Container>,
    args: HybridSearchArgs,
) -> Result<Vec<SearchResultDto>, ApiError> {
    hybrid_search_impl(&container, args.query, args.limit, args.search_mode)
        .await
        .into_result()
}

/// Finds documents similar to a given chunk
///
/// Retrieves documents semantically similar to the specified chunk by using the chunk's
/// content as the search query. Automatically filters out the source chunk from results
/// to avoid self-matches. Useful for "more like this" features.
///
/// # Errors
///
/// * `AppError::NotFound` - Chunk ID not found in database
/// * `AppError::Other` - Embedding or search execution failed
///
/// # Algorithm
///
/// 1. Retrieve source chunk content from database
/// 2. Generate embedding for chunk content
/// 3. Perform vector similarity search
/// 4. Filter out source chunk from results (prevent self-match)
/// 5. Take top N results after filtering
/// 6. Enrich with metadata
#[tauri::command]
#[specta::specta]
#[tracing::instrument(skip(container), fields(chunk_id = %chunk_id))]
pub async fn find_similar(
    container: State<'_, Container>,
    chunk_id: String,
    limit: Option<usize>,
) -> Result<Vec<SearchResultDto>, AppError> {
    let limit = limit.unwrap_or(10);

    let enrichment_service = container.search_enrichment_service();
    let enriched = enrichment_service
        .enrich_results(std::slice::from_ref(&chunk_id))
        .await?;

    let doc_content = enriched
        .get(&chunk_id)
        .ok_or_else(|| AppError::NotFound(format!("Chunk not found: {}", chunk_id)))?;

    let embedding_service = container.get_or_load_embedding().await.map_err(|e| {
        AppError::AiModelsNotInstalled(
            format!("Failed to load embedding model: {}. Download models from Settings → Models to enable search.", e)
        )
    })?;
    let doc_embedding = embedding_service.embed_single(&doc_content.snippet).await?;

    let search_service = container.search_service();
    let results = search_service.search(&doc_embedding, limit + 1)?;

    let filtered: Vec<_> = results
        .into_iter()
        .filter(|r| r.id != chunk_id)
        .take(limit)
        .collect();

    let similar = enrich_vector_results(filtered, enrichment_service).await?;

    Ok(similar)
}

/// Time-aware search with recency boosting
///
/// Performs hybrid search with automatic boosting of recent documents. Combines semantic
/// similarity with temporal decay to surface fresh content while maintaining relevance.
/// Configurable recency weight and maximum age filter.
///
/// # Errors
///
/// * `AppError::InvalidInput` - Query validation failed, invalid recency weight, or negative max_age_days
/// * `AppError::Other` - Embedding or search execution failed
///
/// # Security
///
/// - **Input Validation**: Sanitizes query, validates recency_weight ∈ [0, 1], validates max_age_days > 0
///
/// # Recency Scoring
///
/// **Formula**: `final_score = (1 - recency_weight) * relevance_score + recency_weight * recency_score`
///
/// - **recency_weight**: 0.0 = pure relevance, 1.0 = pure recency
/// - **recency_score**: Exponential decay based on document age
/// - **max_age_days**: Hard filter - documents older than this are excluded
///
/// **Default Values**:
/// - `limit`: 10
/// - `recency_weight`: 0.1 (10% recency, 90% relevance)
/// - `max_age_days`: 730 (2 years)
///
/// # Validation
///
/// - `recency_weight` must be in range [0.0, 1.0]
/// - `max_age_days` must be positive (> 0)
/// - Empty validation errors return `AppError::InvalidInput`
#[tauri::command]
#[specta::specta]
#[tracing::instrument(skip(container), fields(query = %options.query))]
pub async fn search_with_recency(
    container: State<'_, Container>,
    options: RecencySearchOptions,
) -> Result<Vec<SearchResultDto>, AppError> {
    search_with_recency_impl(&container, options).await
}

/// One neighbour of a document, collapsed from its chunk hits.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SimilarDocumentDto {
    pub document_id: String,
    pub title: String,
    pub file_path: Option<String>,
    pub score: f32,
}

/// Documents whose content is closest to the given document.
///
/// `find_similar` is chunk-shaped and the frontend has no way to get a
/// document's chunk ids, so the whole round trip lives here: pick a
/// representative chunk, search, collapse the hits to one row per document.
#[tauri::command]
#[specta::specta]
#[tracing::instrument(skip(container), fields(document_id = %document_id))]
pub async fn find_similar_documents(
    container: State<'_, Container>,
    document_id: String,
    limit: Option<usize>,
) -> Result<Vec<SimilarDocumentDto>, AppError> {
    let limit = limit.unwrap_or(6).max(1);

    let chunks = container
        .chunk_repository()
        .find_by_document(&document_id)
        .await?;

    // The longest chunk, ties broken by lowest index. A PDF's first chunk is
    // often a title page, so longest beats first.
    let representative = chunks.iter().max_by(|a, b| {
        a.content_length()
            .cmp(&b.content_length())
            .then_with(|| b.index().cmp(&a.index()))
    });
    let Some(representative) = representative else {
        return Ok(vec![]);
    };
    let rep_chunk_id = representative.id().to_string();

    // Over-fetch: many hits collapse onto the same document.
    let hits = find_similar_impl(container.inner(), rep_chunk_id, Some(limit * 6)).await?;

    let mut best: std::collections::HashMap<String, (f32, String, Option<String>)> =
        std::collections::HashMap::new();
    for hit in hits {
        let Some(hit_document_id) = hit.document_id.clone() else {
            continue;
        };
        if hit_document_id == document_id {
            continue;
        }
        let entry = best
            .entry(hit_document_id)
            .or_insert_with(|| (hit.score, hit.title.clone(), hit.path.clone()));
        if hit.score > entry.0 {
            *entry = (hit.score, hit.title.clone(), hit.path.clone());
        }
    }

    let mut results: Vec<SimilarDocumentDto> = best
        .into_iter()
        .map(
            |(document_id, (score, title, file_path))| SimilarDocumentDto {
                document_id,
                title,
                file_path,
                score,
            },
        )
        .collect();
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.document_id.cmp(&b.document_id))
    });
    results.truncate(limit);

    Ok(results)
}

/// Batch search command for processing multiple queries in parallel
///
/// ## Overview
///
/// Processes multiple search queries efficiently in a single operation,
/// returning results for each query in the same order as provided.
///
/// ## Security
///
/// - Each query validated individually
#[tauri::command]
#[specta::specta]
#[tracing::instrument(skip(container), fields(query_count = queries.len()))]
pub async fn batch_search(
    container: State<'_, Container>,
    queries: Vec<String>,
    limit: Option<usize>,
    search_mode: Option<String>,
) -> Result<Vec<Vec<SearchResultDto>>, AppError> {
    for query in &queries {
        container
            .security_context()
            .input_validator()
            .validate_search_query(query)?;
    }

    let limit = limit.unwrap_or(10);
    let mode = match search_mode.as_deref() {
        Some("vector") => LibraryMode::Vector,
        Some("keyword") => LibraryMode::Keyword,
        Some("hybrid") => LibraryMode::Hybrid,
        _ => LibraryMode::Hybrid,
    };

    container.get_or_load_embedding().await.map_err(|e| {
        AppError::AiModelsNotInstalled(
            format!("Failed to load embedding model: {}. Download models from Settings → Models to enable search.", e)
        )
    })?;

    let search = container.hybrid_search_use_case();
    let enrichment_service = container.search_enrichment_service();
    let mut all_enriched_results = Vec::new();
    for query in &queries {
        let response = search.execute(mode.request(query, limit)).await?;
        let enriched = enrich_hybrid_results(response.results, enrichment_service.clone()).await?;
        all_enriched_results.push(enriched);
    }

    Ok(all_enriched_results)
}

#[cfg(test)]
mod tests;
