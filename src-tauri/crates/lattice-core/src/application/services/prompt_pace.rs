//! How much prompt a model call with a deadline may send: what its model has
//! shown it can read within that deadline.
//!
//! A call that must answer in seconds (turn intent, the corpus planner, a
//! tutor reply, the grounding judge) cannot be sized from its window alone. A
//! quarter of a 32K window is 8K tokens, which a local 9B model reads in about
//! thirty seconds, so a long chat timed the intent classifier out on every
//! turn. The window is a ceiling here, never a target.
//!
//! The pace is learned per model, from the provider's own prefill timing, not
//! from the time to the answer, which also holds queueing and the answer's
//! writing; an answer that arrived in time never shrinks the next prompt; and
//! a timeout the measured pace cannot explain (the prompt reads in a fraction
//! of the deadline, so the time went to a queue) does not shrink it either.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use crate::application::ports::llm_port::PrefillTiming;

/// The share of a deadline a prompt for background work may take to read.
/// The rest is for writing the answer and any wait in the provider's queue.
const READ_SHARE_OF_DEADLINE: f64 = 0.5;

/// The share of a deadline a prompt the user is waiting on may take to read.
/// Smaller, because the reading is time before the first word of the reply:
/// the planner grown to half its 20 s deadline read for ten seconds, twice a
/// turn, before the answer could start.
const CRITICAL_PATH_READ_SHARE: f64 = 0.2;

/// The pace assumed for a model nothing has been learned about: the reference
/// machine's (Qwen3.5-9B Q4_K_M on Metal reads about 300 prompt tokens a
/// second). A faster model earns more from its first answer on, at most a
/// doubling an answer; a slower one times out once and halves.
const ASSUMED_TOKENS_PER_SECOND: f64 = 300.0;

/// What one kind of deadline-bound call has learned about each model it runs
/// on. Shared by every call of that kind, because how fast a model reads is a
/// property of the model and the machine, not of any one conversation; and
/// keyed by model, because a pace learned on a remote 27B says nothing about
/// the local 9B the user switches to.
pub struct PromptPace {
    deadline: Duration,
    floor: usize,
    start: Option<usize>,
    read_share: f64,
    models: Mutex<HashMap<String, PaceState>>,
}

#[derive(Clone, Copy)]
struct PaceState {
    /// Prompt tokens the next call may send.
    tokens: usize,
    /// Prompt tokens read per second, from the provider's prefill timing.
    per_second: Option<f64>,
}

impl PromptPace {
    /// A pace for background calls that must answer within `deadline`,
    /// starting from what the assumed pace reads in its share of it.
    pub fn new(deadline: Duration) -> Self {
        Self {
            deadline,
            floor: 0,
            start: None,
            read_share: READ_SHARE_OF_DEADLINE,
            models: Mutex::new(HashMap::new()),
        }
    }

    /// A pace that starts at `tokens` and is never shrunk below `floor`.
    pub fn starting_at(deadline: Duration, tokens: usize, floor: usize) -> Self {
        Self {
            start: Some(tokens),
            floor,
            ..Self::new(deadline)
        }
    }

    /// For a call the user is waiting on: its prompt is sized to a smaller
    /// share of the deadline, so reading it does not hold up the reply.
    pub fn on_critical_path(mut self) -> Self {
        self.read_share = CRITICAL_PATH_READ_SHARE;
        self
    }

    /// Never shrink below `floor` tokens: what the call needs to say anything
    /// useful at all (its instructions and the newest turn).
    pub fn with_floor(mut self, floor: usize) -> Self {
        self.floor = floor;
        self
    }

    pub fn deadline(&self) -> Duration {
        self.deadline
    }

    fn models(&self) -> std::sync::MutexGuard<'_, HashMap<String, PaceState>> {
        self.models
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn fresh(&self) -> PaceState {
        PaceState {
            tokens: self
                .start
                .unwrap_or_else(|| self.readable(ASSUMED_TOKENS_PER_SECOND))
                .max(self.floor),
            per_second: None,
        }
    }

