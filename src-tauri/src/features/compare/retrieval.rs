//! Chunk retrieval for one document across every requested column.
//!
//! Each column is searched the way chat searches the library: the hybrid
//! search's vector, BM25 and sparse branches, fused. How much of what comes
//! back reaches the model is decided by grounded generation against the
//! model's window, not here.
//!
//! Repository Barrier: the only state sources are the library search use case
//! and the chunk repository. No filesystem, no raw SQL.

use std::collections::HashSet;

use tracing::{debug, warn};

use crate::features::search::dto::SearchRequestDto;
use crate::interfaces::di::Container;
use crate::shared::error::Result;

use super::use_case::{CHUNKS_PER_COLUMN, MAX_CHUNKS_PER_DOCUMENT};

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

/// Chunks worth showing the model for one document across all columns,
/// best first: ordered by first appearance, deduped by chunk id, capped.
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
    let search = container.hybrid_search_use_case();
    for column in columns {
        let request = SearchRequestDto::hybrid(column, CHUNKS_PER_COLUMN);
        match search.execute_scoped(request, None, Some(&scope)).await {
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
                    "compare: scoped library search failed; will consider the chunk fallback"
                );
            }
        }
    }

    // Fallback: the search failed, or found nothing in this document. A
    // document's opening chunks are the best guess left, and this keeps compare
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
                "compare: the search found nothing; using the opening chunks"
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

    chunks.truncate(MAX_CHUNKS_PER_DOCUMENT);

    Ok(DocumentChunks { chunks, degraded })
}
