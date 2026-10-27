//! Chunking of extracted article text ahead of embedding and storage.

use super::WebIngestionService;
use lattice_core::domain::value_objects::ContextualizedChunk;
use lattice_core::shared::error::Result;

/// The one context prefix every web-article chunk carries.
const WEB_ARTICLE_CONTEXT_PREFIX: &str = "[Document: Web Article]";

impl WebIngestionService {
    /// Splits text losslessly within the loaded model's token budget, document
    /// prefix and special tokens included.
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
}