    fn with_state<T>(&self, model: &str, f: impl FnOnce(&mut PaceState) -> T) -> T {
        let mut models = self.models();
        let fresh = self.fresh();
        f(models.entry(model.to_string()).or_insert(fresh))
    }

    /// Prompt tokens the next call on `model` may send.
    pub fn tokens(&self, model: &str) -> usize {
        self.models()
            .get(model)
            .map_or_else(|| self.fresh().tokens, |state| state.tokens)
    }

    /// The most of `window_share` tokens the next call on `model` may send.
    pub fn bound(&self, model: &str, window_share: usize) -> usize {
        self.tokens(model).min(window_share)
    }

    /// Prompt tokens `model` has been measured to read in `within`, or `None`
    /// before anything is measured. For a call that spends one deadline over
    /// several requests and sizes each from the time that is left.
    pub fn readable_within(&self, model: &str, within: Duration) -> Option<usize> {
        let per_second = self.models().get(model)?.per_second?;
        Some((per_second * within.as_secs_f64()) as usize)
    }

    /// Tokens a model reading `per_second` gets through in its share of the
    /// deadline.
    fn readable(&self, per_second: f64) -> usize {
        (per_second * self.deadline.as_secs_f64() * self.read_share) as usize
    }

    /// A call of `sent_tokens` to `model` answered within the deadline.
    /// `prefill` is the provider's own measure of reading the prompt, when it
    /// gives one.
    ///
    /// The measure teaches a rate only when the provider evaluated at least
    /// half of what was sent: a prompt mostly served from the KV cache
    /// measures a small batch, whose per-token time is dominated by fixed
    /// costs and reads as a far slower model than it is.
    ///
    /// The next prompt may grow toward what the measured pace reads in time,
    /// at most doubling per answer; it never shrinks below what just answered
    /// in time, nor below what was allowed before.
    pub fn answered(&self, model: &str, prefill: Option<PrefillTiming>, sent_tokens: usize) {
        let per_second = prefill
            .filter(|prefill| prefill.tokens.saturating_mul(2) >= sent_tokens as u64)
            .and_then(|prefill| prefill.tokens_per_second());
        let readable = per_second.map(|per_second| self.readable(per_second));
        self.with_state(model, |state| {
            if per_second.is_some() {
                state.per_second = per_second;
            }
            let kept = state.tokens.max(sent_tokens);
            state.tokens = readable.map_or(kept, |readable| {
                readable.min(kept.saturating_mul(2)).max(kept)
            });
        });
    }

    /// A call of `sent_tokens` to `model` ran out of time. The next prompt is
    /// half of it, or what the measured pace reads in time when that is less.
    ///
    /// When the measured pace reads `sent_tokens` in under half the deadline,
    /// the prompt is not what ran out the clock: the time went to a queue
    /// ahead of it (or to a long answer), and shrinking the prompt would only
    /// starve the next call of context. The budget then stands.
    pub fn timed_out(&self, model: &str, sent_tokens: usize) {
        let deadline = self.deadline.as_secs_f64();
        self.with_state(model, |state| {
            if let Some(per_second) = state.per_second {
                if sent_tokens as f64 / per_second <= deadline * READ_SHARE_OF_DEADLINE {
                    return;
                }
            }
            let half = sent_tokens.min(state.tokens) / 2;
            let paced = state
                .per_second
                .map_or(usize::MAX, |per_second| self.readable(per_second));
            state.tokens = half.min(paced).max(self.floor);
        });
    }

    /// The provider refused a prompt of `sent_tokens` as too large for its
    /// window: the next one is half of it.
    pub fn overflowed(&self, model: &str, sent_tokens: usize) {
        self.with_state(model, |state| {
            state.tokens = (sent_tokens.min(state.tokens) / 2).max(self.floor);
        });
    }
}

