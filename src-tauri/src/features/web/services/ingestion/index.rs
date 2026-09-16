//! Chunking of extracted article text ahead of embedding and storage.

use super::WebIngestionService;
use crate::features::indexing::engine::chunker::ContextualizedChunk;
use crate::features::indexing::use_cases::embedding_input::SpanEmbeddingGroup;
use crate::shared::error::Result;

/// The one context prefix every web-article chunk carries.
const WEB_ARTICLE_CONTEXT_PREFIX: &str = "[Document: Web Article]";

impl WebIngestionService {
    /// Split text losslessly within the loaded model's token budget.
    /// The shared policy includes the document prefix and special tokens.
    ///
    /// # Arguments
    ///
    /// * `text` - The text to chunk
    ///
    /// # Returns
    ///
    /// Vector of contextualized chunks with metadata
    ///
    /// # Errors
    ///
    /// - `AppError::TokenizationError` if tokenization fails
    pub(super) fn chunk_text(&self, text: &str) -> Result<Vec<ContextualizedChunk>> {
        let context_prefix = WEB_ARTICLE_CONTEXT_PREFIX;
        let prefix = format!("{context_prefix}\n\n");
        Ok(self
            .embedding_service
            .split_text(text, &prefix)?
            .into_iter()
            .enumerate()
            .map(|(index, part)| ContextualizedChunk {
                contextualized_content: format!("{prefix}{}", part.text),
                original_content: part.text,
                context_prefix: context_prefix.into(),
                chunk_index: index,
                token_count: part.token_count,
                start_idx: part.start,
                end_idx: part.end,
            })
            .collect())
    }

    /// The chunks of an article, grouped as the single structure span they are.
    ///
    /// An article is chunked in one pass under one prefix, so it is exactly one
    /// span: `prefix + text`, with every chunk addressed by its byte range
    /// inside it. That is the same shape `prepare_structured_with_spans`
    /// produces for a file, which is what lets both importers share
    /// `embed_prepared_chunks` and land in the same vector space.
    ///
    /// `None` when the chunks do not tile the text — a chunker that dropped or
    /// overlapped a range cannot be pooled from one forward pass, and the
    /// per-chunk path is the honest answer rather than a guess.
    pub(super) fn span_for_chunks(
        text: &str,
        chunks: &[ContextualizedChunk],
    ) -> Option<SpanEmbeddingGroup> {
        let prefix = format!("{WEB_ARTICLE_CONTEXT_PREFIX}\n\n");
        let mut covered = 0usize;
        let mut chunk_ranges = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            if chunk.start_idx != covered || chunk.end_idx < chunk.start_idx {
                return None;
            }
            covered = chunk.end_idx;
            chunk_ranges.push(prefix.len() + chunk.start_idx..prefix.len() + chunk.end_idx);
        }
        if chunk_ranges.is_empty() || covered != text.len() {
            return None;
        }
        Some(SpanEmbeddingGroup {
            span_text: format!("{prefix}{text}"),
            chunk_ranges,
            first_chunk_index: 0,
        })
    }
}
