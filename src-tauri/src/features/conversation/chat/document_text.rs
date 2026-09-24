//! Turning a stored document back into text a prompt can carry.
//!
//! Two paths lift a whole document into a turn — the follow-up reuse of the
//! last document, and the files a reader attaches to a message. Both need the
//! same two things: the document's text in reading order, and a way to cut it
//! down to what the context window has left. One copy of each means the two
//! paths can never disagree about what "the document" is.

use std::sync::Arc;

use crate::application::ports::LLMPort;
use crate::domain::entities::document::Document;
use crate::shared::text_utils::safe_truncate;

/// The document's text in chunk order, falling back to the stored content for
/// a document that has not been chunked yet.
pub(super) fn assemble_document_text(document: &Document) -> String {
    let chunks = document.chunks();
    if chunks.is_empty() {
        return document.content().to_string();
    }

    let mut sorted_chunks: Vec<_> = chunks.iter().collect();
    sorted_chunks.sort_by_key(|c| c.index());
    sorted_chunks
        .iter()
        .map(|c| c.content())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Cut `content` down to at most `token_budget` tokens.
///
/// Tokens are estimated from a character ratio and then re-checked, because
/// the ratio under-counts on text the tokenizer splits finely; three passes is
/// enough to converge without turning a budget check into a search.
pub(super) fn truncate_to_token_budget(
    content: &str,
    token_budget: usize,
    llm: &Arc<dyn LLMPort>,
) -> String {
    if token_budget == 0 {
        return String::new();
    }

    let token_count = llm.count_tokens(content);
    if token_count <= token_budget {
        return content.to_string();
    }

    let total_chars = content.chars().count();
    if total_chars == 0 {
        return String::new();
    }

    let ratio = token_budget as f64 / token_count as f64;
    let mut target_chars = ((total_chars as f64) * ratio).floor() as usize;
    if target_chars == 0 {
        target_chars = 1;
    }

    let mut truncated = safe_truncate(content, target_chars);
    let mut attempts = 0;
    while llm.count_tokens(&truncated) > token_budget && attempts < 3 && target_chars > 1 {
        target_chars = ((target_chars as f64) * 0.8).floor() as usize;
        if target_chars == 0 {
            break;
        }
        truncated = safe_truncate(content, target_chars);
        attempts += 1;
    }

    truncated
}
