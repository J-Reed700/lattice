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

use crate::shared::error::{AppError, Result};

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

/// Default generation reservation when the caller configures none.
fn default_output_reservation(capacity: usize) -> usize {
    (capacity / 4).min(4096)
}

/// Safety margin against tokenizer imprecision and provider framing.
fn safety_margin(capacity: usize) -> usize {
    let proportional = (capacity as f64 * 0.05).ceil() as usize;
    proportional.max(256)
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
    pub available: usize,
    pub mandatory_memory: usize,
    pub summary: usize,
    pub recall: usize,
    pub recent_history: usize,
    pub rag_and_tools: usize,
    /// Hard ceiling on mandatory memory after it borrows unused optional room.
    pub mandatory_borrow_ceiling: usize,
    pub accounting: TokenAccounting,
}

impl BudgetAllocation {
    /// Divide `capacity` after paying `fixed`.
    ///
    /// # Errors
    ///
    /// - [`AppError::InvalidConfig`] when the model is too small to hold a
    ///   prompt at all, so the caller can say which model to change rather than
    ///   emitting a request that is certain to be rejected.
    /// - [`AppError::InvalidInput`] when the unavoidable parts alone exceed the
    ///   input budget. That is a real, reachable state — a very long system
    ///   prompt, or a pasted user message bigger than the window — and it must
    ///   surface as an error rather than an implicitly clipped instruction.
    pub fn plan(capacity: &ModelCapacity, fixed: usize) -> Result<Self> {
        let total = capacity.context_tokens;
        if total < MIN_USABLE_CAPACITY {
            return Err(AppError::InvalidConfig(format!(
                "Model {} advertises a {}-token context, below the {} needed to assemble a \
                 prompt. Choose a larger-context model.",
                capacity.model_identity, total, MIN_USABLE_CAPACITY
            )));
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
            .ok_or_else(|| {
                AppError::InvalidConfig(format!(
                    "Model {} cannot reserve {} output tokens and a {}-token margin inside a \
                     {}-token context.",
                    capacity.model_identity, output_reserved, margin, total
                ))
            })?;

        if fixed > input_budget {
            return Err(AppError::InvalidInput(format!(
                "The system policy, tools and current message need {} tokens, above the {} this \
                 request may occupy. Shorten the message or choose a larger-context model.",
                fixed, input_budget
            )));
        }
        let available = input_budget - fixed;

        let fraction = |value: f64, cap: usize| ((available as f64 * value) as usize).min(cap);
        let mandatory_memory = fraction(MANDATORY_FRACTION, MANDATORY_CAP);
        let summary = fraction(SUMMARY_FRACTION, SUMMARY_CAP);
        let recall = fraction(RECALL_FRACTION, RECALL_CAP);
        let recent_history = (available as f64 * RECENT_FRACTION) as usize;

        // Whatever the named pools did not claim. Saturating because the four
        // above are each capped and cannot exceed `available` together, but a
        // future tuning change must not be able to wrap this.
        let rag_and_tools = available
            .saturating_sub(mandatory_memory)
            .saturating_sub(summary)
            .saturating_sub(recall)
            .saturating_sub(recent_history);

        let mandatory_borrow_ceiling = ((available as f64 * MANDATORY_BORROW_FRACTION) as usize)
            .min(MANDATORY_BORROW_CAP)
            .max(mandatory_memory);

        let allocation = Self {
            capacity: total,
            output_reserved,
            safety_margin: margin,
            input_budget,
            fixed,
            available,
            mandatory_memory,
            summary,
            recall,
            recent_history,
            rag_and_tools,
            mandatory_borrow_ceiling,
            accounting: capacity.accounting,
        };
        debug_assert!(
            allocation.pools_total() <= available,
            "pool allocations must be a division of `available`, not additions to it"
        );
        Ok(allocation)
    }

    /// Sum of the selectable pools. Never more than [`Self::available`].
    pub fn pools_total(&self) -> usize {
        self.mandatory_memory
            + self.summary
            + self.recall
            + self.recent_history
            + self.rag_and_tools
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

    #[test]
    fn a_model_matrix_reserves_output_and_margin_before_anything_else() {
        // 4K, 8K, 32K and 128K: the sizes a local build actually meets.
        for tokens in [4096, 8192, 32_768, 131_072] {
            let allocation = BudgetAllocation::plan(&capacity(tokens), 500).expect("plan");
            assert_eq!(allocation.capacity, tokens);
            assert_eq!(allocation.output_reserved, (tokens / 4).min(4096));
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
        let allocation = BudgetAllocation::plan(&capacity(1_000_000), 1_000).expect("plan");
        assert_eq!(allocation.mandatory_memory, MANDATORY_CAP);
        assert_eq!(allocation.summary, SUMMARY_CAP);
        assert_eq!(allocation.recall, RECALL_CAP);
        // Recent history has no absolute cap — a large window should carry more
        // raw conversation, not more distillation of it.
        assert!(allocation.recent_history > RECENT_FRACTION as usize);
        assert!(allocation.rag_and_tools > 0);
    }

    #[test]
    fn a_configured_output_limit_is_honoured_but_cannot_consume_the_window() {
        let allocation =
            BudgetAllocation::plan(&capacity(8192).with_output_limit(1024), 0).expect("plan");
        assert_eq!(allocation.output_reserved, 1024);

        // A limit larger than the window is clamped instead of underflowing the
        // input budget into an enormous wrapped number.
        let allocation =
            BudgetAllocation::plan(&capacity(8192).with_output_limit(999_999), 0).expect("plan");
        assert_eq!(allocation.output_reserved, 4096);
        assert!(allocation.input_budget < 8192);
    }

    #[test]
    fn an_unusably_small_model_and_an_oversized_fixed_cost_both_error_explicitly() {
        let error = BudgetAllocation::plan(&capacity(512), 0).unwrap_err();
        assert!(matches!(error, AppError::InvalidConfig(_)), "{error:?}");

        // A system prompt plus a pasted user message larger than the window is
        // reachable, and must not become an implicitly clipped instruction.
        let error = BudgetAllocation::plan(&capacity(4096), 100_000).unwrap_err();
        assert!(matches!(error, AppError::InvalidInput(_)), "{error:?}");
    }

    #[test]
    fn fixed_costs_exactly_filling_the_budget_leave_nothing_selectable_but_do_not_wrap() {
        let allocation = BudgetAllocation::plan(&capacity(4096), 0).expect("plan");
        let exact = BudgetAllocation::plan(&capacity(4096), allocation.input_budget)
            .expect("a full budget is not an error");
        assert_eq!(exact.available, 0);
        assert_eq!(exact.pools_total(), 0);
        assert_eq!(exact.mandatory_memory, 0);
    }

    #[test]
    fn borrowing_is_capped_and_still_funds_the_newest_turn() {
        let allocation = BudgetAllocation::plan(&capacity(32_768), 1_000).expect("plan");
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
        let estimated = BudgetAllocation::plan(&capacity(8192), 0).expect("plan");
        assert_eq!(estimated.accounting, TokenAccounting::Estimated);
        let exact =
            BudgetAllocation::plan(&capacity(8192).with_accounting(TokenAccounting::Exact), 0)
                .expect("plan");
        assert_eq!(exact.accounting, TokenAccounting::Exact);
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
