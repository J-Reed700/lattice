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

use lattice_core::application::ports::llm_port::PrefillTiming;
use lattice_core::application::services::prompt_pace::PromptPace;
use tokio::time::Instant;

use super::PLANNER_TIMEOUT;

/// The prompt, system prompt included, the planner is sent before this machine
/// has answered once: what the reference machine (Qwen3.5-9B Q4_K_M on Metal,
/// about 300 prompt tokens a second) prefills in about nine seconds. From the
/// first answer on, the size comes from the prefill speed the server reports
/// (see [`PlannerHealth::answered`]).
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

/// What the planner has recently managed. One per chat runtime, shared by
/// every turn it runs, because "how much can this model prefill in time" is a
/// property of the machine and the model, not of any one conversation.
pub struct PlannerHealth {
    pace: PromptPace,
    stood_down_until: Mutex<Option<Instant>>,
}

impl Default for PlannerHealth {
    fn default() -> Self {
        Self::new()
    }
}

impl PlannerHealth {
    pub(crate) fn new() -> Self {
        Self {
            // The user waits on the plan: it reads for a small share of its
            // timeout, not half of it, twice a turn.
            pace: PromptPace::starting_at(
                PLANNER_TIMEOUT,
                PLANNER_PROMPT_TOKENS,
                PLANNER_PROMPT_FLOOR,
            )
            .on_critical_path(),
            stood_down_until: Mutex::new(None),
        }
    }

    fn stood_down_until(&self) -> std::sync::MutexGuard<'_, Option<Instant>> {
        self.stood_down_until
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The prompt size, in tokens, the next attempt on `model` should aim for.
    pub(super) fn prompt_tokens(&self, model: &str) -> usize {
        self.pace.tokens(model)
    }

    /// How much longer the planner is standing down, if it is.
    pub(super) fn standing_down(&self) -> Option<Duration> {
        let until = (*self.stood_down_until())?;
        until.checked_duration_since(Instant::now())
    }

    /// An answer in time ends any stand-down and measures the planner from the
    /// server's own prefill timing: the next prompt is what that pace reads in
    /// half of [`PLANNER_TIMEOUT`], growing at most a doubling per answer. The
    /// time to the answer is not used: it also holds the plan's writing and
    /// any queue, and read as prefill it shrank a 9B planner's prompt from
    /// 2,560 tokens to the floor after answers that were all in time. An answer
    /// in time never shrinks the next prompt.
    pub(super) fn answered(&self, model: &str, prefill: Option<PrefillTiming>, sent_tokens: usize) {
        *self.stood_down_until() = None;
        self.pace.answered(model, prefill, sent_tokens);
    }

    /// A timeout halves the next attempt's prompt. Only a timeout at the floor
    /// stands the planner down, because there is nothing smaller left to try.
    pub(super) fn timed_out(&self, model: &str, sent_tokens: usize) {
        let at_floor = self.prompt_tokens(model) <= PLANNER_PROMPT_FLOOR;
        self.pace.timed_out(model, sent_tokens);
        if at_floor {
            *self.stood_down_until() = Some(Instant::now() + PLANNER_STAND_DOWN);
        }
    }

    /// The server refused the prompt as larger than its window. The prompt is
    /// counted against the window the port reports, so this means the report
    /// was wrong. Halving finds the real window within a few turns; there is
    /// no stand-down, since a refusal costs no time.
    pub(super) fn overflowed(&self, model: &str, sent_tokens: usize) {
        self.pace.overflowed(model, sent_tokens);
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

    pub(super) fn record_timeout(&self, model: &str, sent_tokens: usize) {
        self.timed_out.store(true, Ordering::Relaxed);
        self.health.timed_out(model, sent_tokens);
    }

    /// Tokens of conversation the planner's next prompt on `model` carries:
    /// its share of the prompt that model reads in time.
    pub(in super::super) fn history_tokens(&self, model: &str) -> usize {
        super::prompt::history_tokens(self.health.prompt_tokens(model))
    }

    pub(super) fn candidate_ids(&self) -> &[String] {
        self.candidates.as_deref().unwrap_or_default()
    }
}
