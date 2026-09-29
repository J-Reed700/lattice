//! Chunk retrieval for one document across every requested column.
//!
//! Repository Barrier: the only state sources are the semantic search use case
//! and the chunk repository. No filesystem, no raw SQL.

use std::collections::HashSet;

use tracing::{debug, warn};

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

/// What one document contributes to a compare row.
#[derive(Debug, Clone, Default)]
pub struct DocumentChunks {
    pub chunks: Vec<RetrievedChunk>,
    /// Set when semantic search failed and the row answers from the
    /// document's opening passages instead: the values may not come from the
    /// parts of the document the columns ask about.
    pub degraded: Option<String>,
}

/// Shown on a row whose chunks came from the fallback after a search error.
pub(super) const DEGRADED_NOTE: &str =
    "Search failed for this document, so these values come from its opening passages.";

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
) -> Result<DocumentChunks> {
    let mut scope: HashSet<String> = HashSet::new();
    scope.insert(document_id.to_string());

    let mut seen: HashSet<String> = HashSet::new();
    let mut chunks: Vec<RetrievedChunk> = Vec::new();

    let mut search_failed = false;
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
                search_failed = true;
                warn!(
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
    let mut degraded = None;
    if chunks.is_empty() {
        if search_failed {
            warn!(
                document_id,
                "compare: search failed; answering from the opening chunks"
            );
            degraded = Some(DEGRADED_NOTE.to_string());
        } else {
            debug!(
                document_id,
                "compare: nothing cleared the threshold; using the opening chunks"
            );
        }
        let stored = container
            .chunk_repository()
            .find_first_by_document(document_id, MAX_CHUNKS_PER_DOCUMENT)
            .await?;
        for chunk in stored {
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

    while chunks.len() > 1 {
        let total: usize = chunks.iter().map(|c| c.content.chars().count()).sum();
        if total <= MAX_CONTEXT_CHARS {
            break;
        }
        chunks.pop();
    }

    Ok(DocumentChunks { chunks, degraded })
}
