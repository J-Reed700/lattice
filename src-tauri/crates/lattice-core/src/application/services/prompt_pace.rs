//! How much prompt a model call with a deadline may send: what its model has
//! shown it can read within that deadline.
//!
//! A call that must answer in seconds (turn intent, the corpus planner, a
//! tutor reply) cannot be sized from its window alone. A quarter of a 32K
//! window is 8K tokens, which a local 9B model reads in about thirty seconds,
//! so a long chat timed the intent classifier out on every turn. The pace is
//! learned from the provider's own prefill timing, not from the time to the
//! answer, which also holds queueing and the answer's writing; and an answer
//! that arrived in time never shrinks the next prompt.

use std::sync::Mutex;
use std::time::Duration;

use crate::application::ports::llm_port::PrefillTiming;

/// The share of a deadline a prompt is sized to take to read. The rest is
/// for writing the answer and any wait in the provider's queue.
const READ_SHARE_OF_DEADLINE: f64 = 0.5;

/// What one kind of deadline-bound call has learned about its model. Shared by
/// every call of that kind, because how fast a model reads is a property of
/// the model and the machine, not of any one conversation.
pub struct PromptPace {
    deadline: Duration,
    floor: usize,
    state: Mutex<PaceState>,
}

struct PaceState {
    /// Prompt tokens the next call may send. `None` until something has been
    /// learned; the caller's window share stands until then.
    tokens: Option<usize>,
    /// Prompt tokens read per second, from the provider's prefill timing.
    per_second: Option<f64>,
}

impl PromptPace {
    /// A pace for calls that must answer within `deadline`, starting from the
    /// caller's window share.
    pub const fn new(deadline: Duration) -> Self {
        Self {
            deadline,
            floor: 0,
            state: Mutex::new(PaceState {
                tokens: None,
                per_second: None,
            }),
        }
    }

    /// A pace that starts at `tokens` and is never shrunk below `floor`.
    pub const fn starting_at(deadline: Duration, tokens: usize, floor: usize) -> Self {
        Self {
            deadline,
            floor,
            state: Mutex::new(PaceState {
                tokens: Some(tokens),
                per_second: None,
            }),
        }
    }

    pub fn deadline(&self) -> Duration {
        self.deadline
    }

    fn state(&self) -> std::sync::MutexGuard<'_, PaceState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Prompt tokens the next call may send, or `None` while nothing has been
    /// learned (the caller then sends its window share).
    pub fn tokens(&self) -> Option<usize> {
        self.state().tokens
    }

    /// The most of `window_share` tokens the next call may send.
    pub fn bound(&self, window_share: usize) -> usize {
        self.tokens()
            .map_or(window_share, |tokens| tokens.min(window_share))
    }

    /// Tokens a model reading `per_second` gets through in its share of the
    /// deadline.
    fn readable(&self, per_second: f64) -> usize {
        (per_second * self.deadline.as_secs_f64() * READ_SHARE_OF_DEADLINE) as usize
    }

    /// A call of `sent_tokens` answered within the deadline. `prefill` is the
    /// provider's own measure of reading the prompt, when it gives one.
    ///
    /// The next prompt may grow toward what the measured pace reads in time,
    /// at most doubling per answer; it never shrinks below what just answered
    /// in time, nor below what was allowed before.
    pub fn answered(&self, prefill: Option<PrefillTiming>, sent_tokens: usize) {
        let readable = prefill
            .and_then(|prefill| prefill.tokens_per_second())
            .map(|per_second| (per_second, self.readable(per_second)));
        let mut state = self.state();
        if let Some((per_second, _)) = readable {
            state.per_second = Some(per_second);
        }
        let readable = readable.map(|(_, tokens)| tokens);
        state.tokens = match (state.tokens, readable) {
            // Nothing measured and nothing set: the window share stands.
            (None, None) => None,
            (None, Some(readable)) => Some(readable.max(sent_tokens)),
            (Some(current), readable) => {
                let kept = current.max(sent_tokens);
                Some(readable.map_or(kept, |readable| {
                    readable.min(kept.saturating_mul(2)).max(kept)
                }))
            }
        };
    }

