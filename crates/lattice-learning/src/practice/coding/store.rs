//! The session store over the coding tables: the engine's records in, and the coding rows they are kept as.

use super::debrief::{debrief_of, event_of, hint_of, CodingEventRow};
use super::problem::{CaseCompare, Problem, ProblemCase};
use super::records::{
    self as stored, CodingOp, FinishWrite, NewCodingSession, RunFinished, SessionBundle, StoredRun,
};
use super::session::evidence::{hints_of, last_draft_seen, result_of};
use super::session::view::settings_used;
use crate::practical_repository::LearningPracticalRepository;
use crate::practice::kinds::program::parts::line_of;
use crate::practice::kinds::program::shape::{
    case_of, check_status_of, coding_scope_of, exercise_of, files_of, piece_of,
    result_of as case_result_of, run_detail_of, scope_of, verdict_of,
};
use crate::practice::session::policy::Cue;
use crate::practice::session::ports::{At, CheckFinished, SessionStore};
use crate::practice::session::record::{
    Applied, CheckedCase, Finish, Help, NewHint, NewSession, Op, Session, SessionCheck,
    SessionLearnerCase, SessionRow, StageMove,
};
use async_trait::async_trait;
use lattice_core::domain::learning::practice::work::{CheckReport, Piece, Setting, Stage};
use lattice_core::shared::error::Result;
use serde_json::json;

#[cfg(test)]
mod tests;

/// The session store over the coding tables of `repo`.
pub struct CodingStore<'a> {
    pub repo: &'a LearningPracticalRepository,
}

fn coding_op<'a>(op: &Op<'a>) -> CodingOp<'a> {
    CodingOp {
        operation_id: op.operation_id,
        kind: op.command,
        payload_hash: op.payload_hash,
    }
}

fn applied<T, U>(written: stored::Applied<T>, make: impl FnOnce(T) -> U) -> Applied<U> {
    match written {
        stored::Applied::Done(done) => Applied::Done(make(done)),
        stored::Applied::Replayed => Applied::Replayed,
    }
}

