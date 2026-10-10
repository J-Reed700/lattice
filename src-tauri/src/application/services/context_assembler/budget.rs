//! The one place prompt capacity is divided up.
//!
//! Design: `docs/design/2026-09-19-conversation-memory.md` §10.1–10.2.
//!
//! Pure arithmetic, no I/O, so the allocation rules can be tested against a
//! matrix of model sizes rather than inferred from a running chat.
//!
//! The rule that matters: these are *allocations of one budget*, not extra
//! allowances handed out per pool. Their sum never exceeds `available`, and
//! nothing — document RAG, tool output, recalled passages — may be funded by
//! quietly taking room from a recorded active constraint.

use crate::shared::error::AppError;

/// How the token counts in a plan were obtained.
///
/// Reported rather than assumed: `LLMPort::count_tokens` is allowed to
/// approximate, and a margin chosen against an estimate is not a guarantee.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenAccounting {
    /// Counted with the provider's own tokenizer.
    Exact,
    /// Counted with a heuristic. Overflow remains possible; the safety margin
    /// reduces its likelihood and does not remove it.
    Estimated,
}

impl TokenAccounting {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Estimated => "estimated",
        }
    }
}

/// The continuation model's capacity, as the assembler sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCapacity {
    /// Identity of the model this budget was computed for. A model switch
    /// invalidates the plan; it is recorded so that is detectable.
    pub model_identity: String,
    /// Smaller of configured and provider-advertised context window.
    pub context_tokens: usize,
    /// Explicit generation limit, when the caller has one.
    pub configured_output_limit: Option<usize>,
    pub accounting: TokenAccounting,
}

impl ModelCapacity {
    pub fn new(model_identity: impl Into<String>, context_tokens: usize) -> Self {
        Self {
            model_identity: model_identity.into(),
            context_tokens,
            configured_output_limit: None,
            accounting: TokenAccounting::Estimated,
        }
    }

    pub fn with_output_limit(mut self, limit: usize) -> Self {
        self.configured_output_limit = Some(limit);
        self
    }

    pub fn with_accounting(mut self, accounting: TokenAccounting) -> Self {
        self.accounting = accounting;
        self
    }
}

/// Smallest capacity worth attempting. Below this, the fixed policy and one
/// user message cannot coexist with any history at all, and an actionable error
/// beats a prompt that is silently nothing but the system prompt.
pub const MIN_USABLE_CAPACITY: usize = 1024;

/// Upper bounds for each pool, as fractions of `available` (§10.2).
const MANDATORY_CAP: usize = 4096;
const MANDATORY_FRACTION: f64 = 0.25;
const SUMMARY_CAP: usize = 1536;
const SUMMARY_FRACTION: f64 = 0.10;
const RECALL_CAP: usize = 2048;
const RECALL_FRACTION: f64 = 0.15;
const RECENT_FRACTION: f64 = 0.35;
/// Ceiling on mandatory memory after borrowing unused optional allocation.
const MANDATORY_BORROW_CAP: usize = 8192;
const MANDATORY_BORROW_FRACTION: f64 = 0.50;

/// Ceiling on the default generation reservation. A reasoning model spends
/// much of its answer budget thinking before it writes a word, so this is sized
/// for a long written answer plus its reasoning, not for the answer alone.
const OUTPUT_RESERVATION_CAP: usize = 32_768;

/// Default generation reservation when the caller configures none.
///
/// This reservation becomes the request's `max_tokens`. It was once capped at
/// 4096, which a reasoning model on a 128K window exhausted mid-answer — the
/// server stopped with `length` and the turn failed while most of the window
/// sat unused.
fn default_output_reservation(capacity: usize) -> usize {
    (capacity / 4).min(OUTPUT_RESERVATION_CAP)
}

/// Safety margin against tokenizer imprecision and provider framing.
fn safety_margin(capacity: usize) -> usize {
    let proportional = (capacity as f64 * 0.05).ceil() as usize;
    proportional.max(256)
}

/// What one request must carry whole, and how its history is paid for.
///
/// The system policy, the tool schemas and the current input are never cut:
/// a request whose fixed parts do not fit is refused with
/// [`PromptBudgetExceeded`] rather than sent clipped.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BudgetRequest {
    pub system: usize,
    pub tools: usize,
    pub current_input: usize,
    pub history: HistoryCharge,
}

