//! Turning a stored document back into text a prompt can carry: its text in
//! reading order and a cut to the window's budget. Shared by the follow-up
//! reuse of the last document and by attachments, so both read the same text.

use std::sync::Arc;

use lattice_core::application::ports::LLMPort;
use lattice_core::shared::text::safe_truncate;

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
