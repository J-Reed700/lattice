//! Turning vector and hybrid hits into result DTOs with document metadata.

use crate::features::search::dto::{SearchModeDto, SearchRequestDto, SearchResultDto};
use crate::infrastructure::services::traits::SearchEnrichmentServiceTrait;
use lattice_core::shared::error::AppError;

pub(super) async fn enrich_vector_results(
    results: Vec<crate::features::search::engine::service::SearchResult>,
    enrichment_service: std::sync::Arc<dyn SearchEnrichmentServiceTrait>,
) -> Result<Vec<SearchResultDto>, AppError> {
    let chunk_ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
    let enriched_data = enrichment_service.enrich_results(&chunk_ids).await?;

    let search_results = results
        .into_iter()
        .filter_map(|r| {
            // A hit the enrichment could not resolve is dropped, not rendered
            // as a bare chunk id with no file and no path. Enrichment refuses a
            // chat's attachments, which is how they stay off the search page,
            // and the only other way to land here is a chunk whose document row
            // is already gone.
            let doc_metadata = enriched_data.get(&r.id)?;
            let content = doc_metadata.snippet.clone();
            let title = doc_metadata
                .metadata
                .get("filename")
                .and_then(|v| v.as_str())
                .unwrap_or(&r.id)
                .to_string();
            let path = doc_metadata
                .metadata
                .get("path")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let document_id = Some(doc_metadata.document_id.clone());
            let full_metadata = doc_metadata.metadata.clone();

            Some(SearchResultDto {
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
            })
        })
        .collect();

    Ok(search_results)
}

/// The search page's ways of asking the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LibraryMode {
    Vector,
    Keyword,
    Hybrid,
}

impl LibraryMode {
    /// The orchestrator request for this mode. A semantic search keeps the
    /// search page's similarity floor; a hybrid one the application's fusion.
    pub(super) fn request(self, query: &str, limit: usize) -> SearchRequestDto {
        match self {
            Self::Vector => SearchRequestDto {
                query: query.to_owned(),
                limit: Some(limit),
                threshold: Some(lattice_core::shared::constants::MIN_SIMILARITY_SCORE),
                mode: SearchModeDto::Vector,
            },
            Self::Keyword => SearchRequestDto {
                query: query.to_owned(),
                limit: Some(limit),
                threshold: None,
                mode: SearchModeDto::BM25,
            },
            Self::Hybrid => SearchRequestDto::hybrid(query, limit),
        }
    }
}

/// Name fused hits after their documents. Order, scores and per-branch
/// diagnostics are the search's own; title, path, preview and metadata come
/// from the enrichment.
pub(super) async fn enrich_hybrid_results(
    results: Vec<SearchResultDto>,
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
        .filter_map(|r| {
            // Same rule as the vector branch: no document, no result. See
            // `enrich_vector_results`.
            let Some(doc_metadata) = enriched_data.get(&r.id) else {
                tracing::debug!("Dropping chunk {}: no library document behind it", r.id);
                return None;
            };

            tracing::debug!(
                "Chunk {} enriched with document_id: {:?}, filename: {:?}, path: {:?}",
                r.id,
                doc_metadata.document_id,
                doc_metadata.metadata.get("filename"),
                doc_metadata.metadata.get("path")
            );

            let content = doc_metadata.snippet.clone();
            let title = doc_metadata
                .metadata
                .get("filename")
                .and_then(|v| v.as_str())
                .unwrap_or(&r.id)
                .to_string();
            let path = doc_metadata
                .metadata
                .get("path")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let document_id = Some(doc_metadata.document_id.clone());
            let full_metadata = doc_metadata.metadata.clone();

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

            Some(result)
        })
        .collect();

    Ok(search_results)
}
