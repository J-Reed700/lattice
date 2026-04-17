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
//! - Apply rate limiting (CWE-770 mitigation)
//! - Validate inputs (security)
//! - Delegate to use cases (business logic)
//! - Handle caching (performance)
//! - Log audit events (CWE-778 mitigation)

use crate::features::search::dto::{
    CacheStatsDto, EnhancedSearchResponse, RecencySearchOptions, SearchOptions, SearchRequestDto,
    SearchResponseDto, SearchResultDto,
};
use crate::application::ports::RepositoryPort;
use crate::domain::entities::Document;
use crate::infrastructure::audit::{get_audit_logger, AuditAction, AuditEvent, AuditResult};
use crate::features::cache::query_cache::{CachedSearchResult, QueryCacheKey, QUERY_CACHE};
use crate::infrastructure::search::SearchMode;
use crate::infrastructure::services::traits::SearchEnrichmentServiceTrait;
use crate::interfaces::di::container::Container;
use crate::shared::error::AppError;
use serde::Deserialize;
use std::collections::HashSet;
use std::sync::Arc;
use tauri::State;

// Helper functions
async fn enrich_vector_results(
    results: Vec<crate::infrastructure::search::service::SearchResult>,
    enrichment_service: std::sync::Arc<dyn SearchEnrichmentServiceTrait>,
) -> Result<Vec<SearchResultDto>, AppError> {
    let chunk_ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
    let enriched_data = enrichment_service.enrich_results(&chunk_ids).await?;

    let search_results = results
        .into_iter()
        .map(|r| {
            let doc_metadata = enriched_data.get(&r.id);
            let content = doc_metadata.map(|m| m.snippet.clone()).unwrap_or_default();
            let title = doc_metadata
                .and_then(|m| m.metadata.get("filename"))
                .and_then(|v| v.as_str())
                .unwrap_or(&r.id)
                .to_string();
            let path = doc_metadata
                .and_then(|m| m.metadata.get("path"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let document_id = doc_metadata.map(|m| m.document_id.clone());
            let full_metadata = doc_metadata
                .map(|m| m.metadata.clone())
                .unwrap_or_else(std::collections::HashMap::new);

            SearchResultDto {
                id: r.id.clone(),
                title,
                content,
                score: r.score,
                path,
                document_id,
                position: Some(r.index),
                vector_score: Some(r.score),
                bm25_score: None,
                vector_rank: Some(r.index),
                bm25_rank: None,
                metadata: full_metadata,
            }
        })
        .collect();

    Ok(search_results)
}

async fn enrich_hybrid_results(
    results: Vec<crate::search::hybrid::HybridSearchResult>,
    enrichment_service: std::sync::Arc<dyn SearchEnrichmentServiceTrait>,
) -> Result<Vec<SearchResultDto>, AppError> {
    let chunk_ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
    tracing::debug!("Enriching {} hybrid search results", chunk_ids.len());

    let enriched_data = enrichment_service.enrich_results(&chunk_ids).await?;
    tracing::debug!(
        "Enrichment returned {} metadata entries",
        enriched_data.len()
    );

    let search_results = results
        .into_iter()
        .map(|r| {
            let doc_metadata = enriched_data.get(&r.id);

            if let Some(metadata) = doc_metadata {
                tracing::debug!(
                    "Chunk {} enriched with document_id: {:?}, filename: {:?}, path: {:?}",
                    r.id,
                    metadata.document_id,
                    metadata.metadata.get("filename"),
                    metadata.metadata.get("path")
                );
            } else {
                tracing::warn!("No enrichment data found for chunk {}", r.id);
            }

            let content = doc_metadata
                .map(|m| m.snippet.clone())
                .unwrap_or_default();
            let title = doc_metadata
                .and_then(|m| m.metadata.get("filename"))
                .and_then(|v| v.as_str())
                .unwrap_or(&r.id)
                .to_string();
            let path = doc_metadata
                .and_then(|m| m.metadata.get("path"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let document_id = doc_metadata
                .map(|m| m.document_id.clone());
            let full_metadata = doc_metadata
                .map(|m| m.metadata.clone())
                .unwrap_or_else(std::collections::HashMap::new);

            let result = SearchResultDto {
                id: r.id.clone(),
                title: title.clone(),
                content,
                score: r.score,
                path: path.clone(),
                document_id: document_id.clone(),
                position: None,
                vector_score: r.vector_score,
                bm25_score: r.bm25_score,
                vector_rank: r.vector_rank,
                bm25_rank: r.bm25_rank,
                metadata: full_metadata,
            };

            tracing::debug!(
                "Created SearchResultDto for chunk {}: title='{}', path={:?}, document_id={:?}, score={}",
                r.id, title, path, document_id, r.score
            );

            result
        })
        .collect();

    Ok(search_results)
}

/// Advanced document search with intelligent caching
///
/// Performs semantic, keyword, or hybrid search across indexed documents with automatic
/// query result caching. Supports multiple search modes and returns enriched results with
/// metadata, snippets, and ranking information. Cache hits dramatically improve response times.
///
/// # Arguments
///
/// * `container` - Service container with search services, cache, and security context
/// * `options` - Search configuration including query, mode, filters, and limit
///
/// # Returns
///
/// * `Ok(EnhancedSearchResponse)` - Search results with caching metadata and performance stats
/// * `Err(AppError)` - If rate limited, validation fails, or search execution fails
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many search requests (rate limited)
/// * `AppError::InvalidInput` - Query validation failed (empty, too long, malicious content)
/// * `AppError::Other` - Embedding generation or search execution failed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface SearchOptions {
///   query: string;
///   searchMode?: 'semantic' | 'keyword' | 'hybrid';
///   limit?: number;
///   filter?: Record<string, string>;
/// }
///
/// // Semantic search with caching
/// const response = await invoke('search_documents', {
///   options: {
///     query: 'machine learning algorithms',
///     searchMode: 'semantic',
///     limit: 20
///   }
/// });
///
/// console.log(`Found ${response.results.length} results`);
/// console.log(`From cache: ${response.fromCache}`);
/// console.log(`Execution time: ${response.executionTimeMs}ms`);
/// console.log(`Cache hit rate: ${response.cacheStats.hitRate}%`);
/// ```
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Prevents DoS attacks via excessive searches
/// - **Input Validation**: Sanitizes query to prevent injection attacks
/// - **Audit Logging (CWE-778)**: Logs all searches with result count and cache status
/// - **Access Tracking**: Records document access asynchronously for usage analytics
///
/// # Search Modes
///
/// - **Semantic** (`SearchMode::Vector`): Pure embedding similarity search
/// - **Keyword** (`SearchMode::Keyword`): Traditional BM25 full-text search
/// - **Hybrid** (`SearchMode::Hybrid`): Combines vector + BM25 with RRF ranking
///
/// # Caching Strategy
///
/// - **Cache Key**: Query + filters + limit + search mode
/// - **Cache Hit**: Returns cached results, bypasses expensive embedding/search
/// - **Cache Miss**: Executes search, stores result with timestamp and execution time
/// - **TTL**: Cached results automatically expire based on LRU eviction
///
/// # Command Flow
///
/// 1. Rate limiting check (prevent DoS)
/// 2. Input validation (sanitize query)
/// 3. Generate cache key from query + options
/// 4. **Cache lookup** - if hit, return cached results immediately
/// 5. Embed query using embedding service
/// 6. Execute search based on mode (vector/keyword/hybrid)
/// 7. Enrich results with document metadata and snippets
/// 8. Store results in cache for future requests
/// 9. Track document access asynchronously
/// 10. Log audit event with query and result count
#[tauri::command]
#[specta::specta]
pub async fn search_documents(
    container: State<'_, Container>,
    options: SearchOptions,
) -> Result<serde_json::Value, String> {
    use crate::interfaces::commands::api_boundary::TauriResultBoundary;

    let result = search_documents_impl(&container, options)
        .await
        .map_err(|e| e.to_string());

    match result {
        Ok(data) => Ok(serde_json::json!({ "ok": true, "data": data })),
        Err(e) => Ok(
            serde_json::json!({ "ok": false, "error": { "code": "SEARCH_ERROR", "message": e } }),
        ),
    }
}

/// Implementation: Advanced document search with intelligent caching
#[tracing::instrument(skip(container), fields(query = %options.query, limit = ?options.limit))]
async fn search_documents_original_impl(
    container: &Container,
    options: SearchOptions,
) -> Result<EnhancedSearchResponse, AppError> {
    let start_time = std::time::Instant::now();

    container
        .security_context()
        .rate_limiters()
        .search
        .check_rate_limit("search")
        .await?;

    container
        .security_context()
        .input_validator()
        .validate_search_query(&options.query)?;

    let limit = options.limit.unwrap_or(10);
    let search_mode = match options.search_mode.as_deref() {
        Some("semantic") => SearchMode::Vector,
        Some("keyword") => SearchMode::Keyword,
        Some("hybrid") => SearchMode::Hybrid,
        _ => SearchMode::Vector,
    };

    let cache_key = QueryCacheKey::new(
        options.query.clone(),
        options.filter.as_ref().map(|f| {
            f.iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                .collect()
        }),
        limit,
        format!("{:?}", search_mode),
    );

    if let Some(cached) = QUERY_CACHE.get(&cache_key) {
        container.metrics().record_cache_hit();
        let stats = QUERY_CACHE.stats();

        let response = EnhancedSearchResponse {
            results: cached.results,
            from_cache: true,
            execution_time_ms: start_time.elapsed().as_millis() as u64,
            cache_stats: Some(CacheStatsDto {
                size: stats.size,
                capacity: stats.capacity,
                hits: stats.hits,
                misses: stats.misses,
                total_time_saved_ms: stats.total_time_saved_ms,
                hit_rate: stats.hit_rate,
            }),
        };

        audit_search(&options.query, response.results.len(), true).await?;

        // Track document access (async, non-blocking)
        track_document_access(&response.results, container.document_repository()).await?;

        return Ok(response);
    }

    container.metrics().record_cache_miss();

    let embedding_service = container.get_or_load_embedding().await.map_err(|e| {
        AppError::AiModelsNotInstalled(
            format!("Failed to load embedding model: {}. Download models from Settings → Models to enable search.", e)
        )
    })?;
    let query_embedding = embedding_service.embed_single(&options.query).await?;

    let enrichment_service = container.search_enrichment_service();

    let search_results: Vec<SearchResultDto> = match search_mode {
        SearchMode::Vector => {
            let search_service = container.search_service();
            let results = search_service.search_with_threshold(
                &query_embedding,
                limit,
                crate::shared::constants::MIN_SIMILARITY_SCORE,
            )?;
            enrich_vector_results(results, enrichment_service).await?
        }
        SearchMode::Keyword | SearchMode::Hybrid => {
            let hybrid_service = container.hybrid_search();
            let results = hybrid_service
                .search(&options.query, &query_embedding, limit, search_mode)
                .await?;
            enrich_hybrid_results(results, enrichment_service).await?
        }
    };

    let execution_time_ms = start_time.elapsed().as_millis() as u64;
    container.metrics().record_search(execution_time_ms);

    let cached_result = CachedSearchResult {
        results: search_results.clone(),
        cached_at: chrono::Utc::now().timestamp(),
        execution_time_ms,
    };
    QUERY_CACHE.put(cache_key, cached_result);

    let stats = QUERY_CACHE.stats();
    let response = EnhancedSearchResponse {
        results: search_results,
        from_cache: false,
        execution_time_ms,
        cache_stats: Some(CacheStatsDto {
            size: stats.size,
            capacity: stats.capacity,
            hits: stats.hits,
            misses: stats.misses,
            total_time_saved_ms: stats.total_time_saved_ms,
            hit_rate: stats.hit_rate,
        }),
    };

    audit_search(&options.query, response.results.len(), false).await?;

    // Track document access (async, non-blocking)
    track_document_access(&response.results, container.document_repository()).await?;

    Ok(response)
}

/// Implementation: Fast lightweight search returning minimal data (IDs and scores only).
///
/// This is the business logic implementation that performs the actual search.
/// It is separated from the Tauri command handler for better testability and
/// adherence to clean architecture principles.
///
/// # Architecture Note
///
/// This function contains the core business logic and returns ApiResult<T>.
/// The Tauri command handler `search_fast` below wraps this implementation
/// using the TauriResultBoundary trait.
#[tracing::instrument(skip(container), fields(query = %query, limit = limit))]
async fn search_fast_impl(
    container: &Container,
    query: String,
    limit: usize,
) -> crate::shared::api_result::ApiResult<Vec<(String, f32)>> {
    use crate::shared::api_result::{ApiResult, ErrorCode};

    // Rate limiting check
    if let Err(e) = container
        .security_context()
        .rate_limiters()
        .search
        .check_rate_limit("search")
        .await
    {
        return ApiResult::error(ErrorCode::RateLimitExceeded, e.to_string());
    }

    // Input validation
    if let Err(e) = container
        .security_context()
        .input_validator()
        .validate_search_query(&query)
    {
        return ApiResult::error(ErrorCode::InvalidInput, e.to_string());
    }

    // Load embedding service
    let embedding_service = match container.get_or_load_embedding().await {
        Ok(service) => service,
        Err(e) => {
            return ApiResult::error_with_details(
                ErrorCode::ModelNotLoaded,
                "Failed to load embedding model",
                format!(
                    "{}. Download models from Settings → Models to enable search.",
                    e
                ),
            );
        }
    };

    // Generate query embedding
    let query_embedding = match embedding_service.embed_single(&query).await {
        Ok(embedding) => embedding,
        Err(e) => {
            return ApiResult::error(ErrorCode::EmbeddingError, e.to_string());
        }
    };

    // Perform vector search
    let search_service = container.search_service();
    let results = match search_service.search(&query_embedding, limit) {
        Ok(results) => results,
        Err(e) => {
            return ApiResult::error(ErrorCode::ProcessingError, e.to_string());
        }
    };

    // Transform results to tuples
    let result: Vec<(String, f32)> = results.into_iter().map(|r| (r.id, r.score)).collect();

    // Audit logging (non-blocking)
    if let Err(e) = audit_search(&query, result.len(), false).await {
        tracing::warn!("Failed to log audit event: {}", e);
    }

    ApiResult::success(result)
}

/// Fast lightweight search returning minimal data (IDs and scores only)
///
/// Optimized search that returns only chunk IDs and similarity scores without expensive
/// metadata enrichment. Ideal for autocomplete, quick lookups, or when only IDs are needed.
/// Does not use caching (results too lightweight to justify cache overhead).
///
/// # Arguments
///
/// * `container` - Service container with search service and security context
/// * `query` - Search query string
/// * `limit` - Maximum number of results to return
///
/// # Returns
///
/// * JSON response with structure:
///   - Success: `{ "ok": true, "data": [[chunk_id, score], ...] }`
///   - Error: `{ "ok": false, "error": { "code": "...", "message": "..." } }`
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Fast search without metadata
/// const response = await invoke<{
///   ok: boolean;
///   data?: [string, number][];
///   error?: { code: string; message: string; }
/// }>('search_fast', {
///   query: 'neural networks',
///   limit: 10
/// });
///
/// if (response.ok) {
///   response.data.forEach(([chunkId, score]) => {
///     console.log(`${chunkId}: ${score}`);
///   });
/// }
/// ```
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Prevents DoS attacks
/// - **Input Validation**: Sanitizes query
/// - **Audit Logging (CWE-778)**: Logs searches with result count
///
/// # Performance
///
/// - **No Caching**: Results are lightweight, cache overhead not justified
/// - **No Enrichment**: Skips expensive metadata/snippet lookup
/// - **Pure Vector Search**: Always uses semantic similarity (no hybrid)
/// - **Typical Response**: < 100ms for most queries
///
/// # Use Cases
///
/// - Autocomplete suggestions
/// - Quick ID lookups for subsequent operations
/// - Pre-filtering before enriched search
/// - Real-time search-as-you-type
#[tauri::command]
#[specta::specta]
pub async fn search_fast(
    container: State<'_, Container>,
    query: String,
    limit: usize,
) -> Result<serde_json::Value, String> {
    use crate::interfaces::commands::api_boundary::TauriResultBoundary;

    let result = search_fast_impl(&container, query, limit).await;
    result.to_tauri_result()
}

/// Pure semantic search using vector similarity (DDD use case)
///
/// Executes semantic search using embedding-based similarity without keyword matching.
/// This is the DDD-aligned version that delegates to a dedicated use case for business
/// logic encapsulation and testability.
///
/// # Arguments
///
/// * `container` - Service container with semantic search use case
/// * `request` - Search request DTO with query and options
///
/// # Returns
///
/// * `Ok(SearchResponseDto)` - Semantic search results with enriched metadata
/// * `Err(AppError)` - If rate limited, validation fails, or execution fails
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many search requests (rate limited)
/// * `AppError::InvalidInput` - Query validation failed
/// * `AppError::Other` - Use case execution failed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface SearchRequestDto {
///   query: string;
///   limit?: number;
///   filters?: Record<string, string>;
/// }
///
/// // DDD-aligned semantic search
/// const response = await invoke('semantic_search', {
///   request: {
///     query: 'quantum computing fundamentals',
///     limit: 15
///   }
/// });
///
/// console.log(`Found ${response.results.length} semantically similar documents`);
/// ```
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Prevents DoS attacks
/// - **Input Validation**: Sanitizes query
/// - **Audit Logging (CWE-778)**: Logs searches with result count
///
/// # DDD Architecture
///
/// This command is a **thin controller** that:
/// - Applies cross-cutting concerns (rate limiting, validation)
/// - Delegates to `SemanticSearchUseCase` for business logic
/// - Handles error translation and audit logging
///
/// # Comparison with Other Search Commands
///
/// - **vs search_documents**: DDD-aligned, no caching, pure semantic
/// - **vs search_fast**: Returns enriched results (not just IDs)
/// - **vs hybrid_search**: Pure vector similarity (no BM25)
///
/// # Command Flow
///
/// 1. Rate limiting check
/// 2. Input validation
/// 3. Delegate to SemanticSearchUseCase
/// 4. Use case handles embedding, search, enrichment
/// 5. Log audit event
/// 6. Return enriched results
/// Implementation: Pure semantic search using vector similarity.
#[tracing::instrument(skip(container), fields(query = %request.query))]
pub async fn semantic_search_impl(
    container: &Container,
    request: SearchRequestDto,
) -> crate::shared::api_result::ApiResult<SearchResponseDto> {
    use crate::shared::api_result::{ApiResult, ErrorCode};

    // Rate limiting check
    if let Err(e) = container
        .security_context()
        .rate_limiters()
        .search
        .check_rate_limit("search")
        .await
    {
        return ApiResult::error(ErrorCode::RateLimitExceeded, e.to_string());
    }

    // Input validation
    if let Err(e) = container
        .security_context()
        .input_validator()
        .validate_search_query(&request.query)
    {
        return ApiResult::error(ErrorCode::InvalidInput, e.to_string());
    }

    // Execute use case
    let use_case = container.semantic_search_use_case();
    let response = match use_case.execute(request).await {
        Ok(response) => response,
        Err(e) => {
            return ApiResult::error(ErrorCode::ProcessingError, e.to_string());
        }
    };

    // Audit logging (non-blocking)
    if !response.results.is_empty() {
        if let Some(first_result) = response.results.first() {
            if let Err(e) = audit_search(&first_result.id, response.results.len(), false).await {
                tracing::warn!("Failed to log audit event: {}", e);
            }
        }
    }

    ApiResult::success(response)
}

#[tauri::command]
#[specta::specta]
pub async fn semantic_search(
    container: State<'_, Container>,
    request: SearchRequestDto,
) -> Result<serde_json::Value, String> {
    use crate::interfaces::commands::api_boundary::TauriResultBoundary;

    let result = semantic_search_impl(&container, request).await;
    result.to_tauri_result()
}

/// Hybrid search combining vector similarity and BM25 keyword matching
///
/// Executes hybrid search that combines embedding-based semantic similarity with
/// traditional BM25 keyword matching using Reciprocal Rank Fusion (RRF) for ranking.
/// Supports fallback to pure semantic or pure keyword modes.
///
/// # Arguments
///
/// * `container` - Service container with hybrid search service
/// * `query` - Search query string
/// * `limit` - Maximum number of results to return
/// * `search_mode` - Search mode: "semantic", "keyword", or "hybrid"
///
/// # Returns
///
/// * `Ok(Vec<SearchResultDto>)` - Hybrid search results with dual ranking scores
/// * `Err(AppError)` - If rate limited, validation fails, or execution fails
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many search requests (rate limited)
/// * `AppError::InvalidInput` - Query validation failed
/// * `AppError::Other` - Embedding or search execution failed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Hybrid search with dual ranking
/// const results = await invoke('hybrid_search', {
///   query: 'transformer architecture attention mechanism',
///   limit: 20,
///   searchMode: 'hybrid'
/// });
///
/// results.forEach(result => {
///   console.log(`${result.title}`);
///   console.log(`  Vector score: ${result.vectorScore}`);
///   console.log(`  BM25 score: ${result.bm25Score}`);
///   console.log(`  Final score: ${result.score}`);
/// });
/// ```
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Prevents DoS attacks
/// - **Input Validation**: Sanitizes query
/// - **Audit Logging (CWE-778)**: Logs searches with result count
///
/// # Search Modes
///
/// - **"semantic"**: Pure vector similarity (falls back to `SearchMode::Vector`)
/// - **"keyword"**: Pure BM25 keyword matching (`SearchMode::Keyword`)
/// - **"hybrid"**: RRF fusion of vector + BM25 (`SearchMode::Hybrid`)
/// - **Default**: Hybrid mode if unrecognized
///
/// # Ranking Algorithm
///
/// **Reciprocal Rank Fusion (RRF)**:
/// - Combines rankings from vector and BM25 searches
/// - Score = 1/(k + vector_rank) + 1/(k + bm25_rank)
/// - k = 60 (standard RRF constant)
/// - Normalizes final scores to [0, 1]
///
/// # Performance
///
/// - **Hybrid**: ~2x slower than pure semantic (runs both searches)
/// - **Semantic**: Single vector search
/// - **Keyword**: Single BM25 search
///
/// # Command Flow
///
/// 1. Rate limiting check
/// 2. Input validation
/// 3. Parse search mode string → `SearchMode` enum
/// 4. Embed query
/// 5. Execute hybrid search (vector + BM25 + RRF)
/// 6. Enrich results with metadata
/// 7. Log audit event
/// 8. Return ranked results
/// Implementation: Hybrid search combining vector similarity and BM25 keyword matching.
pub async fn hybrid_search_impl(
    container: &Container,
    query: String,
    limit: usize,
    search_mode: String,
) -> crate::shared::api_result::ApiResult<Vec<SearchResultDto>> {
    use crate::shared::api_result::{ApiResult, ErrorCode};

    // Rate limiting check
    if let Err(e) = container
        .security_context()
        .rate_limiters()
        .search
        .check_rate_limit("search")
        .await
    {
        return ApiResult::error(ErrorCode::RateLimitExceeded, e.to_string());
    }

    // Input validation
    if let Err(e) = container
        .security_context()
        .input_validator()
        .validate_search_query(&query)
    {
        return ApiResult::error(ErrorCode::InvalidInput, e.to_string());
    }

    // Load embedding service
    let embedding_service = match container.get_or_load_embedding().await {
        Ok(service) => service,
        Err(e) => {
            return ApiResult::error_with_details(
                ErrorCode::ModelNotLoaded,
                "Failed to load embedding model",
                format!(
                    "{}. Download models from Settings → Models to enable search.",
                    e
                ),
            );
        }
    };

    // Generate query embedding
    let query_embedding = match embedding_service.embed_single(&query).await {
        Ok(embedding) => embedding,
        Err(e) => {
            return ApiResult::error(ErrorCode::EmbeddingError, e.to_string());
        }
    };

    // Determine search mode
    let mode = match search_mode.as_str() {
        "semantic" => SearchMode::Vector,
        "keyword" => SearchMode::Keyword,
        "hybrid" => SearchMode::Hybrid,
        _ => SearchMode::Hybrid,
    };

    // Perform hybrid search
    let hybrid_service = container.hybrid_search();
    let results = match hybrid_service
        .search(&query, &query_embedding, limit, mode)
        .await
    {
        Ok(results) => results,
        Err(e) => {
            return ApiResult::error(ErrorCode::ProcessingError, e.to_string());
        }
    };

    // Enrich results
    let enrichment_service = container.search_enrichment_service();
    let search_results = match enrich_hybrid_results(results, enrichment_service).await {
        Ok(results) => results,
        Err(e) => {
            return ApiResult::error(ErrorCode::ProcessingError, e.to_string());
        }
    };

    // Audit logging (non-blocking)
    if let Err(e) = audit_search(&query, search_results.len(), false).await {
        tracing::warn!("Failed to log audit event: {}", e);
    }

    ApiResult::success(search_results)
}

#[derive(Debug, Deserialize)]
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
) -> Result<serde_json::Value, String> {
    use crate::interfaces::commands::api_boundary::TauriResultBoundary;

    let result = hybrid_search_impl(&container, args.query, args.limit, args.search_mode).await;
    result.to_tauri_result()
}

/// Finds documents similar to a given chunk
///
/// Retrieves documents semantically similar to the specified chunk by using the chunk's
/// content as the search query. Automatically filters out the source chunk from results
/// to avoid self-matches. Useful for "more like this" features.
///
/// # Arguments
///
/// * `container` - Service container with search and enrichment services
/// * `chunk_id` - ID of the chunk to find similar documents for
/// * `limit` - Maximum number of similar documents to return (default: 10)
///
/// # Returns
///
/// * `Ok(Vec<SearchResultDto>)` - Similar documents ranked by semantic similarity
/// * `Err(AppError)` - If rate limited, chunk not found, or search fails
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many search requests (rate limited)
/// * `AppError::NotFound` - Chunk ID not found in database
/// * `AppError::Other` - Embedding or search execution failed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Find similar documents to a chunk
/// const similar = await invoke('find_similar', {
///   chunkId: 'chunk_abc123',
///   limit: 10
/// });
///
/// console.log(`Found ${similar.length} similar documents`);
/// similar.forEach(doc => {
///   console.log(`${doc.title}: ${doc.score}`);
/// });
/// ```
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Prevents DoS attacks
/// - **Audit Logging (CWE-778)**: Logs similarity searches with chunk ID
///
/// # Algorithm
///
/// 1. Retrieve source chunk content from database
/// 2. Generate embedding for chunk content
/// 3. Perform vector similarity search
/// 4. Filter out source chunk from results (prevent self-match)
/// 5. Take top N results after filtering
/// 6. Enrich with metadata
///
/// # Use Cases
///
/// - "More like this" document recommendations
/// - Related content suggestions
/// - Document clustering visualization
/// - Duplicate detection
///
/// # Command Flow
///
/// 1. Rate limiting check
/// 2. Fetch chunk content using enrichment service
/// 3. Error if chunk not found
/// 4. Embed chunk content
/// 5. Vector similarity search (limit + 1 to account for self-match)
/// 6. Filter out source chunk
/// 7. Take top `limit` results
/// 8. Enrich with metadata
/// 9. Log audit event with "similar:{chunk_id}" query
/// 10. Return similar documents
#[tauri::command]
#[specta::specta]
#[tracing::instrument(skip(container), fields(chunk_id = %chunk_id))]
pub async fn find_similar(
    container: State<'_, Container>,
    chunk_id: String,
    limit: Option<usize>,
) -> Result<Vec<SearchResultDto>, AppError> {
    container
        .security_context()
        .rate_limiters()
        .search
        .check_rate_limit("search")
        .await?;

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

    audit_search(&format!("similar:{}", chunk_id), similar.len(), false).await?;
    Ok(similar)
}

/// Time-aware search with recency boosting
///
/// Performs hybrid search with automatic boosting of recent documents. Combines semantic
/// similarity with temporal decay to surface fresh content while maintaining relevance.
/// Configurable recency weight and maximum age filter.
///
/// # Arguments
///
/// * `container` - Service container with hybrid search service
/// * `options` - Search options including query, recency weight, and age filter
///
/// # Returns
///
/// * `Ok(Vec<SearchResultDto>)` - Search results ranked with recency boost
/// * `Err(AppError)` - If rate limited, validation fails, or execution fails
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many search requests (rate limited)
/// * `AppError::InvalidInput` - Query validation failed, invalid recency weight, or negative max_age_days
/// * `AppError::Other` - Embedding or search execution failed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface RecencySearchOptions {
///   query: string;
///   limit?: number;
///   recencyWeight?: number;  // 0.0 to 1.0
///   maxAgeDays?: number;     // positive integer
/// }
///
/// // Search with recency boost (favor recent documents)
/// const results = await invoke('search_with_recency', {
///   options: {
///     query: 'project status',
///     limit: 15,
///     recencyWeight: 0.3,  // 30% weight to recency
///     maxAgeDays: 90       // Only last 3 months
///   }
/// });
///
/// console.log('Recent relevant documents:');
/// results.forEach(doc => {
///   console.log(`${doc.title} (score: ${doc.score})`);
/// });
/// ```
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Prevents DoS attacks
/// - **Input Validation**: Sanitizes query, validates recency_weight ∈ [0, 1], validates max_age_days > 0
/// - **Audit Logging (CWE-778)**: Logs searches with result count
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
///
/// # Use Cases
///
/// - Finding recent project updates
/// - News/blog search favoring fresh content
/// - Status report queries
/// - Time-sensitive document retrieval
///
/// # Command Flow
///
/// 1. Rate limiting check
/// 2. Input validation (query, recency_weight, max_age_days)
/// 3. Apply defaults (limit=10, recency_weight=0.1, max_age_days=730)
/// 4. Validate recency_weight ∈ [0, 1]
/// 5. Validate max_age_days > 0
/// 6. Embed query
/// 7. Execute recency-aware hybrid search
/// 8. Enrich results with metadata
/// 9. Log audit event
/// 10. Return time-boosted results
#[tauri::command]
#[specta::specta]
#[tracing::instrument(skip(container), fields(query = %options.query))]
pub async fn search_with_recency(
    container: State<'_, Container>,
    options: RecencySearchOptions,
) -> Result<Vec<SearchResultDto>, AppError> {
    container
        .security_context()
        .rate_limiters()
        .search
        .check_rate_limit("search")
        .await?;

    container
        .security_context()
        .input_validator()
        .validate_search_query(&options.query)?;

    let limit = options.limit.unwrap_or(10);
    let recency_weight = options.recency_weight.unwrap_or(0.1);
    let max_age_days = options.max_age_days.unwrap_or(730);

    if !(0.0..=1.0).contains(&recency_weight) {
        return Err(AppError::InvalidInput(
            "Recency weight must be between 0.0 and 1.0".to_string(),
        ));
    }

    if max_age_days <= 0 {
        return Err(AppError::InvalidInput(
            "Max age days must be positive".to_string(),
        ));
    }

    let embedding_service = container.get_or_load_embedding().await.map_err(|e| {
        AppError::AiModelsNotInstalled(
            format!("Failed to load embedding model: {}. Download models from Settings → Models to enable search.", e)
        )
    })?;
    let query_embedding = embedding_service.embed_single(&options.query).await?;

    let hybrid_service = container.hybrid_search();
    let results = hybrid_service
        .search_with_recency(
            &options.query,
            &query_embedding,
            limit,
            recency_weight,
            max_age_days,
        )
        .await?;

    let enrichment_service = container.search_enrichment_service();
    let search_results = enrich_hybrid_results(results, enrichment_service).await?;

    audit_search(&options.query, search_results.len(), false).await?;
    Ok(search_results)
}

// Audit helper
async fn audit_search(query: &str, result_count: usize, from_cache: bool) -> Result<(), AppError> {
    let audit_logger = get_audit_logger();
    let event = AuditEvent::new(AuditAction::SearchPerformed, AuditResult::success())
        .with_resource_id(query)
        .with_metadata("result_count", result_count.to_string())
        .with_metadata("from_cache", from_cache.to_string());

    audit_logger.log(event).await?;
    Ok(())
}

// Access tracking helper
async fn track_document_access(
    results: &[SearchResultDto],
    document_repo: Arc<dyn RepositoryPort<Document>>,
) -> Result<(), AppError> {
    // Extract unique document IDs from results
    let document_ids: HashSet<String> = results
        .iter()
        .filter_map(|r| r.document_id.clone())
        .collect();

    for doc_id in document_ids {
        if let Some(mut document) = document_repo.find_by_id(&doc_id).await? {
            document.increment_access();
            document_repo.save(&document).await?;
        }
    }
    Ok(())
}

// =============================================================================
// GATEWAY IMPL FUNCTIONS - Async implementations for gateway dispatch
// =============================================================================

/// Search documents implementation for gateway pattern
///
/// This async function is called directly by the gateway.
/// Async state is stored on heap via Futures.
#[tracing::instrument(skip(container), fields(query = %options.query, limit = ?options.limit))]
pub async fn search_documents_impl(
    container: &Container,
    options: SearchOptions,
) -> Result<EnhancedSearchResponse, AppError> {
    let start_time = std::time::Instant::now();

    container
        .security_context()
        .rate_limiters()
        .search
        .check_rate_limit("search")
        .await?;

    container
        .security_context()
        .input_validator()
        .validate_search_query(&options.query)?;

    let limit = options.limit.unwrap_or(10);
    let search_mode = match options.search_mode.as_deref() {
        Some("semantic") => SearchMode::Vector,
        Some("keyword") => SearchMode::Keyword,
        Some("hybrid") => SearchMode::Hybrid,
        _ => SearchMode::Vector,
    };

    let cache_key = QueryCacheKey::new(
        options.query.clone(),
        options.filter.as_ref().map(|f| {
            f.iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                .collect()
        }),
        limit,
        format!("{:?}", search_mode),
    );

    if let Some(cached) = QUERY_CACHE.get(&cache_key) {
        container.metrics().record_cache_hit();
        let stats = QUERY_CACHE.stats();

        let response = EnhancedSearchResponse {
            results: cached.results,
            from_cache: true,
            execution_time_ms: start_time.elapsed().as_millis() as u64,
            cache_stats: Some(CacheStatsDto {
                size: stats.size,
                capacity: stats.capacity,
                hits: stats.hits,
                misses: stats.misses,
                total_time_saved_ms: stats.total_time_saved_ms,
                hit_rate: stats.hit_rate,
            }),
        };

        audit_search(&options.query, response.results.len(), true).await?;
        track_document_access(&response.results, container.document_repository()).await?;

        return Ok(response);
    }

    container.metrics().record_cache_miss();

    let embedding_service = container.get_or_load_embedding().await.map_err(|e| {
        AppError::AiModelsNotInstalled(
            format!("Failed to load embedding model: {}. Download models from Settings → Models to enable search.", e)
        )
    })?;
    let query_embedding = embedding_service.embed_single(&options.query).await?;

    let enrichment_service = container.search_enrichment_service();

    let search_results: Vec<SearchResultDto> = match search_mode {
        SearchMode::Vector => {
            let search_service = container.search_service();
            let results = search_service.search_with_threshold(
                &query_embedding,
                limit,
                crate::shared::constants::MIN_SIMILARITY_SCORE,
            )?;
            enrich_vector_results(results, enrichment_service).await?
        }
        SearchMode::Keyword | SearchMode::Hybrid => {
            let hybrid_service = container.hybrid_search();
            let results = hybrid_service
                .search(&options.query, &query_embedding, limit, search_mode)
                .await?;
            enrich_hybrid_results(results, enrichment_service).await?
        }
    };

    let execution_time_ms = start_time.elapsed().as_millis() as u64;
    container.metrics().record_search(execution_time_ms);

    let cached_result = CachedSearchResult {
        results: search_results.clone(),
        cached_at: chrono::Utc::now().timestamp(),
        execution_time_ms,
    };
    QUERY_CACHE.put(cache_key, cached_result);

    let stats = QUERY_CACHE.stats();
    let response = EnhancedSearchResponse {
        results: search_results,
        from_cache: false,
        execution_time_ms,
        cache_stats: Some(CacheStatsDto {
            size: stats.size,
            capacity: stats.capacity,
            hits: stats.hits,
            misses: stats.misses,
            total_time_saved_ms: stats.total_time_saved_ms,
            hit_rate: stats.hit_rate,
        }),
    };

    audit_search(&options.query, response.results.len(), false).await?;
    track_document_access(&response.results, container.document_repository()).await?;

    Ok(response)
}

/// Find similar implementation for gateway pattern
#[tracing::instrument(skip(container), fields(chunk_id = %chunk_id))]
pub async fn find_similar_impl(
    container: &Container,
    chunk_id: String,
    limit: Option<usize>,
) -> Result<Vec<SearchResultDto>, AppError> {
    container
        .security_context()
        .rate_limiters()
        .search
        .check_rate_limit("search")
        .await?;

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

    audit_search(&format!("similar:{}", chunk_id), similar.len(), false).await?;
    Ok(similar)
}

/// Search with recency implementation for gateway pattern
#[tracing::instrument(skip(container), fields(query = %options.query))]
pub async fn search_with_recency_impl(
    container: &Container,
    options: RecencySearchOptions,
) -> Result<Vec<SearchResultDto>, AppError> {
    container
        .security_context()
        .rate_limiters()
        .search
        .check_rate_limit("search")
        .await?;

    container
        .security_context()
        .input_validator()
        .validate_search_query(&options.query)?;

    let limit = options.limit.unwrap_or(10);
    let recency_weight = options.recency_weight.unwrap_or(0.1);
    let max_age_days = options.max_age_days.unwrap_or(730);

    if !(0.0..=1.0).contains(&recency_weight) {
        return Err(AppError::InvalidInput(
            "Recency weight must be between 0.0 and 1.0".to_string(),
        ));
    }

    if max_age_days <= 0 {
        return Err(AppError::InvalidInput(
            "Max age days must be positive".to_string(),
        ));
    }

    let embedding_service = container.get_or_load_embedding().await.map_err(|e| {
        AppError::AiModelsNotInstalled(
            format!("Failed to load embedding model: {}. Download models from Settings → Models to enable search.", e)
        )
    })?;
    let query_embedding = embedding_service.embed_single(&options.query).await?;

    let hybrid_service = container.hybrid_search();
    let results = hybrid_service
        .search_with_recency(
            &options.query,
            &query_embedding,
            limit,
            recency_weight,
            max_age_days,
        )
        .await?;

    let enrichment_service = container.search_enrichment_service();
    let search_results = enrich_hybrid_results(results, enrichment_service).await?;

    audit_search(&options.query, search_results.len(), false).await?;
    Ok(search_results)
}

/// Batch search command for processing multiple queries in parallel
///
/// ## Overview
///
/// Processes multiple search queries efficiently in a single operation,
/// returning results for each query in the same order as provided.
///
/// ## Arguments
///
/// * `container` - DI container with services
/// * `queries` - List of search queries to process
/// * `limit` - Maximum results per query (default: 10)
/// * `search_mode` - Search algorithm to use (Vector, Keyword, Hybrid)
///
/// ## Returns
///
/// Vector of result vectors, one for each query in input order.
///
/// ## Security
///
/// - Rate limiting applied once for the batch
/// - Each query validated individually
/// - Audit logged as batch operation
///
/// ## Example
///
/// ```javascript
/// const results = await VaultAPI.batchSearch(
///   ['query1', 'query2', 'query3'],
///   10,
///   'hybrid'
/// );
/// // results[0] = results for 'query1'
/// // results[1] = results for 'query2'
/// // results[2] = results for 'query3'
/// ```
#[tauri::command]
#[specta::specta]
#[tracing::instrument(skip(container), fields(query_count = queries.len()))]
pub async fn batch_search(
    container: State<'_, Container>,
    queries: Vec<String>,
    limit: Option<usize>,
    search_mode: Option<String>,
) -> Result<Vec<Vec<SearchResultDto>>, AppError> {
    // Rate limit check for batch operation
    container
        .security_context()
        .rate_limiters()
        .search
        .check_rate_limit("batch_search")
        .await?;

    // Validate all queries
    for query in &queries {
        container
            .security_context()
            .input_validator()
            .validate_search_query(query)?;
    }

    let limit = limit.unwrap_or(10);
    let mode = match search_mode.as_deref() {
        Some("vector") => SearchMode::Vector,
        Some("keyword") => SearchMode::Keyword,
        Some("hybrid") => SearchMode::Hybrid,
        _ => SearchMode::Hybrid,
    };

    // Get embedding service
    let embedding_service = container.get_or_load_embedding().await.map_err(|e| {
        AppError::AiModelsNotInstalled(
            format!("Failed to load embedding model: {}. Download models from Settings → Models to enable search.", e)
        )
    })?;

    // Generate embeddings for all queries
    let mut query_embeddings = Vec::new();
    for query in &queries {
        let embedding = embedding_service.embed_single(query).await?;
        query_embeddings.push((query.clone(), embedding));
    }

    // Execute batch search
    let hybrid_service = container.hybrid_search();
    let batch_results = hybrid_service
        .batch_search(query_embeddings, limit, mode)
        .await?;

    // Enrich all results
    let enrichment_service = container.search_enrichment_service();
    let mut all_enriched_results = Vec::new();

    for results in batch_results {
        let enriched = enrich_hybrid_results(results, enrichment_service.clone()).await?;
        all_enriched_results.push(enriched);
    }

    // Audit the batch search
    let total_results: usize = all_enriched_results.iter().map(|r| r.len()).sum();
    audit_search(
        &format!("batch_search_{}_queries", queries.len()),
        total_results,
        false,
    )
    .await?;

    Ok(all_enriched_results)
}
