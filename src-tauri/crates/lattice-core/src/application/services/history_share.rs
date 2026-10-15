//! Conversation history for an auxiliary model call (turn intent, query
//! planning, tutoring), sized from that call's own model window.
//!
//! These calls read history to resolve a follow-up, not to answer from it, so
//! they take a share of their window rather than all of it: the rest is their
//! instructions and material, and a small local model must still answer them
//! quickly. Turns go in whole, newest first, and what does not fit is
//! counted, never cut mid-message.

use crate::application::ports::LLMPort;
use crate::application::services::context_assembler::{
    BudgetAllocation, BudgetRequest, ModelCapacity,
};

/// Share of an auxiliary call's input budget, after its fixed prompt, that
/// history may take.
const HISTORY_SHARE: f64 = 0.25;

/// Tokens of history an auxiliary call on `llm` may carry once `fixed` (its
/// instructions, message and material) is paid for. Zero when the window
/// cannot hold the fixed prompt at all.
pub fn auxiliary_history_room(llm: &dyn LLMPort, fixed: &str) -> usize {
    let capacity = ModelCapacity::new(llm.model_name(), llm.max_context_tokens());
    BudgetAllocation::plan(
        &capacity,
        &BudgetRequest::new(llm.count_tokens(fixed), 0, 0),
    )
    .map_or(0, |allocation| {
        (allocation.available as f64 * HISTORY_SHARE) as usize
    })
}

/// The newest of `turns` (oldest first) that fit `room` tokens together, and
/// how many older turns were left out. The walk stops at the first turn that
/// does not fit, so what is kept is the unbroken recent stretch a follow-up
/// refers to.
pub fn newest_that_fit<T>(turns: &[T], room: usize, cost: impl Fn(&T) -> usize) -> (&[T], usize) {
    let mut used = 0usize;
    let mut kept = 0usize;
    for turn in turns.iter().rev() {
        let next = used.saturating_add(cost(turn));
        if next > room {
            break;
        }
        used = next;
        kept += 1;
    }
    let omitted = turns.len() - kept;
    (turns.get(omitted..).unwrap_or_default(), omitted)
}

/// The line a history view carries when it leaves earlier turns out.
pub fn omitted_note(omitted: usize) -> Option<String> {
    (omitted > 0).then(|| {
        format!(
            "({omitted} earlier message{} not shown)",
            if omitted == 1 { "" } else { "s" }
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_unbroken_stretch_that_fits_is_kept_and_the_rest_counted() {
        let turns = [
            "a".repeat(10),
            "b".repeat(50),
            "c".repeat(10),
            "d".repeat(10),
        ];
        let (kept, omitted) = newest_that_fit(&turns, 25, |turn| turn.len());
        assert_eq!(kept, &turns[2..]);
        assert_eq!(omitted, 2);
        let (kept, omitted) = newest_that_fit(&turns, 1_000, |turn| turn.len());
        assert_eq!(kept.len(), 4);
        assert_eq!(omitted, 0);
        assert_eq!(omitted_note(0), None);
        assert_eq!(
            omitted_note(2).as_deref(),
            Some("(2 earlier messages not shown)")
        );
    }
}
