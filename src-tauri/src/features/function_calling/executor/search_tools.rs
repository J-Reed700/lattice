//! `semantic_search` tool handler and search-result shaping helpers.

use super::FunctionExecutor;
use crate::features::function_calling::domain::FunctionResult;
use crate::features::function_calling::dto::*;
use crate::features::search::dto::{SearchRequestDto, SearchResultDto};
use crate::features::search::engine::service::SearchResult as InfraSearchResult;
use crate::shared::error::{AppError, Result};
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::time::Instant;
use tracing::{debug, info};

impl FunctionExecutor {
    fn build_document_result_from_search(&self, result: InfraSearchResult) -> DocumentResult {
        let filename = result
            .filename
            .or(result.file_name)
            .unwrap_or_else(|| "Unknown".to_string());
        let file_path = result.file_path.unwrap_or_default();
        let mime_type = result
            .mime_type
            .unwrap_or_else(|| "application/octet-stream".to_string());
        let snippet = result.snippet.or(result.content).unwrap_or_default();
        let modified_at = result
            .updated_at
            .as_deref()
            .and_then(Self::parse_datetime)
            .unwrap_or_else(Utc::now);
        let size_bytes = result.size_bytes.unwrap_or(0);
        let document_id = result.document_id.or(result.file_id).unwrap_or(result.id);

        DocumentResult {
            document_id,
            filename,
            file_path,
            mime_type,
            score: result.score,
            snippet,
            chunk_index: result.chunk_index,
            modified_at,
            size_bytes,
        }
    }

    /// Name each fused hit after its document, keeping hit order. A hit
    /// whose document is gone is dropped rather than shown as a bare chunk.
    async fn label_hits(&self, hits: Vec<SearchResultDto>) -> Result<Vec<DocumentResult>> {
        let document_ids: Vec<String> = hits
            .iter()
            .filter_map(|hit| hit.document_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let documents: HashMap<String, _> = self
            .document_repository
            .find_metadata_by_ids(&document_ids)
            .await?
            .into_iter()
            .map(|document| (document.id().as_str().to_owned(), document))
            .collect();
        Ok(hits
            .into_iter()
            .filter_map(|hit| {
                let document = documents.get(hit.document_id.as_deref()?)?;
                Some(DocumentResult {
                    document_id: document.id().as_str().to_owned(),
                    filename: document.file_name().to_owned(),
                    file_path: document.file_path().display().to_string(),
                    mime_type: document.mime_type().to_owned(),
                    score: hit.score,
                    snippet: hit.content,
                    chunk_index: hit.position,
                    modified_at: *document.modified_at(),
                    size_bytes: document.size_bytes(),
                })
            })
            .collect())
    }

    pub(super) fn build_document_evidence(results: &[DocumentResult]) -> Vec<DocumentEvidence> {
        let mut groups: Vec<DocumentEvidence> = Vec::new();
        let mut by_document: HashMap<String, usize> = HashMap::new();

        for result in results {
            let entry_index = if let Some(index) = by_document.get(&result.document_id) {
                *index
            } else {
                let index = groups.len();
                groups.push(DocumentEvidence {
                    document_id: result.document_id.clone(),
                    filename: result.filename.clone(),
                    file_path: result.file_path.clone(),
                    mime_type: result.mime_type.clone(),
                    max_score: result.score,
                    match_count: 0,
                    matches: Vec::new(),
                });
                by_document.insert(result.document_id.clone(), index);
                index
            };

            if let Some(group) = groups.get_mut(entry_index) {
                group.max_score = group.max_score.max(result.score);
                group.matches.push(DocumentEvidenceMatch {
                    chunk_index: result.chunk_index,
                    score: result.score,
                    excerpt: Self::truncate_excerpt(&result.snippet, 280),
                });
                group.match_count = group.matches.len();
            }
        }

        for group in &mut groups {
            group.matches.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.chunk_index.cmp(&b.chunk_index))
            });
            if group.matches.len() > 8 {
                group.matches.truncate(8);
                group.match_count = group.matches.len();
            }
        }

        groups.sort_by(|a, b| {
            b.max_score
                .partial_cmp(&a.max_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.match_count.cmp(&a.match_count))
                .then_with(|| a.filename.cmp(&b.filename))
        });

        groups
    }

    fn truncate_excerpt(text: &str, max_chars: usize) -> String {
        if text.chars().count() <= max_chars {
            return text.to_string();
        }

        let mut truncated: String = text.chars().take(max_chars).collect();
        truncated.push_str("...");
        truncated
    }

    /// Execute semantic_search function
    ///
    /// Boxed to prevent stack overflow: all handlers are boxed so the execute()
    /// match arms hold small Pin<Box<...>> pointers instead of inline futures.
    pub(super) fn handle_semantic_search<'a>(
        &'a self,
        args: &'a serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FunctionResult>> + Send + 'a>>
    {
        Box::pin(async move {
            let start = Instant::now();

            // Deserialize and validate input
            let input: SemanticSearchInput = serde_json::from_value(args.clone())
                .map_err(|e| AppError::InvalidInput(format!("Invalid arguments: {}", e)))?;

            debug!(
                "Semantic search: query='{}', limit={}",
                input.query, input.limit
            );

            let doc_results: Vec<DocumentResult> = match input.search_mode {
                SearchMode::Semantic => {
                    let embedding = self.embedding_service.embed_query(&input.query).await?;
                    let results = self
                        .search_service
                        .search_with_metadata(&embedding, input.limit)
                        .await?
                        .into_iter()
                        .filter(|r| r.score >= input.threshold)
                        .map(|r| self.build_document_result_from_search(r))
                        .collect::<Vec<_>>();
                    results
                }
                SearchMode::Keyword => self
                    .bm25_service
                    .search(&input.query, input.limit)
                    .await?
                    .into_iter()
                    .map(|r| self.build_document_result_from_search(r.into()))
                    .collect(),
                // The search chat runs, vault-wide.
                SearchMode::Hybrid => {
                    let response = self
                        .hybrid_search
                        .execute(SearchRequestDto::hybrid(input.query.clone(), input.limit))
                        .await?;
                    self.label_hits(response.results).await?
                }
            };

            let search_time_ms = start.elapsed().as_secs_f64() * 1000.0;
            let documents = Self::build_document_evidence(&doc_results);

            let output = SemanticSearchOutput {
                results: doc_results.clone(),
                documents,
                total_found: doc_results.len(),
                search_time_ms,
                query: input.query,
            };

            info!(
                "Semantic search completed: {} results in {:.2}ms",
                output.total_found, search_time_ms
            );

            Ok(FunctionResult::success(serde_json::to_value(output)?))
        }) // Box::pin handle_semantic_search
    }
}