impl BudgetRequest {
    pub fn new(system: usize, tools: usize, current_input: usize) -> Self {
        Self {
            system,
            tools,
            current_input,
            history: HistoryCharge::Reserved,
        }
    }

    pub fn with_history(mut self, history: HistoryCharge) -> Self {
        self.history = history;
        self
    }

    fn fixed(&self) -> usize {
        self.system
            .saturating_add(self.tools)
            .saturating_add(self.current_input)
    }
}

/// How a request's conversation history is charged against its budget.
///
/// Evidence is planned before the final request exists, so the planner has to
/// know whether history will claim its whole allocation or only what it needs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HistoryCharge {
    /// The assembler selects history itself out of the memory pools, so all
    /// of them are held back.
    #[default]
    Reserved,
    /// History comes out of the memory pools and would cost this much carried
    /// raw. Evidence may use the part of those pools it does not need.
    Measured(usize),
    /// No memory ledger: the caller selected this much history and sends it
    /// whole, so the memory pools are empty and evidence gets the rest.
    Carried(usize),
}

/// How one assembled prompt's capacity is divided.
///
/// Every field is a token count against the same model. `input_budget` is what
/// the request may occupy; `available` is what is left for selectable content
/// once the unavoidable parts are paid for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetAllocation {
    pub capacity: usize,
    pub output_reserved: usize,
    pub safety_margin: usize,
    pub input_budget: usize,
    /// System policy + tool schemas + current input + message framing.
    pub fixed: usize,
    pub system: usize,
    pub tools: usize,
    pub current_input: usize,
    pub available: usize,
    pub mandatory_memory: usize,
    pub summary: usize,
    pub recall: usize,
    pub recent_history: usize,
    /// What the request's history is charged, per its [`HistoryCharge`].
    pub history_charged: usize,
    /// Room for document, web and tool evidence once history is charged.
    /// `history_charged + evidence` never exceeds `available`.
    pub evidence: usize,
    /// Hard ceiling on mandatory memory after it borrows unused optional room.
    pub mandatory_borrow_ceiling: usize,
    pub accounting: TokenAccounting,
}

impl BudgetAllocation {
    /// Divide `capacity` after paying the request's fixed parts.
    ///
    /// # Errors
    ///
    /// - [`BudgetError::ModelTooSmall`] / [`BudgetError::NoInputRoom`] when
    ///   the model cannot hold a prompt at all, so the caller can say which
    ///   model to change rather than emitting a request certain to be rejected.
    /// - [`BudgetError::Overflow`] when the unavoidable parts alone exceed the
    ///   input budget. That is a real, reachable state — a very long system
    ///   prompt, or a pasted user message bigger than the window — and it must
    ///   surface as an error rather than an implicitly clipped instruction.
    pub fn plan(
        capacity: &ModelCapacity,
        request: &BudgetRequest,
    ) -> std::result::Result<Self, BudgetError> {
        let total = capacity.context_tokens;
        if total < MIN_USABLE_CAPACITY {
            return Err(BudgetError::ModelTooSmall {
                model: capacity.model_identity.clone(),
                context_tokens: total,
            });
        }

        let output_reserved = capacity
            .configured_output_limit
            .unwrap_or_else(|| default_output_reservation(total))
            // A configured limit larger than the window itself would leave no
            // input at all; clamp it rather than underflowing below.
            .min(total / 2);
        let margin = safety_margin(total);
        // Checked throughout: these are usize, and the whole point of the guard
        // above is that a wrapped subtraction here would produce an enormous
        // "budget" and a prompt the provider rejects.
        let input_budget = total
            .checked_sub(output_reserved)
            .and_then(|value| value.checked_sub(margin))
            .ok_or_else(|| BudgetError::NoInputRoom {
                model: capacity.model_identity.clone(),
                output_reserved,
                margin,
                context_tokens: total,
            })?;

        let fixed = request.fixed();
        if fixed > input_budget {
            return Err(PromptBudgetExceeded {
                required: fixed,
                available: input_budget,
            }
            .into());
        }
        let available = input_budget - fixed;

        let fraction = |value: f64, cap: usize| ((available as f64 * value) as usize).min(cap);
        let (mandatory_memory, summary, recall, recent_history) = match request.history {
            // Without a ledger there is nothing to put in the memory pools.
            HistoryCharge::Carried(_) => (0, 0, 0, 0),
            HistoryCharge::Reserved | HistoryCharge::Measured(_) => (
                fraction(MANDATORY_FRACTION, MANDATORY_CAP),
                fraction(SUMMARY_FRACTION, SUMMARY_CAP),
                fraction(RECALL_FRACTION, RECALL_CAP),
                (available as f64 * RECENT_FRACTION) as usize,
            ),
        };
        // Saturating because the four pools are each capped and cannot exceed
        // `available` together, but a future tuning change must not be able to
        // wrap this.
        let history_pools = mandatory_memory
            .saturating_add(summary)
            .saturating_add(recall)
            .saturating_add(recent_history)
            .min(available);
        let history_charged = match request.history {
            HistoryCharge::Reserved => history_pools,
            HistoryCharge::Measured(measured) => measured.min(history_pools),
            HistoryCharge::Carried(carried) => carried.min(available),
        };
        let evidence = available - history_charged;

        let mandatory_borrow_ceiling = ((available as f64 * MANDATORY_BORROW_FRACTION) as usize)
            .min(MANDATORY_BORROW_CAP)
            .max(mandatory_memory);

        let allocation = Self {
            capacity: total,
            output_reserved,
            safety_margin: margin,
            input_budget,
            fixed,
            system: request.system,
            tools: request.tools,
            current_input: request.current_input,
            available,
            mandatory_memory,
            summary,
            recall,
            recent_history,
            history_charged,
            evidence,
            mandatory_borrow_ceiling,
            accounting: capacity.accounting,
        };
        debug_assert!(
            allocation.history_charged + allocation.evidence <= available,
            "history and evidence are a division of `available`, not additions to it"
        );
        Ok(allocation)
    }

