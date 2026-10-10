//! The turn's share of the model's window, planned once by the context
//! assembler before retrieval decides how much to read.
use crate::application::ports::LLMPort;
use crate::application::services::context_assembler::{
    BudgetAllocation, BudgetRequest, EvidenceBudget, HistoryCharge, ModelCapacity,
};
use tracing::warn;

/// What the turn may spend, from one [`BudgetAllocation`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TurnBudget {
    /// Room for everything the turn reads: attachments, the folder, recalled
    /// evidence, web pages and library passages, each charged as it is taken.
    pub(super) evidence: EvidenceBudget,
    /// The generation reservation. The request enforces it, so the answer
    /// cannot overrun the window the prompt was measured against.
    pub(super) output_tokens: usize,
    /// What the whole request may occupy, tool rounds included.
    pub(super) input_budget: usize,
}

impl TurnBudget {
    /// Plan against the question, the system prompt and the history.
    ///
    /// With bounded memory on, the memory plan carries the history and spends
    /// at most its own pools on it, so evidence is charged only what the
    /// history actually needs up to those pools. With it off, the string
    /// history is sent whole and charged whole.
    ///
    /// A question the window cannot hold is not refused here: the request is
    /// checked whole when it is assembled, after the user's message is saved,
    /// so the refusal leaves a retryable message. Until then the turn simply
    /// has no room for evidence.
    pub(super) fn plan(
        llm: &dyn LLMPort,
        system_prompt: &str,
        question: &str,
        context: &[String],
        bounded_memory: bool,
    ) -> Self {
        let (system_entries, history_entries): (Vec<&String>, Vec<&String>) = context
            .iter()
            .partition(|entry| entry.starts_with("System:"));
        let history: usize = history_entries
            .iter()
            .map(|entry| llm.count_tokens(entry))
            .sum();
        let (system, charge) = if bounded_memory {
            (
                llm.count_tokens(system_prompt),
                HistoryCharge::Measured(history),
            )
        } else {
            (
                system_entries
                    .iter()
                    .map(|entry| llm.count_tokens(entry))
                    .sum(),
                HistoryCharge::Carried(history),
            )
        };
        let capacity = ModelCapacity::new(llm.model_name(), llm.max_context_tokens());
        let request =
            BudgetRequest::new(system, 0, llm.count_tokens(question)).with_history(charge);
        match BudgetAllocation::plan(&capacity, &request) {
            Ok(allocation) => Self {
                evidence: allocation.evidence_budget(),
                output_tokens: allocation.output_reserved,
                input_budget: allocation.input_budget,
            },
            Err(error) => {
                warn!(%error, "No room for evidence this turn");
                match BudgetAllocation::plan(&capacity, &BudgetRequest::default()) {
                    Ok(allocation) => Self {
                        evidence: EvidenceBudget::default(),
                        output_tokens: allocation.output_reserved,
                        input_budget: allocation.input_budget,
                    },
                    // A model too small to plan for: the assembled request
                    // reports it, and nothing here caps the provider.
                    Err(_) => Self {
                        evidence: EvidenceBudget::default(),
                        output_tokens: 0,
                        input_budget: llm.max_context_tokens(),
                    },
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::llm::engine::factory::MockLLMPort;

    /// A long chat whose string history fills the window.
    fn long_history(llm: &dyn LLMPort) -> Vec<String> {
        let mut context = vec!["System: You are helpful.".to_string()];
        while context
            .iter()
            .map(|entry| llm.count_tokens(entry))
            .sum::<usize>()
            < llm.max_context_tokens()
        {
            context.push(format!("User: {}", "earlier words ".repeat(40)));
        }
        context
    }

    #[test]
    fn bounded_memory_leaves_retrieval_room_in_a_chat_that_fills_the_window() {
        let llm = MockLLMPort::new();
        let context = long_history(&llm);

        let bounded = TurnBudget::plan(&llm, "You are helpful.", "question", &context, true);
        let carried = TurnBudget::plan(&llm, "You are helpful.", "question", &context, false);

        assert!(bounded.evidence.total() > 0, "{bounded:?}");
        assert_eq!(
            carried.evidence.total(),
            0,
            "the string total leaves nothing"
        );
        // Retrieval sized this way still fits the plan's own input budget.
        assert!(
            llm.count_tokens("You are helpful.")
                + llm.count_tokens("question")
                + bounded.evidence.total()
                <= bounded.input_budget
        );
    }

    #[test]
    fn a_short_chat_gives_evidence_what_its_history_does_not_use() {
        let llm = MockLLMPort::new();
        let empty = TurnBudget::plan(&llm, "You are helpful.", "question", &[], true);
        let short = TurnBudget::plan(
            &llm,
            "You are helpful.",
            "question",
            &["User: hello".to_string(), "Assistant: hi".to_string()],
            true,
        );

        let history = llm.count_tokens("User: hello") + llm.count_tokens("Assistant: hi");
        assert_eq!(empty.evidence.total() - short.evidence.total(), history);
        assert!(empty.output_tokens > 0);
    }

    #[test]
    fn a_question_larger_than_the_window_leaves_no_evidence_but_keeps_the_reservation() {
        let llm = MockLLMPort::new();
        let budget = TurnBudget::plan(&llm, "", &"word ".repeat(50_000), &[], true);
        assert_eq!(budget.evidence.total(), 0);
        assert!(budget.output_tokens > 0);
        assert!(budget.input_budget < llm.max_context_tokens());
    }
}
