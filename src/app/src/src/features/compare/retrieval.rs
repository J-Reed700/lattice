//! Chunk retrieval for one document across every requested column.
//!
//! Repository Barrier: the only state sources are the semantic search use case
//! and the chunk repository. No filesystem, no raw SQL.

use std::collections::HashSet;

use tracing::debug;

use crate::features::search::dto::{SearchModeDto, SearchRequestDto};
use crate::interfaces::di::Container;
use crate::shared::error::Result;

use super::use_case::{
    CHUNKS_PER_COLUMN, MAX_CHUNKS_PER_DOCUMENT, MAX_CHUNK_CHARS, MAX_CONTEXT_CHARS,
    SEMANTIC_THRESHOLD,
};

#[derive(Debug, Clone)]
pub struct RetrievedChunk {
    pub chunk_id: String,
    pub content: String,
    pub section: Option<String>,
    pub index: usize,
}

/// Char-safe truncation with a trailing ellipsis.
pub(super) fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut output = text
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    output.push('…');
    output
}

/// Chunks worth showing the model for one document across all columns.
/// Ordered by first appearance, deduped by chunk id, capped.
pub async fn retrieve_for_document(
    container: &Container,
    document_id: &str,
    columns: &[String],
) -> Result<Vec<RetrievedChunk>> {
    let mut scope: HashSet<String> = HashSet::new();
    scope.insert(document_id.to_string());

    let mut seen: HashSet<String> = HashSet::new();
    let mut chunks: Vec<RetrievedChunk> = Vec::new();

    let search = container.semantic_search_use_case();
    for column in columns {
        let request = SearchRequestDto {
            query: column.clone(),
            limit: Some(CHUNKS_PER_COLUMN),
            threshold: Some(SEMANTIC_THRESHOLD),
            mode: SearchModeDto::Vector,
        };
        match search.execute_scoped(request, Some(&scope)).await {
            Ok(response) => {
                for result in response.results {
                    if !seen.insert(result.id.clone()) {
                        continue;
                    }
                    chunks.push(RetrievedChunk {
                        chunk_id: result.id,
                        content: result.content,
                        section: None,
                        index: result.position.unwrap_or(chunks.len()),
                    });
                }
            }
            Err(error) => {
                debug!(
                    document_id,
                    column = column.as_str(),
                    %error,
                    "compare: scoped semantic search failed; will consider the chunk fallback"
                );
            }
        }
    }

    // Fallback: no embeddings, or nothing cleared the threshold. A document's
    // opening chunks are the best no-embedding guess, and this keeps compare
    // usable on a fresh install.
    if chunks.is_empty() {
        debug!(document_id, "compare: falling back to the chunk repository");
        let mut stored = container
            .chunk_repository()
            .find_by_document(document_id)
            .await?;
        stored.sort_by_key(|chunk| chunk.index());
        for chunk in stored.into_iter().take(MAX_CHUNKS_PER_DOCUMENT) {
            chunks.push(RetrievedChunk {
                chunk_id: chunk.id().to_string(),
                content: chunk.content().to_string(),
                section: chunk.section().map(|s| s.to_string()),
                index: chunk.index(),
            });
        }
    }

    for chunk in &mut chunks {
        chunk.content = truncate_chars(&chunk.content, MAX_CHUNK_CHARS);
    }
    chunks.truncate(MAX_CHUNKS_PER_DOCUMENT);

    // Drop from the tail until the whole context fits.
    while chunks.len() > 1 {
        let total: usize = chunks.iter().map(|c| c.content.chars().count()).sum();
        if total <= MAX_CONTEXT_CHARS {
            break;
        }
        chunks.pop();
    }

    Ok(chunks)
}