    /// A call of `sent_tokens` ran out of time. The next prompt is half of it,
    /// or what the measured pace reads in time when that is less.
    pub fn timed_out(&self, sent_tokens: usize) {
        let mut state = self.state();
        let half = sent_tokens.min(state.tokens.unwrap_or(usize::MAX)) / 2;
        let paced = state
            .per_second
            .map_or(usize::MAX, |per_second| self.readable(per_second));
        state.tokens = Some(half.min(paced).max(self.floor));
    }

    /// The provider refused a prompt of `sent_tokens` as too large for its
    /// window: the next one is half of it.
    pub fn overflowed(&self, sent_tokens: usize) {
        let mut state = self.state();
        let current = state.tokens.unwrap_or(usize::MAX);
        state.tokens = Some((sent_tokens.min(current) / 2).max(self.floor));
    }
}

/// The share of a deadline-bound call's window its history may take, bounded
/// by what `pace` says the model reads in time once `fixed_tokens` (the
/// call's instructions and material) are paid for.
pub fn history_room_within(pace: &PromptPace, window_share: usize, fixed_tokens: usize) -> usize {
    match pace.tokens() {
        None => window_share,
        Some(tokens) => window_share.min(tokens.saturating_sub(fixed_tokens)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(tokens: u64, seconds: f64) -> Option<PrefillTiming> {
        Some(PrefillTiming { tokens, seconds })
    }

    /// The reported failure: 8K tokens of history at 300 tokens a second
    /// cannot be read inside a 15-second deadline. After one measured answer
    /// the next prompt is what that pace reads in half the deadline.
    #[test]
    fn a_measured_pace_bounds_the_window_share() {
        let pace = PromptPace::new(Duration::from_secs(15));
        assert_eq!(pace.bound(8_000), 8_000, "nothing measured: the share");
        pace.answered(read(600, 2.0), 600);
        assert_eq!(pace.tokens(), Some(2_250));
        assert_eq!(pace.bound(8_000), 2_250);
        assert_eq!(history_room_within(&pace, 8_000, 250), 2_000);
    }

    /// The time to the answer includes writing it and waiting in a queue;
    /// only the prefill measure is used, and an answer in time never shrinks
    /// the next prompt below what it sent.
    #[test]
    fn an_answer_in_time_never_shrinks_the_next_prompt() {
        let pace = PromptPace::starting_at(Duration::from_secs(20), 2_560, 1_024);
        // Read slowly (2,560 tokens in 11s, past the 10s share), yet in
        // time: the prompt keeps its room.
        pace.answered(read(2_560, 11.0), 2_560);
        assert_eq!(pace.tokens(), Some(2_560));
        // A quick read earns at most a doubling.
        pace.answered(read(2_560, 1.0), 2_560);
        assert_eq!(pace.tokens(), Some(5_120));
        // No measure from the provider: kept as it is.
        pace.answered(None, 1_000);
        assert_eq!(pace.tokens(), Some(5_120));
    }

    /// With no measure at all the window share stands until something is
    /// learned; a timeout then halves what was sent.
    #[test]
    fn a_timeout_halves_and_a_known_pace_can_say_less() {
        let unmeasured = PromptPace::new(Duration::from_secs(15));
        unmeasured.answered(None, 4_000);
        assert_eq!(unmeasured.tokens(), None);
        unmeasured.timed_out(8_000);
        assert_eq!(unmeasured.tokens(), Some(4_000));

        // A fast read earned 7,500 tokens; a later, slower read in time kept
        // them. When that prompt then times out, the slower measured pace
        // says less than half of it.
        let measured = PromptPace::new(Duration::from_secs(15));
        measured.answered(read(1_000, 1.0), 1_000);
        assert_eq!(measured.tokens(), Some(7_500));
        measured.answered(read(300, 1.0), 7_500);
        assert_eq!(measured.tokens(), Some(7_500));
        measured.timed_out(7_500);
        assert_eq!(measured.tokens(), Some(2_250));

        let floored = PromptPace::starting_at(Duration::from_secs(20), 1_200, 1_024);
        floored.timed_out(1_200);
        assert_eq!(floored.tokens(), Some(1_024));
        floored.overflowed(1_024);
        assert_eq!(floored.tokens(), Some(1_024));
    }
}