    /// Sum of the selectable pools when history holds its whole allocation.
    /// Never more than [`Self::available`].
    pub fn pools_total(&self) -> usize {
        self.history_charged + self.evidence
    }

    /// The evidence pool as a budget the caller fills piece by piece.
    pub fn evidence_budget(&self) -> EvidenceBudget {
        EvidenceBudget::new(self.evidence)
    }

    /// Room mandatory memory may claim beyond its own pool by borrowing the
    /// optional allocations, while leaving the newest complete turn funded.
    ///
    /// `newest_turn_tokens` is reserved out of the borrowable room rather than
    /// out of the mandatory pool: a prompt with every constraint and no idea
    /// what was just asked is not a usable prompt either.
    pub fn mandatory_ceiling_with_turn_reserved(&self, newest_turn_tokens: usize) -> usize {
        let borrowable = self
            .available
            .saturating_sub(newest_turn_tokens.min(self.available));
        self.mandatory_borrow_ceiling.min(borrowable)
    }
}

/// One kind of evidence's claim on what is left of the evidence pool.
///
/// Shares are of the room *remaining* when the claim is made, so material
/// claimed earlier is never counted twice, and none can take more than is left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EvidenceShare {
    fraction: f64,
    /// Kept while the remaining room can still hold it.
    floor: usize,
    cap: usize,
}

impl EvidenceShare {
    /// Files attached to the message. They are the reader's own material and
    /// outrank anything a search turns up, but a turn that attaches a book
    /// must still have room to search, cite and answer. What they do not use
    /// goes back to retrieval, so a small attachment costs a small amount.
    pub const ATTACHMENTS: Self = Self {
        fraction: 0.6,
        floor: 0,
        cap: usize::MAX,
    };
    /// The folder an Explorer conversation reads beside the chat: enough to
    /// show the open file on a small local model without crowding out the
    /// question and its history.
    pub const FOLDER: Self = Self {
        fraction: 0.25,
        floor: 500,
        cap: 8_000,
    };
    /// Passages earlier turns cited, recalled for this one.
    pub const PRIOR_EVIDENCE: Self = Self {
        fraction: 0.25,
        floor: 0,
        cap: 4_000,
    };
    /// Fetched web pages. The rest stays with the user's own documents, which
    /// the prompt ranks first.
    pub const WEB_PAGES: Self = Self {
        fraction: 0.4,
        floor: 0,
        cap: usize::MAX,
    };
}

