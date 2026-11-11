//! What a session shows the counterpart, which cases a check runs, and how a session went: for every kind of work.

use crate::practice::session::kind::Kind;
use crate::practice::session::observation::{
    CaseView, CheckView, ClockView, Coverage, HintView, LearnerCaseView, Observation,
};
use crate::practice::session::record::{Outcome, Session, SessionCheck};
use lattice_core::domain::learning::practice::work::{
    Case, CaseStatus, CaseVisibility, CheckScope, Hidden, LearnerCaseVerdict, Piece, Stage,
};
use lattice_core::shared::error::{AppError, Result};

#[cfg(test)]
mod tests;

fn stages_reached(session: &Session, learner_cases: bool) -> Vec<Stage> {
    let mut reached = Vec::new();
    for stage in session.row.form.stages(learner_cases) {
        reached.push(stage);
        if stage == session.row.stage {
            break;
        }
    }
    reached
}

fn shown_cases(session: &Session) -> Vec<Case> {
    session
        .exercise
        .cases
        .iter()
        .filter(|case| case.visibility == CaseVisibility::Shown)
        .cloned()
        .collect()
}

fn check_view(check: &SessionCheck, failing_open: bool) -> CheckView {
    CheckView {
        id: check.id.clone(),
        scope: check.scope,
        status: check.status,
        cases: check
            .cases
            .iter()
            .map(|checked| CaseView {
                open: failing_open
                    || checked.own
                    || checked.case.visibility == CaseVisibility::Shown,
                result: check
                    .report
                    .results
                    .iter()
                    .find(|result| result.case_id == checked.case.id)
                    .cloned(),
                case: checked.case.clone(),
            })
            .collect(),
        detail: check.report.detail.clone(),
    }
}

fn coverage(session: &Session, uncaught_shown: bool) -> Coverage {
    let caught: Vec<&str> = session
        .learner_cases
        .iter()
        .filter(|case| case.verdict == LearnerCaseVerdict::Right)
        .flat_map(|case| case.catches.iter().map(String::as_str))
        .collect();
    let missed: Vec<_> = session
        .exercise
        .misreadings
        .iter()
        .filter(|misreading| !caught.contains(&misreading.id.as_str()))
        .collect();
    let total = session.exercise.misreadings.len();
    Coverage {
        caught: u32::try_from(total.saturating_sub(missed.len())).unwrap_or(u32::MAX),
        total: u32::try_from(total).unwrap_or(u32::MAX),
        uncaught: missed
            .first()
            .filter(|_| uncaught_shown)
            .map(|misreading| misreading.description.clone()),
    }
}

/// What the counterpart may read: what the learner sees in `session`, cut by the setting, with `piece` as the draft
/// and the changes since `seen`.
pub fn observe(
    kind: &Kind,
    session: &Session,
    piece: &Piece,
    seen: Option<&Piece>,
) -> Result<Observation> {
    let row = &session.row;
    let finished = row.finished_at.is_some();
    let submitted = row.submitted_at.is_some();
    let discloses = |hidden: Hidden| finished || row.setting.discloses(hidden, submitted);
    let failing_open = discloses(Hidden::FailingCase);
    Ok(Observation {
        session_id: row.id.clone(),
        form: row.form,
        setting: row.setting,
        stage: row.stage,
        stages_reached: stages_reached(session, kind.learner_cases.is_some()),
        brief: session.exercise.brief.clone(),
        shown: shown_cases(session),
        clock: ClockView {
            active_ms: row.active_ms,
            budget_seconds: row.budget_seconds,
        },
        piece: piece.clone(),
        has_baseline: seen.is_some(),
        changes: seen
            .map(|seen| kind.diff.changes(&session.exercise.brief, seen, piece))
            .transpose()?
            .unwrap_or_default(),
        last_check: session
            .checks
            .last()
            .map(|check| check_view(check, failing_open)),
        learner_cases: session
            .learner_cases
            .iter()
            .map(|case| LearnerCaseView {
                ordinal: case.ordinal,
                body: case.body.clone(),
                verdict: case.verdict,
            })
            .collect(),
        coverage: coverage(session, discloses(Hidden::UncaughtMisreading)),
        hints: session
            .hints
            .iter()
            .map(|hint| HintView {
                rung: hint.rung,
                text: hint.text.clone(),
                check_id: hint.check_id.clone(),
            })
            .collect(),
        turns: session.turns.clone(),
    })
}

/// The cases a check of `scope` runs, and the ids of those the learner wrote. Shown: the cases shown with the brief
/// and each right case of the learner's. All: every case. Own is a kind's own command and is refused.
pub fn cases_for(
    kind: &Kind,
    session: &Session,
    scope: CheckScope,
) -> Result<(Vec<Case>, Vec<String>)> {
    match scope {
        CheckScope::All => Ok((session.exercise.cases.clone(), Vec::new())),
        CheckScope::Shown => {
            let mut cases = shown_cases(session);
            let mut own = Vec::new();
            if let Some(learner_cases) = &kind.learner_cases {
                for case in session
                    .learner_cases
                    .iter()
                    .filter(|case| case.verdict == LearnerCaseVerdict::Right)
                {
                    let made = learner_cases.case(&session.exercise, case)?;
                    own.push(made.id.clone());
                    cases.push(made);
                }
            }
            Ok((cases, own))
        }
        CheckScope::Own => Err(AppError::InvalidInput(
            "A check of the learner's own input is a command of the kind.".into(),
        )),
    }
}

fn last_of_every_case(session: &Session) -> Option<&SessionCheck> {
    session
        .checks
        .iter()
        .rev()
        .find(|check| check.scope == CheckScope::All)
}

/// How the session went, for the schedules of what it practised.
pub fn outcome(session: &Session) -> Outcome {
    let first_pass = session
        .checks
        .iter()
        .filter(|check| check.scope == CheckScope::All && check.report.all_passed())
        .map(|check| check.at)
        .min();
    Outcome {
        all_passed: last_of_every_case(session).is_some_and(|check| check.report.all_passed()),
        highest_rung: session.hints.iter().map(|hint| hint.rung).max(),
        solution_seen_before_passing: session
            .row
            .solution_seen_at
            .is_some_and(|seen| first_pass.is_none_or(|pass| seen < pass)),
    }
}

/// The cases passed and run on the last check of every case.
pub fn submit_counts(session: &Session) -> (u32, u32) {
    last_of_every_case(session).map_or((0, 0), |check| {
        let passed = check
            .report
            .results
            .iter()
            .filter(|result| result.status == CaseStatus::Passed)
            .count();
        (
            u32::try_from(passed).unwrap_or(u32::MAX),
            u32::try_from(check.report.results.len()).unwrap_or(u32::MAX),
        )
    })
}
