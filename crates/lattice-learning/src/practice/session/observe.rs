//! What a session shows the counterpart, which cases a check runs, and how a session went: for every kind of work.

use crate::practice::session::kind::Kind;
use crate::practice::session::observation::{ClockView, Coverage, Observation};
use crate::practice::session::record::{Outcome, Session};
use lattice_core::domain::learning::practice::work::{Case, CheckScope, Piece};
use lattice_core::shared::error::Result;

#[cfg(test)]
mod tests;

/// What the counterpart may read: what the learner sees in `session`, cut by the setting, with `piece` as the draft
/// and the changes since `seen`.
pub fn observe(
    _kind: &Kind,
    session: &Session,
    piece: &Piece,
    _seen: Option<&Piece>,
) -> Result<Observation> {
    Ok(Observation {
        session_id: session.row.id.clone(),
        form: session.row.form,
        setting: session.row.setting,
        stage: session.row.stage,
        stages_reached: Vec::new(),
        brief: session.exercise.brief.clone(),
        shown: Vec::new(),
        clock: ClockView {
            active_ms: 0,
            budget_seconds: None,
        },
        piece: piece.clone(),
        has_baseline: false,
        changes: Vec::new(),
        last_check: None,
        learner_cases: Vec::new(),
        coverage: Coverage {
            caught: 0,
            total: 0,
            uncaught: None,
        },
        hints: Vec::new(),
        turns: Vec::new(),
    })
}

/// The cases a check of `scope` runs, and the ids of those the learner wrote. Shown: the cases shown with the brief
/// and each right case of the learner's. All: every case. Own is a kind's own command and is refused.
pub fn cases_for(
    _kind: &Kind,
    _session: &Session,
    _scope: CheckScope,
) -> Result<(Vec<Case>, Vec<String>)> {
    Ok((Vec::new(), Vec::new()))
}

/// How the session went, for the schedules of what it practised.
pub fn outcome(_session: &Session) -> Outcome {
    Outcome {
        all_passed: false,
        highest_rung: None,
        solution_seen_before_passing: false,
    }
}

/// The cases passed and run on the last check of every case.
pub fn submit_counts(_session: &Session) -> (u32, u32) {
    (0, 0)
}