/// The evidence pool, filled piece by piece in the order a request claims it.
///
/// Every claim is charged, so what one kind of evidence takes is not there for
/// the next, and the total can never exceed the pool the allocation set aside.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EvidenceBudget {
    total: usize,
    spent: usize,
}

impl EvidenceBudget {
    pub fn new(total: usize) -> Self {
        Self { total, spent: 0 }
    }

    pub fn total(&self) -> usize {
        self.total
    }

    pub fn remaining(&self) -> usize {
        self.total.saturating_sub(self.spent)
    }

    /// What `share` may take of the room that is left. Not charged: the caller
    /// charges what its material actually measured.
    pub fn allowance(&self, share: EvidenceShare) -> usize {
        let remaining = self.remaining();
        let proportional = ((remaining as f64 * share.fraction) as usize).min(share.cap);
        proportional.max(share.floor).min(remaining)
    }

    /// Record `tokens` of evidence as committed to the request.
    pub fn charge(&mut self, tokens: usize) {
        self.spent = self.spent.saturating_add(tokens);
    }

    /// The items that fit, best first: each is kept when its cost fits what is
    /// left and skipped otherwise, so one long item cannot shut out shorter
    /// ones ranked below it. What is kept is charged.
    pub fn select<'a, T>(&mut self, items: &'a [T], cost: impl Fn(&T) -> usize) -> Vec<&'a T> {
        items
            .iter()
            .filter(|item| {
                let tokens = cost(item);
                if tokens > self.remaining() {
                    return false;
                }
                self.charge(tokens);
                true
            })
            .collect()
    }
}

/// Raised when a request's fixed parts — policy, tools, current input, and any
/// material its caller requires whole — do not fit the input budget.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "The system policy, tools and current message need {required} tokens, above the {available} \
     this request may occupy. Shorten the message or choose a larger-context model."
)]
pub struct PromptBudgetExceeded {
    pub required: usize,
    pub available: usize,
}

/// Why a request could not be planned.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BudgetError {
    #[error(
        "Model {model} advertises a {context_tokens}-token context, below the \
         {MIN_USABLE_CAPACITY} needed to assemble a prompt. Choose a larger-context model."
    )]
    ModelTooSmall {
        model: String,
        context_tokens: usize,
    },
    #[error(
        "Model {model} cannot reserve {output_reserved} output tokens and a {margin}-token \
         margin inside a {context_tokens}-token context."
    )]
    NoInputRoom {
        model: String,
        output_reserved: usize,
        margin: usize,
        context_tokens: usize,
    },
    #[error(transparent)]
    Overflow(#[from] PromptBudgetExceeded),
}

impl From<BudgetError> for AppError {
    fn from(error: BudgetError) -> Self {
        match error {
            BudgetError::Overflow(overflow) => AppError::InvalidInput(overflow.to_string()),
            other => AppError::InvalidConfig(other.to_string()),
        }
    }
}

/// Raised when the genuinely active constraints do not fit any allocation.
///
/// A real finite-context limit, surfaced rather than resolved: the alternatives
/// are dropping the oldest restriction, truncating a negation, or summarizing
/// exact evidence into a sentence nobody can check, and all three are worse
/// than telling the user.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "This conversation's {count} active requirements need {required} tokens, above the {available} \
     available. Review the active requirements and resolve the obsolete ones, choose a \
     larger-context model, or start a new conversation for the next piece of work."
)]
pub struct ActiveMemoryBudgetExceeded {
    pub count: usize,
    pub required: usize,
    pub available: usize,
}

