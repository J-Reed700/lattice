//! The search implementations behind the commands: fast, semantic, hybrid, full and similar-document.

use std::collections::HashSet;
use std::sync::Arc;

use crate::application::ports::RepositoryPort;
use crate::domain::entities::Document;
use crate::features::search::dto::{
    CacheStatsDto, EnhancedSearchResponse, SearchOptions, SearchRequestDto, SearchResponseDto,
    SearchResultDto,
};
use crate::features::search::query_cache::{CachedSearchResult, QueryCacheKey};
use crate::interfaces::di::container::Container;
use crate::shared::error::AppError;

use super::enrichment::{enrich_hybrid_results, enrich_vector_results, LibraryMode};

/// Implementation: Fast lightweight search returning minimal data (IDs and scores only).
///
/// This is the business logic implementation that performs the actual search.
/// It is separated from the Tauri command handler for better testability and
/// adherence to clean architecture principles.
///
/// # Architecture Note
///
/// This function contains the core business logic and returns ApiResult<T>.
/// The Tauri command handler `search_fast` below turns it into the command's
/// `Result` with `ApiResult::into_result`.
#[tracing::instrument(skip(container), fields(query = %query, limit = limit))]
pub(super) async fn search_fast_impl(
    container: &Container,
    query: String,
    limit: usize,
) -> crate::shared::ipc::ApiResult<Vec<(String, f32)>> {
    use crate::shared::ipc::{ApiResult, ErrorCode};

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

    let query_embedding = match embedding_service.embed_query(&query).await {
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

    ApiResult::success(result)
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
///
/// # DDD Architecture
///
/// This command is a **thin controller** that:
/// - Applies cross-cutting concerns (rate limiting, validation)
/// - Delegates to `SemanticSearchUseCase` for business logic
/// - Handles error translation
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
/// 5. Return enriched results
/// Implementation: Pure semantic search using vector similarity.
#[tracing::instrument(skip(container), fields(query = %request.query))]
pub async fn semantic_search_impl(
    container: &Container,
    request: SearchRequestDto,
) -> crate::shared::ipc::ApiResult<SearchResponseDto> {
    use crate::shared::ipc::{ApiResult, ErrorCode};

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

    let use_case = container.semantic_search_use_case();
    let response = match use_case.execute(request).await {
        Ok(response) => response,
        Err(e) => {
            return ApiResult::error(ErrorCode::ProcessingError, e.to_string());
        }
    };

    ApiResult::success(response)
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
///
/// # Search Modes
///
/// - **"semantic"**: Pure vector similarity
/// - **"keyword"**: Pure BM25 keyword matching
/// - **"hybrid"**: RRF fusion of vector + BM25, exactly as chat searches
/// - **Default**: Hybrid mode if unrecognized
///
/// # Ranking Algorithm
///
/// **Reciprocal Rank Fusion (RRF)**:
/// - Combines rankings from vector and BM25 searches
/// - Score = vector_weight/(k + vector_rank) + keyword_weight/(k + bm25_rank)
/// - k = 10, with 0.7 vector / 0.3 keyword weights
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
/// 3. Parse search mode string
/// 4. Run the library search orchestrator (vector + BM25 + RRF)
/// 5. Enrich results with metadata
/// 6. Return ranked results
///
/// Implementation: Hybrid search combining vector similarity and BM25 keyword matching.
pub async fn hybrid_search_impl(
    container: &Container,
    query: String,
    limit: usize,
    search_mode: String,
) -> crate::shared::ipc::ApiResult<Vec<SearchResultDto>> {
    use crate::shared::ipc::{ApiResult, ErrorCode};

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

    if let Err(e) = container.get_or_load_embedding().await {
        return ApiResult::error_with_details(
            ErrorCode::ModelNotLoaded,
            "Failed to load embedding model",
            format!(
                "{}. Download models from Settings → Models to enable search.",
                e
            ),
        );
    }

    // Determine search mode
    let mode = match search_mode.as_str() {
        "semantic" => LibraryMode::Vector,
        "keyword" => LibraryMode::Keyword,
        "hybrid" => LibraryMode::Hybrid,
        _ => LibraryMode::Hybrid,
    };

    let results = match container
        .hybrid_search_use_case()
        .execute(mode.request(&query, limit))
        .await
    {
        Ok(response) => response.results,
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

    ApiResult::success(search_results)
}

// Access tracking helper
async fn track_document_access(
    results: &[SearchResultDto],
    document_repo: Arc<dyn RepositoryPort<Document>>,
) -> Result<(), AppError> {
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
    let query_cache = container.query_cache();
    let cache_generation = query_cache.generation();
    let search_mode = match options.search_mode.as_deref() {
        Some("semantic") => LibraryMode::Vector,
        Some("keyword") => LibraryMode::Keyword,
        Some("hybrid") => LibraryMode::Hybrid,
        _ => LibraryMode::Vector,
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
        cache_generation,
        container.search.embedding_identity().map(str::to_owned),
    );

    if let Some(cached) = query_cache.get(&cache_key) {
        container.metrics().record_cache_hit();
        let stats = query_cache.stats();

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

        track_document_access(&response.results, container.document_repository()).await?;

        return Ok(response);
    }

    container.metrics().record_cache_miss();

    let embedding_service = container.get_or_load_embedding().await.map_err(|e| {
        AppError::AiModelsNotInstalled(
            format!("Failed to load embedding model: {}. Download models from Settings → Models to enable search.", e)
        )
    })?;

    let enrichment_service = container.search_enrichment_service();

    let search_results: Vec<SearchResultDto> = match search_mode {
        LibraryMode::Vector => {
            let query_embedding = embedding_service.embed_query(&options.query).await?;
            let search_service = container.search_service();
            let results = search_service.search_with_threshold(
                &query_embedding,
                limit,
                crate::shared::constants::MIN_SIMILARITY_SCORE,
            )?;
            enrich_vector_results(results, enrichment_service).await?
        }
        LibraryMode::Keyword | LibraryMode::Hybrid => {
            let response = container
                .hybrid_search_use_case()
                .execute(search_mode.request(&options.query, limit))
                .await?;
            enrich_hybrid_results(response.results, enrichment_service).await?
        }
    };

    let execution_time_ms = start_time.elapsed().as_millis() as u64;
    container.metrics().record_search(execution_time_ms);

    let cached_result = CachedSearchResult {
        results: search_results.clone(),
        cached_at: chrono::Utc::now().timestamp(),
        execution_time_ms,
    };
    query_cache.put_if_generation(cache_key, cached_result);

    let stats = query_cache.stats();
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

    Ok(similar)
}
