//! The turn's share of the model's window, planned once by the context
//! assembler before retrieval decides how much to read.
use std::sync::Arc;

use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::context_assembler::{
    render, BudgetAllocation, BudgetRequest, ContextAssembler, EvidenceBudget, HistoryCharge,
    ModelCapacity, TokenCounter,
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
    /// What the reader's pictures may cost: the request's room once the
    /// system prompt, the tools, the message and any history sent whole are
    /// paid. Pictures over it are shrunk, then dropped (chat-vision §8).
    pub(super) picture_room: usize,
}

impl TurnBudget {
    /// Plan against the message, the system prompt, the tools and the history.
    ///
    /// `message` is what the evidence will travel in: the question inside the
    /// turn's template, with no evidence in it. `tool_schema_tokens` is what
    /// the tools a grounded turn is offered cost. Both are paid before any
    /// evidence is, since evidence that leaves no room for them is evidence
    /// the request cannot carry.
    ///
    /// With bounded memory on, the fixed parts are priced exactly as the
    /// context assembler prices them, memory rules and date line included, and
    /// the memory plan carries the history and spends at most its own pools on
    /// it, so evidence is charged only what the history actually needs up to
    /// those pools. With it off, the string history is sent whole and charged
    /// whole.
    ///
    /// `picture_tokens` is what the reader's attached pictures cost at the
    /// size the model's geometry takes them: their own material, paid with
    /// the message before evidence, and never more than the room left for
    /// them, since pictures over it are shrunk or dropped before the request
    /// is built.
    ///
    /// A question the window cannot hold is not refused here: the request is
    /// checked whole when it is assembled, after the user's message is saved,
    /// so the refusal leaves a retryable message. Until then the turn simply
    /// has no room for evidence.
    pub(super) fn plan(
        llm: &Arc<dyn LLMPort>,
        system_prompt: &str,
        message: &str,
        context: &[String],
        bounded_memory: bool,
        tool_schema_tokens: usize,
        picture_tokens: usize,
        today: chrono::NaiveDate,
    ) -> Self {
        let (system_entries, history_entries): (Vec<&String>, Vec<&String>) = context
            .iter()
            .partition(|entry| entry.starts_with("System:"));
        let history: usize = history_entries
            .iter()
            .map(|entry| llm.count_tokens(entry))
            .sum();
        let mut request = if bounded_memory {
            let counter: TokenCounter = {
                let llm = Arc::clone(llm);
                Arc::new(move |text: &str| llm.count_tokens(text))
            };
            ContextAssembler::new(counter)
                .fixed_parts(system_prompt, message, tool_schema_tokens, today)
                .with_history(HistoryCharge::Measured(history))
        } else {
            // The string path sends the date line ahead of the message.
            let current_input = format!("{}\n\n{message}", render::render_date_line(today));
            BudgetRequest::new(
                system_entries
                    .iter()
                    .map(|entry| llm.count_tokens(entry))
                    .sum(),
                tool_schema_tokens,
                llm.count_tokens(&current_input),
            )
            .with_history(HistoryCharge::Carried(history))
        };
        let capacity = ModelCapacity::new(llm.model_name(), llm.max_context_tokens());
        // History sent whole is as fixed as the message; history the memory
        // plan selects gives way to the pictures instead.
        let carried_history = match request.history {
            HistoryCharge::Carried(tokens) => tokens,
            HistoryCharge::Reserved | HistoryCharge::Measured(_) => 0,
        };
        let text_fixed = request.system + request.tools + request.current_input + carried_history;
        let picture_room = |input_budget: usize| input_budget.saturating_sub(text_fixed);
        if picture_tokens > 0 {
            let room = match BudgetAllocation::plan(&capacity, &BudgetRequest::default()) {
                Ok(allocation) => picture_room(allocation.input_budget),
                Err(_) => 0,
            };
            request.current_input += picture_tokens.min(room);
        }
        match BudgetAllocation::plan(&capacity, &request) {
            Ok(allocation) => Self {
                evidence: allocation.evidence_budget(),
                output_tokens: allocation.output_reserved,
                input_budget: allocation.input_budget,
                picture_room: picture_room(allocation.input_budget),
            },
            Err(error) => {
                warn!(%error, "No room for evidence this turn");
                match BudgetAllocation::plan(&capacity, &BudgetRequest::default()) {
                    Ok(allocation) => Self {
                        evidence: EvidenceBudget::default(),
                        output_tokens: allocation.output_reserved,
                        input_budget: allocation.input_budget,
                        picture_room: picture_room(allocation.input_budget),
                    },
                    // A model too small to plan for: the assembled request
                    // reports it, and nothing here caps the provider.
                    Err(_) => Self {
                        evidence: EvidenceBudget::default(),
                        output_tokens: 0,
                        input_budget: llm.max_context_tokens(),
                        picture_room: 0,
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
    use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
    use lattice_core::shared::error::Result;

    fn mock() -> Arc<dyn LLMPort> {
        Arc::new(MockLLMPort::new())
    }

    fn today() -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 13).expect("a real date")
    }

    /// A model with a window of the given size that is never asked anything.
    struct Window(usize);

    #[async_trait::async_trait]
    impl LLMPort for Window {
        async fn complete(&self, _request: &CompletionRequest) -> Result<CompletionResponse> {
            Ok(CompletionResponse::from_text(String::new()))
        }
        fn model_name(&self) -> &str {
            "window"
        }
        fn max_context_tokens(&self) -> usize {
            self.0
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }

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

    fn plan(llm: &Arc<dyn LLMPort>, context: &[String], bounded: bool, tools: usize) -> TurnBudget {
        TurnBudget::plan(
            llm,
            "You are helpful.",
            "question",
            context,
            bounded,
            tools,
            0,
            today(),
        )
    }

    #[test]
    fn bounded_memory_leaves_retrieval_room_in_a_chat_that_fills_the_window() {
        let llm = mock();
        let context = long_history(llm.as_ref());

        let bounded = plan(&llm, &context, true, 0);
        let carried = plan(&llm, &context, false, 0);

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
        let llm = mock();
        let empty = plan(&llm, &[], true, 0);
        let short = plan(
            &llm,
            &["User: hello".to_string(), "Assistant: hi".to_string()],
            true,
            0,
        );

        let history = llm.count_tokens("User: hello") + llm.count_tokens("Assistant: hi");
        assert_eq!(empty.evidence.total() - short.evidence.total(), history);
        assert!(empty.output_tokens > 0);
    }

    #[test]
    fn a_question_larger_than_the_window_leaves_no_evidence_but_keeps_the_reservation() {
        let llm = mock();
        let budget = TurnBudget::plan(&llm, "", &"word ".repeat(50_000), &[], true, 0, 0, today());
        assert_eq!(budget.evidence.total(), 0);
        assert!(budget.output_tokens > 0);
        assert!(budget.input_budget < llm.max_context_tokens());
    }

    /// The 2026-10-13 shipped-path run at `--ctx-size 8192`: the tools and the
    /// policy came to about 2,100 tokens, the evidence pool was sized as if
    /// they cost nothing, and 31 of 34 turns were refused before generation.
    #[test]
    fn at_8k_the_evidence_pool_leaves_room_for_the_tools_and_the_policy() {
        let llm: Arc<dyn LLMPort> = Arc::new(Window(8_192));
        let system = "You answer from the user's documents. ".repeat(40);
        let tools = 1_400;
        let budget = TurnBudget::plan(&llm, &system, "question", &[], true, tools, 0, today());
        let unreserved = TurnBudget::plan(&llm, &system, "question", &[], true, 0, 0, today());

        // The tool schemas come out of the evidence, token for token.
        assert_eq!(unreserved.evidence.total() - budget.evidence.total(), tools);
        // Every fixed part the assembler will charge, priced as it prices
        // them, plus all the evidence, fits what the request may occupy.
        let counter: TokenCounter = {
            let llm = Arc::clone(&llm);
            Arc::new(move |text: &str| llm.count_tokens(text))
        };
        let fixed = ContextAssembler::new(counter).fixed_parts(&system, "question", tools, today());
        assert_eq!(budget.input_budget, 5_734, "70% of 8,192");
        assert!(
            fixed.system > llm.count_tokens(&system),
            "the memory rules are charged"
        );
        assert_eq!(
            fixed.system + fixed.tools + fixed.current_input + budget.evidence.total(),
            budget.input_budget
        );
    }

    #[test]
    fn the_string_path_also_pays_for_its_tools_before_evidence() {
        let llm: Arc<dyn LLMPort> = Arc::new(Window(8_192));
        let without = plan(&llm, &[], false, 0);
        let with = plan(&llm, &[], false, 1_000);
        assert_eq!(without.evidence.total() - with.evidence.total(), 1_000);
    }

    /// Attached pictures are paid with the message, before evidence, and
    /// never more than the room the fixed parts leave them.
    #[test]
    fn pictures_are_paid_before_evidence_and_bounded_by_their_room() {
        let llm: Arc<dyn LLMPort> = Arc::new(Window(8_192));
        let plain = TurnBudget::plan(&llm, "", "question", &[], true, 0, 0, today());
        let pictured = TurnBudget::plan(&llm, "", "question", &[], true, 0, 1_000, today());
        assert_eq!(plain.evidence.total() - pictured.evidence.total(), 1_000);
        assert_eq!(plain.picture_room, pictured.picture_room);

        let flooded = TurnBudget::plan(&llm, "", "question", &[], true, 0, 1_000_000, today());
        assert_eq!(flooded.evidence.total(), 0);
        assert_eq!(flooded.picture_room, plain.picture_room);
        assert!(plain.picture_room > 0 && plain.picture_room < plain.input_budget);

        // History sent whole is fixed; history the memory plan selects gives
        // way to the pictures.
        let history = vec!["User: ".to_string() + &"word ".repeat(500)];
        let without = TurnBudget::plan(&llm, "", "question", &[], false, 0, 0, today());
        let carried = TurnBudget::plan(&llm, "", "question", &history, false, 0, 0, today());
        assert_eq!(
            without.picture_room - carried.picture_room,
            llm.count_tokens(&history[0])
        );
        let selected = TurnBudget::plan(&llm, "", "question", &history, true, 0, 0, today());
        assert_eq!(selected.picture_room, plain.picture_room);
    }
}
