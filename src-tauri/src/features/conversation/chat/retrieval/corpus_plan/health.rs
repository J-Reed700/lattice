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

/// The prompt, system prompt included, that the planner is sent by default.
///
/// Sized to answer inside [`PLANNER_TIMEOUT`] on the reference machine
/// (Qwen3.5-9B Q4_K_M on Metal), which prefills about 300 tokens a second and
/// writes about 14. A full plan of three queries and four document handles is
/// about 100 tokens, seven seconds of writing; 2,560 prompt tokens are another
/// nine seconds of prefill. That leaves four seconds for a slower prefill or a
/// queue wait before the timeout.
pub(super) const PLANNER_PROMPT_TOKENS: usize = 2_560;

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

/// What the planner has recently managed. Process-wide in production, because
/// "how much can this model prefill in time" is a property of the machine and
/// the model, not of any one conversation.
pub(in super::super) struct PlannerHealth {
    state: Mutex<HealthState>,
}

struct HealthState {
    prompt_tokens: usize,
    stood_down_until: Option<Instant>,
}

impl PlannerHealth {
    pub(in super::super) const fn new() -> Self {
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

    /// An answer in time ends any stand-down. A comfortably quick one also
    /// earns back prompt room, one doubling at a time; a slow one keeps the
    /// size that worked, so the planner does not oscillate between a size that
    /// times out and one that barely does not.
    pub(super) fn answered(&self, elapsed: Duration) {
        let mut state = self.state();
        state.stood_down_until = None;
        if elapsed <= PLANNER_TIMEOUT / 2 {
            state.prompt_tokens = (state.prompt_tokens * 2).min(PLANNER_PROMPT_TOKENS);
        }
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

/// The planner's health for this process.
static PLANNER_HEALTH: PlannerHealth = PlannerHealth::new();

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

impl PlannerTurn<'static> {
    pub(in super::super) fn new() -> Self {
        Self::with_health(&PLANNER_HEALTH)
    }
}

impl Default for PlannerTurn<'static> {
    fn default() -> Self {
        Self::new()
    }
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