/// The share of a deadline-bound call's window its history may take, bounded
/// by what `pace` says `model` reads in time once `fixed_tokens` (the call's
/// instructions and material) are paid for.
pub fn history_room_within(
    pace: &PromptPace,
    model: &str,
    window_share: usize,
    fixed_tokens: usize,
) -> usize {
    window_share.min(pace.tokens(model).saturating_sub(fixed_tokens))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCAL: &str = "qwen3.5-9b";
    const REMOTE: &str = "qwen3.5-27b";

    fn read(tokens: u64, seconds: f64) -> Option<PrefillTiming> {
        Some(PrefillTiming { tokens, seconds })
    }

    /// The whole of `sent` evaluated at `per_second`.
    fn at_rate(sent: usize, per_second: f64) -> Option<PrefillTiming> {
        read(sent as u64, sent as f64 / per_second)
    }

    /// The reported failure: 8K tokens of history at 300 tokens a second
    /// cannot be read inside a 15-second deadline. Before anything is
    /// measured the assumed pace sizes the prompt, never the window share;
    /// after one measured answer it is what that pace reads in half the
    /// deadline.
    #[test]
    fn a_measured_pace_bounds_the_window_share() {
        let pace = PromptPace::new(Duration::from_secs(15));
        assert_eq!(
            pace.bound(LOCAL, 8_000),
            2_250,
            "the window is not a target"
        );
        assert_eq!(pace.bound(LOCAL, 1_000), 1_000, "it is a ceiling");
        pace.answered(LOCAL, read(600, 2.0), 600);
        assert_eq!(pace.tokens(LOCAL), 2_250);
        pace.answered(LOCAL, read(1_000, 1.0), 1_000);
        assert_eq!(pace.bound(LOCAL, 8_000), 4_500, "a doubling at most");
        assert_eq!(history_room_within(&pace, LOCAL, 8_000, 500), 4_000);
    }

    /// A pace learned on one model says nothing about another: switching
    /// from a fast remote server to the local 9B starts the 9B afresh.
    #[test]
    fn the_pace_is_kept_per_model() {
        let pace = PromptPace::new(Duration::from_secs(15));
        for _ in 0..4 {
            let sent = pace.tokens(REMOTE);
            pace.answered(REMOTE, at_rate(sent, 4_000.0), sent);
        }
        assert_eq!(pace.tokens(REMOTE), 30_000);
        assert_eq!(
            pace.tokens(LOCAL),
            2_250,
            "the 9B starts from the assumed pace"
        );
        pace.timed_out(LOCAL, 2_250);
        assert_eq!(pace.tokens(REMOTE), 30_000, "and its timeouts are its own");
    }

    /// llama.cpp's prompt_n excludes tokens reused from the cache. A prompt
    /// served almost entirely from the cache measures a tiny batch (50
    /// tokens in a second), which is not the model's reading speed; it must
    /// not teach a rate that a later timeout would then clamp the prompt to.
    #[test]
    fn a_mostly_cached_prompt_teaches_no_rate() {
        let pace = PromptPace::starting_at(Duration::from_secs(15), 3_000, 0);
        pace.answered(LOCAL, read(50, 1.0), 3_000);
        assert_eq!(pace.readable_within(LOCAL, Duration::from_secs(1)), None);
        pace.timed_out(LOCAL, 3_000);
        assert_eq!(pace.tokens(LOCAL), 1_500, "halved, not clamped to 375");

        pace.answered(LOCAL, read(1_600, 1.0), 3_000);
        assert_eq!(
            pace.readable_within(LOCAL, Duration::from_secs(1)),
            Some(1_600),
            "half or more evaluated: a real measure"
        );
    }

    /// The time to the answer includes writing it and waiting in a queue;
    /// only the prefill measure is used, and an answer in time never shrinks
    /// the next prompt below what it sent.
    #[test]
    fn an_answer_in_time_never_shrinks_the_next_prompt() {
        let pace = PromptPace::starting_at(Duration::from_secs(20), 2_560, 1_024);
        // Read slowly (2,560 tokens in 11s, past the 10s share), yet in
        // time: the prompt keeps its room.
        pace.answered(LOCAL, read(2_560, 11.0), 2_560);
        assert_eq!(pace.tokens(LOCAL), 2_560);
        // A quick read earns at most a doubling.
        pace.answered(LOCAL, read(2_560, 1.0), 2_560);
        assert_eq!(pace.tokens(LOCAL), 5_120);
        // No measure from the provider: kept as it is.
        pace.answered(LOCAL, None, 1_000);
        assert_eq!(pace.tokens(LOCAL), 5_120);
    }

    /// A timeout halves what was sent, and a known pace can say less; a
    /// timeout the known pace cannot explain is a queue and shrinks nothing.
    #[test]
    fn a_timeout_halves_unless_the_pace_says_the_prompt_was_not_the_cause() {
        let unmeasured = PromptPace::starting_at(Duration::from_secs(15), 8_000, 0);
        unmeasured.answered(LOCAL, None, 4_000);
        unmeasured.timed_out(LOCAL, 8_000);
        assert_eq!(unmeasured.tokens(LOCAL), 4_000);

        // 300 tokens a second: 7,500 tokens take 25 s, past the deadline,
        // so the timeout is the prompt's, and the pace says less than half.
        let measured = PromptPace::starting_at(Duration::from_secs(15), 7_500, 0);
        measured.answered(LOCAL, read(4_500, 15.0), 7_500);
        assert_eq!(measured.tokens(LOCAL), 7_500);
        measured.timed_out(LOCAL, 7_500);
        assert_eq!(measured.tokens(LOCAL), 2_250);

        // 1,000 tokens a second reads 2,000 in 2 s of a 15 s deadline: the
        // time went to a queue, and the budget stands.
        let queued = PromptPace::new(Duration::from_secs(15));
        queued.answered(LOCAL, read(2_000, 2.0), 2_000);
        let before = queued.tokens(LOCAL);
        queued.timed_out(LOCAL, 2_000);
        assert_eq!(queued.tokens(LOCAL), before);

        let floored = PromptPace::starting_at(Duration::from_secs(20), 1_200, 1_024);
        floored.timed_out(LOCAL, 1_200);
        assert_eq!(floored.tokens(LOCAL), 1_024);
        floored.overflowed(LOCAL, 1_024);
        assert_eq!(floored.tokens(LOCAL), 1_024);
    }

    /// A call the user waits on reads for a smaller share of its deadline,
    /// so its prompt grows to less than a background call's at the same pace.
    #[test]
    fn a_critical_path_call_reads_for_a_smaller_share_of_its_deadline() {
        let background = PromptPace::new(Duration::from_secs(20));
        let critical = PromptPace::new(Duration::from_secs(20)).on_critical_path();
        assert_eq!(critical.tokens(LOCAL), 1_200);
        assert!(critical.tokens(LOCAL) < background.tokens(LOCAL));
        for pace in [&background, &critical] {
            for _ in 0..8 {
                let sent = pace.tokens(LOCAL);
                pace.answered(LOCAL, at_rate(sent, 1_000.0), sent);
            }
        }
        assert_eq!(background.tokens(LOCAL), 10_000);
        assert_eq!(critical.tokens(LOCAL), 4_000);
    }

    /// A floor holds against any number of timeouts: what the call needs to
    /// be useful at all.
    #[test]
    fn a_floor_holds_against_repeated_timeouts() {
        let pace = PromptPace::new(Duration::from_secs(15)).with_floor(1_024);
        for _ in 0..10 {
            pace.timed_out(LOCAL, pace.tokens(LOCAL));
        }
        assert_eq!(pace.tokens(LOCAL), 1_024);
    }
}
