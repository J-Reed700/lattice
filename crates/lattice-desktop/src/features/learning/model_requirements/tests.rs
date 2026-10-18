use super::*;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::application::services::context_assembler::{
    BudgetAllocation, BudgetRequest, ModelCapacity,
};

/// A model that is only a window: the checks make no model call.
struct Window(usize);

#[async_trait::async_trait]
impl LLMPort for Window {
    async fn complete(&self, _request: &CompletionRequest) -> Result<CompletionResponse> {
        unreachable!("the window check makes no model call")
    }
    fn model_name(&self) -> &str {
        "Ornith-1.5-9B"
    }
    fn max_context_tokens(&self) -> usize {
        self.0
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// What the budget planner reserves for an answer of `output_tokens` in a
/// window of `window` tokens.
fn reserved(window: usize, output_tokens: usize) -> usize {
    let capacity = ModelCapacity::new("model", window).with_output_limit(output_tokens);
    BudgetAllocation::plan(&capacity, &BudgetRequest::new(0, 0, 0))
        .unwrap()
        .output_reserved
}

#[test]
fn a_floor_is_the_smallest_window_the_planner_gives_the_whole_answer() {
    for (requirements, output_tokens) in [
        (lesson_preparation(), LESSON_OUTPUT_RESERVE),
        (outline(), OUTLINE_OUTPUT_RESERVE),
        (study_deck(12), deck_output_tokens(12)),
        (recall_drafts(4), recall_output_tokens(4)),
    ] {
        let floor = requirements.min_context_tokens.unwrap();
        assert_eq!(reserved(floor, output_tokens), output_tokens);
        assert!(reserved(floor - 1, output_tokens) < output_tokens);
    }
}

#[test]
fn a_study_deck_needs_more_room_the_more_cards_it_asks_for() {
    let floor = |cards| study_deck(cards).min_context_tokens.unwrap();
    assert!(floor(20) > floor(10));
}

#[test]
fn a_window_below_the_floor_is_refused_with_the_numbers_and_the_fix() {
    let requirements = lesson_preparation();
    let needed = requirements.min_context_tokens.unwrap();

    let error = ensure_window(
        &Window(8_704),
        &requirements,
        "Preparing 'Transformers from scratch'",
    )
    .unwrap_err();

    let AppError::InvalidConfig(message) = &error else {
        panic!("a window is a setting, so this is a configuration error: {error:?}");
    };
    assert_eq!(
        message,
        &format!(
            "Preparing 'Transformers from scratch' needs a context window of at least {needed} tokens. Your Learning model, Ornith-1.5-9B, has a 8704-token window. Set its context length in Settings → Models to Auto or to {needed} or more, or choose a Learning model with a larger window"
        )
    );
    // The error's display adds its own ". Please check your settings."
    assert!(!message.ends_with('.'));
}

#[test]
fn a_window_at_the_floor_passes() {
    let requirements = lesson_preparation();
    let floor = requirements.min_context_tokens.unwrap();
    ensure_window(&Window(floor), &requirements, "Preparing a lesson").unwrap();
}

#[test]
fn work_that_sizes_its_answer_to_fit_is_never_refused_up_front() {
    for requirements in [
        practice(),
        assessment(),
        diagnostics(),
        conversation_study_deck(),
    ] {
        assert_eq!(requirements, ModelRequirements::default());
        ensure_window(&Window(512), &requirements, "Practice").unwrap();
    }
}