impl From<ActiveMemoryBudgetExceeded> for AppError {
    fn from(error: ActiveMemoryBudgetExceeded) -> Self {
        AppError::InvalidState(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capacity(tokens: usize) -> ModelCapacity {
        ModelCapacity::new("test-model", tokens)
    }

    fn fixed(tokens: usize) -> BudgetRequest {
        BudgetRequest::new(tokens, 0, 0)
    }

    #[test]
    fn a_model_matrix_reserves_output_and_margin_before_anything_else() {
        // 4K, 8K, 32K and 128K: the sizes a local build actually meets.
        for tokens in [4096, 8192, 32_768, 131_072] {
            let allocation = BudgetAllocation::plan(&capacity(tokens), &fixed(500)).expect("plan");
            assert_eq!(allocation.capacity, tokens);
            assert_eq!(
                allocation.output_reserved,
                (tokens / 4).min(OUTPUT_RESERVATION_CAP)
            );
            assert_eq!(
                allocation.safety_margin,
                ((tokens as f64 * 0.05).ceil() as usize).max(256)
            );
            assert_eq!(
                allocation.input_budget,
                tokens - allocation.output_reserved - allocation.safety_margin
            );
            assert_eq!(allocation.available, allocation.input_budget - 500);
            // The division is exactly that: a division.
            assert!(
                allocation.pools_total() <= allocation.available,
                "{tokens}: pools {} exceed available {}",
                allocation.pools_total(),
                allocation.available
            );
        }
    }

    #[test]
    fn pool_ceilings_hold_at_large_capacities_so_a_big_window_is_not_all_memory() {
        let allocation = BudgetAllocation::plan(&capacity(1_000_000), &fixed(1_000)).expect("plan");
        assert_eq!(allocation.mandatory_memory, MANDATORY_CAP);
        assert_eq!(allocation.summary, SUMMARY_CAP);
        assert_eq!(allocation.recall, RECALL_CAP);
        // Recent history has no absolute cap — a large window should carry more
        // raw conversation, not more distillation of it.
        assert!(allocation.recent_history > RECENT_FRACTION as usize);
        assert!(allocation.evidence > 0);
        assert_eq!(allocation.output_reserved, OUTPUT_RESERVATION_CAP);
    }

    #[test]
    fn a_configured_output_limit_is_honoured_but_cannot_consume_the_window() {
        let allocation = BudgetAllocation::plan(&capacity(8192).with_output_limit(1024), &fixed(0))
            .expect("plan");
        assert_eq!(allocation.output_reserved, 1024);

        // A limit larger than the window is clamped instead of underflowing the
        // input budget into an enormous wrapped number.
        let allocation =
            BudgetAllocation::plan(&capacity(8192).with_output_limit(999_999), &fixed(0))
                .expect("plan");
        assert_eq!(allocation.output_reserved, 4096);
        assert!(allocation.input_budget < 8192);
    }

    #[test]
    fn an_unusably_small_model_and_an_oversized_fixed_cost_both_error_explicitly() {
        let error = BudgetAllocation::plan(&capacity(512), &fixed(0)).unwrap_err();
        assert!(
            matches!(error, BudgetError::ModelTooSmall { .. }),
            "{error:?}"
        );
        assert!(matches!(AppError::from(error), AppError::InvalidConfig(_)));

        // A system prompt plus a pasted user message larger than the window is
        // reachable, and must not become an implicitly clipped instruction.
        let error = BudgetAllocation::plan(&capacity(4096), &fixed(100_000)).unwrap_err();
        let BudgetError::Overflow(overflow) = &error else {
            panic!("expected an overflow, got {error:?}");
        };
        assert_eq!(overflow.required, 100_000);
        assert!(matches!(AppError::from(error), AppError::InvalidInput(_)));
    }

    #[test]
    fn fixed_costs_exactly_filling_the_budget_leave_nothing_selectable_but_do_not_wrap() {
        let allocation = BudgetAllocation::plan(&capacity(4096), &fixed(0)).expect("plan");
        let exact = BudgetAllocation::plan(&capacity(4096), &fixed(allocation.input_budget))
            .expect("a full budget is not an error");
        assert_eq!(exact.available, 0);
        assert_eq!(exact.pools_total(), 0);
        assert_eq!(exact.mandatory_memory, 0);
    }

    #[test]
    fn borrowing_is_capped_and_still_funds_the_newest_turn() {
        let allocation = BudgetAllocation::plan(&capacity(32_768), &fixed(1_000)).expect("plan");
        assert!(allocation.mandatory_borrow_ceiling >= allocation.mandatory_memory);
        assert!(allocation.mandatory_borrow_ceiling <= MANDATORY_BORROW_CAP);

        // Reserving the newest turn reduces what memory may borrow...
        let with_turn = allocation.mandatory_ceiling_with_turn_reserved(2_000);
        assert!(with_turn <= allocation.mandatory_borrow_ceiling);
        // ...and an enormous newest turn cannot drive the ceiling negative.
        assert_eq!(
            allocation.mandatory_ceiling_with_turn_reserved(usize::MAX),
            0
        );
    }

    #[test]
    fn accounting_method_is_carried_into_the_allocation_rather_than_assumed() {
        let estimated = BudgetAllocation::plan(&capacity(8192), &fixed(0)).expect("plan");
        assert_eq!(estimated.accounting, TokenAccounting::Estimated);
        let exact = BudgetAllocation::plan(
            &capacity(8192).with_accounting(TokenAccounting::Exact),
            &fixed(0),
        )
        .expect("plan");
        assert_eq!(exact.accounting, TokenAccounting::Exact);
    }

    #[test]
    fn measured_history_leaves_the_room_it_does_not_need_to_evidence() {
        let reserved = BudgetAllocation::plan(&capacity(32_768), &fixed(500)).expect("plan");
        let short = BudgetAllocation::plan(
            &capacity(32_768),
            &fixed(500).with_history(HistoryCharge::Measured(200)),
        )
        .expect("plan");
        assert_eq!(short.history_charged, 200);
        assert_eq!(short.evidence, short.available - 200);
        assert!(short.evidence > reserved.evidence);

        // A history longer than its pools is charged the pools, never more:
        // the memory plan carries the rest as memory, not raw.
        let long = BudgetAllocation::plan(
            &capacity(32_768),
            &fixed(500).with_history(HistoryCharge::Measured(usize::MAX)),
        )
        .expect("plan");
        assert_eq!(long.evidence, reserved.evidence);
    }

    #[test]
    fn carried_history_has_no_memory_pools_and_is_charged_whole() {
        let allocation = BudgetAllocation::plan(
            &capacity(8192),
            &fixed(300).with_history(HistoryCharge::Carried(1_000)),
        )
        .expect("plan");
        assert_eq!(allocation.mandatory_memory + allocation.recent_history, 0);
        assert_eq!(allocation.history_charged, 1_000);
        assert_eq!(allocation.evidence, allocation.available - 1_000);

        // A carried history larger than the window leaves evidence nothing
        // rather than wrapping.
        let full = BudgetAllocation::plan(
            &capacity(8192),
            &fixed(300).with_history(HistoryCharge::Carried(1_000_000)),
        )
        .expect("plan");
        assert_eq!(full.evidence, 0);
    }

    #[test]
    fn evidence_shares_are_taken_from_what_is_left_and_charged_once() {
        let mut budget = EvidenceBudget::new(10_000);
        assert_eq!(budget.allowance(EvidenceShare::ATTACHMENTS), 6_000);
        budget.charge(5_000);
        // A quarter of what is left, capped, and never past what is left.
        assert_eq!(budget.allowance(EvidenceShare::PRIOR_EVIDENCE), 1_250);
        assert_eq!(
            EvidenceBudget::new(100_000).allowance(EvidenceShare::PRIOR_EVIDENCE),
            4_000
        );
        // The folder keeps its floor while the room can hold it, and gives
        // way when it cannot.
        assert_eq!(
            EvidenceBudget::new(1_000).allowance(EvidenceShare::FOLDER),
            500
        );
        assert_eq!(
            EvidenceBudget::new(300).allowance(EvidenceShare::FOLDER),
            300
        );
        assert_eq!(budget.remaining(), 5_000);
        budget.charge(usize::MAX);
        assert_eq!(budget.remaining(), 0);
    }

    #[test]
    fn selection_skips_what_does_not_fit_and_keeps_shorter_items_below_it() {
        let mut budget = EvidenceBudget::new(100);
        let items = [60usize, 50, 30, 20];
        let kept: Vec<usize> = budget
            .select(&items, |cost| *cost)
            .into_iter()
            .copied()
            .collect();
        assert_eq!(kept, vec![60, 30]);
        assert_eq!(budget.remaining(), 10);
    }

    #[test]
    fn an_overflow_error_reports_the_numbers_a_user_can_act_on() {
        let error = ActiveMemoryBudgetExceeded {
            count: 41,
            required: 9000,
            available: 4096,
        };
        let message = error.to_string();
        assert!(message.contains("41"));
        assert!(message.contains("9000"));
        assert!(message.contains("4096"));
        // All four of §13's practical choices, because an error that states a
        // limit without a way past it just tells the user they are stuck.
        assert!(message.contains("Review the active requirements"));
        assert!(message.contains("resolve the obsolete ones"));
        assert!(message.contains("larger-context model"));
        assert!(message.contains("start a new conversation"));
    }
}
