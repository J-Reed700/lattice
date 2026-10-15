//! How much prompt the corpus planner may be sent, learned from how it has
//! been answering, and what one turn may spend on it.
//!
//! The planner used to have one response to a timeout: stand down for ten
//! minutes. On a 9B model with a 379-document vault every request timed out or
//! overflowed, so the first timeout of a session switched planning off, and
//! with it the corrective retry, for most of the session. A timeout now says
//! "send less next time", which is something the next attempt can act on.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tokio::time::Instant;

use super::PLANNER_TIMEOUT;

/// The prompt, system prompt included, the planner is sent before this machine
/// has answered once: what the reference machine (Qwen3.5-9B Q4_K_M on Metal,
/// about 300 prompt tokens a second) prefills in about nine seconds. From the
/// first answer on, the size comes from the speed actually measured (see
/// [`PlannerHealth::answered`]).
pub(super) const PLANNER_PROMPT_TOKENS: usize = 2_560;

/// The share of [`PLANNER_TIMEOUT`] a prompt is sized to take, measured end to
/// end. The rest is room for a slower prefill, the plan's own writing, or a
/// queue wait before the timeout.
const PROMPT_SHARE_OF_TIMEOUT: f64 = 0.5;

/// The smallest prompt a timeout shrinks the planner to: the system prompt,
/// the request, a little conversation and a handful of catalog rows. A planner
/// that cannot answer this in time is not going to answer anything.
pub(super) const PLANNER_PROMPT_FLOOR: usize = 1_024;

/// How long the planner stands down after even the smallest prompt timed out.
///
/// Brief on purpose. The usual cause is a server busy with other work, which
/// passes; the cost of standing down too long is every turn in between
/// planning locally and skipping the corrective retry.
pub(super) const PLANNER_STAND_DOWN: Duration = Duration::from_secs(2 * 60);

/// What the planner has recently managed. One per chat runtime, shared by
/// every turn it runs, because "how much can this model prefill in time" is a
/// property of the machine and the model, not of any one conversation.
pub struct PlannerHealth {
    state: Mutex<HealthState>,
}

struct HealthState {
    prompt_tokens: usize,
    stood_down_until: Option<Instant>,
}

impl Default for PlannerHealth {
    fn default() -> Self {
        Self::new()
    }
}

impl PlannerHealth {
    pub(crate) const fn new() -> Self {
        Self {
            state: Mutex::new(HealthState {
                prompt_tokens: PLANNER_PROMPT_TOKENS,
                stood_down_until: None,
            }),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, HealthState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The prompt size, in tokens, the next attempt should aim for.
    pub(super) fn prompt_tokens(&self) -> usize {
        self.state().prompt_tokens
    }

    /// How much longer the planner is standing down, if it is.
    pub(super) fn standing_down(&self) -> Option<Duration> {
        let state = self.state();
        let until = state.stood_down_until?;
        until.checked_duration_since(Instant::now())
    }

    /// An answer in time ends any stand-down and measures the planner: a
    /// prompt of `sent_tokens` answered in `elapsed`. The next prompt is what
    /// that rate gets through in half of [`PLANNER_TIMEOUT`], so a fast
    /// planner is shown more of the catalog and a slow one less, with no fixed
    /// ceiling. Growth is at most a doubling per answer, so one quick reply to
    /// a short prompt cannot jump the next past what the planner has shown it
    /// can do; an answer inside the target share never shrinks the prompt,
    /// since the time it took may be the plan's writing rather than the
    /// prompt's reading.
    pub(super) fn answered(&self, elapsed: Duration, sent_tokens: usize) {
        let mut state = self.state();
        state.stood_down_until = None;
        let target_time = PLANNER_TIMEOUT.as_secs_f64() * PROMPT_SHARE_OF_TIMEOUT;
        let measured = if elapsed.is_zero() {
            usize::MAX
        } else {
            (sent_tokens as f64 * target_time / elapsed.as_secs_f64()) as usize
        };
        let mut next = measured.min(state.prompt_tokens.saturating_mul(2));
        if elapsed.as_secs_f64() <= target_time {
            next = next.max(state.prompt_tokens);
        }
        state.prompt_tokens = next.max(PLANNER_PROMPT_FLOOR);
    }

    /// A timeout halves the next attempt's prompt. Only a timeout at the floor
    /// stands the planner down, because there is nothing smaller left to try.
    pub(super) fn timed_out(&self, sent_tokens: usize) {
        let mut state = self.state();
        let at_floor = state.prompt_tokens <= PLANNER_PROMPT_FLOOR;
        state.prompt_tokens = (sent_tokens.min(state.prompt_tokens) / 2).max(PLANNER_PROMPT_FLOOR);
        if at_floor {
            state.stood_down_until = Some(Instant::now() + PLANNER_STAND_DOWN);
        }
    }

    /// The server refused the prompt as larger than its window. The prompt is
    /// counted against the window the port reports, so this means the report
    /// was wrong. Halving finds the real window within a few turns; there is
    /// no stand-down, since a refusal costs no time.
    pub(super) fn overflowed(&self, sent_tokens: usize) {
        let mut state = self.state();
        state.prompt_tokens = (sent_tokens.min(state.prompt_tokens) / 2).max(PLANNER_PROMPT_FLOOR);
    }
}

/// One turn's use of the planner: the documents a local search ranked for the
/// request, and whether the planner has already cost this turn a timeout.
///
/// A turn pays for at most one planner timeout. The corrective retry replans
/// on every turn where the first pass did not time out, including the turns
/// after one that did; it only gives way on the turn that already waited.
pub(in super::super) struct PlannerTurn<'h> {
    /// Document IDs best first, from searching the request. `None` until the
    /// search has run, so a turn searches once however many times it plans.
    pub(in super::super) candidates: Option<Vec<String>>,
    timed_out: AtomicBool,
    pub(super) health: &'h PlannerHealth,
}

impl<'h> PlannerTurn<'h> {
    pub(in super::super) fn with_health(health: &'h PlannerHealth) -> Self {
        Self {
            candidates: None,
            timed_out: AtomicBool::new(false),
            health,
        }
    }

    /// Why the planner should not be asked again this turn, if it should not.
    pub(in super::super) fn unavailable(&self) -> Option<String> {
        if self.timed_out.load(Ordering::Relaxed) {
            return Some("the planner already timed out on this turn".into());
        }
        self.health.standing_down().map(|remaining| {
            format!(
                "the planner is standing down for another {}s after timing out at its smallest prompt",
                remaining.as_secs()
            )
        })
    }

    pub(super) fn record_timeout(&self, sent_tokens: usize) {
        self.timed_out.store(true, Ordering::Relaxed);
        self.health.timed_out(sent_tokens);
    }

    pub(super) fn candidate_ids(&self) -> &[String] {
        self.candidates.as_deref().unwrap_or_default()
    }
}
