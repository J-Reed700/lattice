//! Search commands (DDD-aligned).

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