fn check_of(run: &StoredRun, compare: &CaseCompare) -> Result<SessionCheck> {
    let results = run
        .cases
        .iter()
        .map(|detail| case_result_of(&result_of(detail)))
        .collect::<Result<Vec<_>>>()?;
    let cases = run
        .cases
        .iter()
        .map(|detail| {
            Ok(CheckedCase {
                case: case_of(&ProblemCase {
                    id: detail.case_id.clone(),
                    category: detail.category.clone(),
                    args: detail.args.clone().unwrap_or_default(),
                    expected: detail.expected.clone(),
                    compare: compare.clone(),
                    visibility: detail.visibility,
                    scale: detail.scale,
                    digest: None,
                })?,
                own: detail.mine,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(SessionCheck {
        id: run.run_id.clone(),
        scope: scope_of(run.scope),
        piece: piece_of(&run.files)?,
        report: CheckReport {
            results,
            marks: Vec::new(),
            detail: run_detail_of(&run.stdout, &run.stderr)?,
        },
        at: run.created_at,
        status: check_status_of(run.status),
        cases,
    })
}

fn learner_case_of(example: &stored::StoredExample) -> SessionLearnerCase {
    SessionLearnerCase {
        ordinal: example.ordinal,
        body: json!({"args": example.args, "expected": example.expected}),
        verdict: verdict_of(example.verdict),
        catches: example.catches.clone(),
    }
}

/// The session the engine reads, from what the tables hold.
pub fn session_of(bundle: &SessionBundle) -> Result<Session> {
    let row = &bundle.row;
    let compare = bundle.problem.default_compare();
    let mut exercise = exercise_of(&bundle.problem)?;
    exercise.kind = row.kind.clone();
    Ok(Session {
        row: SessionRow {
            id: row.id.clone(),
            program_id: row.program_id.clone(),
            activity_id: row.activity_id.clone(),
            form: row.form,
            setting: row.setting,
            stage: row.stage,
            budget_seconds: row.budget_seconds,
            active_ms: row.active_ms,
            opened_at: row.opened_at,
            draft: piece_of(&row.draft)?,
            pending_cue: row.pending_cue.clone(),
            submitted_at: row.submitted_at,
            solution_seen_at: row.solution_seen_at,
            finished_at: row.finished_at,
            revision: row.revision,
            created_at: row.created_at,
        },
        activity_revision: bundle.activity_revision,
        exercise,
        settings: settings_used(bundle)?,
        turns: bundle.turns.clone(),
        checks: bundle
            .runs
            .iter()
            .map(|run| check_of(run, &compare))
            .collect::<Result<Vec<_>>>()?,
        learner_cases: bundle.examples.iter().map(learner_case_of).collect(),
        hints: hints_of(bundle)?.iter().map(hint_of).collect(),
        events: bundle
            .events
            .iter()
            .map(|event| {
                event_of(&CodingEventRow {
                    ordinal: event.ordinal,
                    kind: event.kind.clone(),
                    payload: event.payload.clone(),
                    at: event.at,
                })
            })
            .collect(),
        draft_seen: last_draft_seen(bundle)?
            .map(|files| piece_of(&files))
            .transpose()?,
    })
}

/// The check a stored run is, with the learner's cases compared as `compare` says.
pub fn run_as_check(run: &StoredRun, compare: &CaseCompare) -> Result<SessionCheck> {
    check_of(run, compare)
}

#[async_trait]
impl SessionStore for CodingStore<'_> {
    async fn session(&self, program_id: &str, session_id: &str) -> Result<Session> {
        session_of(&self.repo.coding_session(program_id, session_id).await?)
    }

    async fn operation_seen(
        &self,
        program_id: &str,
        session_id: &str,
        op: &Op<'_>,
    ) -> Result<bool> {
        self.repo
            .coding_operation_seen(program_id, session_id, &coding_op(op))
            .await
    }

    async fn create(&self, op: &Op<'_>, new: &NewSession<'_>, now: i64) -> Result<(String, bool)> {
        let default_budget = |problem: &Problem| (new.default_budget)(&exercise_of(problem)?);
        self.repo
            .create_coding_session(
                &coding_op(op),
                &NewCodingSession {
                    program_id: new.program_id,
                    activity_id: new.activity_id,
                    setting: new.setting,
                    budget_seconds: new.budget_seconds,
                    default_budget: &default_budget,
                },
                now,
            )
            .await
    }

    async fn move_stage(
        &self,
        at: &At<'_>,
        op: &Op<'_>,
        stage: Stage,
        expected_revision: i64,
        now: i64,
    ) -> Result<Applied<StageMove>> {
        let moved = self
            .repo
            .move_coding_stage(
                at.program_id,
                at.session_id,
                &coding_op(op),
                stage,
                expected_revision,
                now,
            )
            .await?;
        Ok(applied(moved, |moved| StageMove {
            from: moved.from,
            to: moved.to,
            skipped: moved.skipped,
        }))
    }

    async fn save_draft(&self, at: &At<'_>, draft: &Piece) -> Result<()> {
        self.repo
            .save_coding_draft(at.program_id, at.session_id, &files_of(draft)?)
            .await
    }

    async fn record_check(&self, at: &At<'_>, check: &CheckFinished<'_>, now: i64) -> Result<bool> {
        self.repo
            .record_coding_run(
                at.program_id,
                at.session_id,
                &RunFinished {
                    run_id: check.check_id,
                    scope: coding_scope_of(check.scope),
                    passed: check.passed,
                    total: check.total,
                    reference_actual: None,
                    submit: check.submit,
                },
                now,
            )
            .await
    }

    async fn offer_cue(&self, at: &At<'_>, cue: &Cue) -> Result<()> {
        self.repo
            .offer_coding_cue(at.program_id, at.session_id, cue)
            .await
            .map(|_| ())
    }

    async fn take_cue(&self, at: &At<'_>) -> Result<Option<Cue>> {
        Ok(self
            .repo
            .take_coding_cue(at.program_id, at.session_id)
            .await?)
    }

    async fn restore_cue(&self, at: &At<'_>, cue: &Cue) -> Result<()> {
        self.repo
            .restore_coding_cue(at.program_id, at.session_id, cue)
            .await
    }

    async fn record_reply(
        &self,
        at: &At<'_>,
        content: Option<&str>,
        model: &str,
        draft: &Piece,
        now: i64,
    ) -> Result<()> {
        self.repo
            .record_coding_reply(
                at.program_id,
                at.session_id,
                content,
                model,
                &files_of(draft)?,
                now,
            )
            .await
    }

    async fn append_turn(
        &self,
        at: &At<'_>,
        op: &Op<'_>,
        content: &str,
        draft: &Piece,
        expected_revision: i64,
        now: i64,
    ) -> Result<Applied<()>> {
        let written = self
            .repo
            .append_coding_turn(
                at.program_id,
                at.session_id,
                &coding_op(op),
                content,
                &files_of(draft)?,
                expected_revision,
                now,
            )
            .await?;
        Ok(applied(written, |()| ()))
    }

    async fn record_hint(
        &self,
        at: &At<'_>,
        op: &Op<'_>,
        hint: &NewHint<'_>,
        draft: &Piece,
        expected_revision: i64,
        now: i64,
    ) -> Result<()> {
        let mut path = None;
        let mut lines = Vec::new();
        for part in hint.parts {
            let (file, line) = line_of(part)?;
            path.get_or_insert(file);
            lines.push(line);
        }
        self.repo
            .record_coding_hint(
                at.program_id,
                at.session_id,
                &coding_op(op),
                &stored::NewHint {
                    rung: hint.rung,
                    text: hint.text,
                    reason: hint.reason,
                    lines: &lines,
                    path: path.as_deref(),
                    in_role: hint.in_role,
                    model: hint.model,
                },
                &files_of(draft)?,
                expected_revision,
                now,
            )
            .await
            .map(|_| ())
    }

    async fn change_setting(
        &self,
        at: &At<'_>,
        op: &Op<'_>,
        setting: Setting,
        default_budget_seconds: Option<u32>,
        expected_revision: i64,
        now: i64,
    ) -> Result<()> {
        self.repo
            .change_coding_setting(
                at.program_id,
                at.session_id,
                &coding_op(op),
                setting,
                default_budget_seconds,
                expected_revision,
                now,
            )
            .await
            .map(|_| ())
    }

    async fn reveal_solution(&self, at: &At<'_>, op: &Op<'_>, now: i64) -> Result<()> {
        self.repo
            .reveal_coding_solution(at.program_id, at.session_id, &coding_op(op), now)
            .await
            .map(|_| ())
    }

    async fn worked_since(&self, session_id: &str, since: i64) -> Result<bool> {
        self.repo.coding_worked_since(session_id, since).await
    }

    async fn finish(&self, at: &At<'_>, op: &Op<'_>, finish: &Finish<'_>, now: i64) -> Result<()> {
        let bundle = self
            .repo
            .coding_session(at.program_id, at.session_id)
            .await?;
        let debrief = debrief_of(finish.debrief.clone())?;
        let files = files_of(finish.draft)?;
        self.repo
            .finish_coding_session(
                at.program_id,
                at.session_id,
                &coding_op(op),
                &FinishWrite {
                    scorecard: &debrief.scorecard,
                    approach: &debrief.approach,
                    complexity: &debrief.complexity,
                    model: finish.model,
                    language: &bundle.problem.spec.language,
                    pattern_ids: &bundle.problem.spec.patterns,
                    rating: finish.rating,
                    files: &files,
                    passed: finish.passed,
                    total: finish.total,
                    help: match finish.help {
                        Help::Full => "full_help",
                        Help::HintsOnly => "hints_only",
                    },
                    hints: finish.hints,
                },
                now,
            )
            .await
            .map(|_| ())
    }
}
