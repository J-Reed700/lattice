//! What each learning job needs from the Learning model, and the refusal a
//! model gets when its window is smaller.
//!
//! A floor comes from the job's own answer reservation and the budget
//! planner's rule for it: the planner never reserves more than half a model's
//! window for the answer (`BudgetAllocation::plan`), so a job that reserves
//! `n` tokens for its answer needs a window of at least `2n`, and a smaller
//! window cuts the answer itself short. Jobs whose calls size their answers
//! to fit state no floor and are refused, if at all, by the planner on the
//! request that does not fit.
//!
//! Requirements go to `learning_llm`, which can refuse before anything is
//! loaded; [`ensure_window`] checks the model that was loaded, before any
//! research or model call is spent on the job.

use crate::features::learning::generation::{
    recall_output_tokens, LESSON_OUTPUT_RESERVE, OUTLINE_OUTPUT_RESERVE,
};
use crate::features::learning::recall::deck_output_tokens;
use lattice_core::application::ports::llm_port::ModelRequirements;
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::{AppError, Result};

/// The smallest window in which the planner reserves `output_tokens` for the
/// answer in full.
fn window_reserving(output_tokens: usize) -> usize {
    output_tokens.saturating_mul(2)
}

fn reserving(output_tokens: usize) -> ModelRequirements {
    ModelRequirements {
        min_context_tokens: Some(window_reserving(output_tokens)),
        ..ModelRequirements::default()
    }
}

/// Writing a lesson: its answer is the whole lesson.
pub fn lesson_preparation() -> ModelRequirements {
    reserving(LESSON_OUTPUT_RESERVE)
}

/// Drafting or repairing a course outline.
pub fn outline() -> ModelRequirements {
    reserving(OUTLINE_OUTPUT_RESERVE)
}

/// A study deck of `card_count` cards, written in one answer.
pub fn study_deck(card_count: usize) -> ModelRequirements {
    reserving(deck_output_tokens(card_count))
}

/// A study deck from a conversation's verified claims: its claims are
/// counted only once they are read, and each batch of them is checked
/// against the window before it is sent.
pub fn conversation_study_deck() -> ModelRequirements {
    ModelRequirements::default()
}

/// `count` recall drafts for a lesson, written in one answer.
pub fn recall_drafts(count: usize) -> ModelRequirements {
    reserving(recall_output_tokens(count))
}

/// Practice sets and their grading: each call sizes its answer to fit.
pub fn practice() -> ModelRequirements {
    ModelRequirements::default()
}

/// Assessment authoring and grading: each call sizes its answer to fit.
pub fn assessment() -> ModelRequirements {
    ModelRequirements::default()
}

/// Diagnostic probes and their scoring: each call sizes its answer to fit.
pub fn diagnostics() -> ModelRequirements {
    ModelRequirements::default()
}

/// Refuses a loaded Learning model whose window is below what `work` needs,
/// naming the model, its window, the window needed and the fix. `work` reads
/// as the subject of a sentence: "Preparing 'Transformers from scratch'".
pub fn ensure_window(
    llm: &dyn LLMPort,
    requirements: &ModelRequirements,
    work: &str,
) -> Result<()> {
    let Some(needed) = requirements.min_context_tokens else {
        return Ok(());
    };
    let window = llm.max_context_tokens();
    if window >= needed {
        return Ok(());
    }
    Err(AppError::InvalidConfig(format!(
        "{work} needs a context window of at least {needed} tokens. Your Learning model, {}, has a {window}-token window. Set its context length in Settings → Models to Auto or to {needed} or more, or choose a Learning model with a larger window",
        llm.model_name()
    )))
}

#[cfg(test)]
mod tests;
