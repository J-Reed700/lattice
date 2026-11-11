//! The debrief of a coding session: the engine's debrief, over a program's roles, in the vocabulary the rest of the
//! coding code speaks (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md 4.8).

use super::dto::{
    CodingExampleDto, CodingRunScope, DebriefFactsDto, PracticeHintDto, RungCountDto, StageTimeDto,
};
use super::harness::CaseHarness;
use super::problem::{CaseResult, Problem};
use crate::drills::runner::CodeRunner;
use crate::lab_runtime::LearningLabFile;
use crate::practice::kinds::program::kind;
use crate::practice::kinds::program::parts::line_of;
use crate::practice::kinds::program::shape::{
    exercise_of, piece_of, result_of, scope_of, verdict_of,
};
use crate::practice::kinds::program::toolchain::Toolchains;
use crate::practice::practical_dto::LearningSimulationTurnDto;
use crate::practice::session::debrief::{self as engine, Evidence};
use crate::practice::session::kind::Kind;
use crate::practice::session::record::{
    Counted, DebriefSession, EventKind, SessionCheck, SessionEvent, SessionHint, SessionLearnerCase,
};
use async_trait::async_trait;
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::practice::work::{CheckReport, CheckStatus};
use lattice_core::domain::learning::practice::work::{Form, Setting, Stage};
use lattice_core::domain::learning::practice::{Scorecard, ScorecardEvidence, ScorecardRow};
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};
use std::sync::Arc;

/// Everything the debrief reads about one session.
#[derive(Debug, Clone)]
pub struct SessionEvidence {
    pub problem: Problem,
    pub form: Form,
    pub settings: Vec<Setting>,
    pub turns: Vec<LearningSimulationTurnDto>,
    pub runs: Vec<EvidenceRun>,
    pub examples: Vec<CodingExampleDto>,
    pub hints: Vec<PracticeHintDto>,
    pub events: Vec<CodingEventRow>,
    pub draft: Vec<LearningLabFile>,
}

/// One run of the learner's code, with the files it ran and a result for each case.
#[derive(Debug, Clone)]
pub struct EvidenceRun {
    pub run_id: String,
    pub scope: CodingRunScope,
    pub files: Vec<LearningLabFile>,
    pub results: Vec<CaseResult>,
    pub created_at: i64,
}

/// A row of the session's event log.
#[derive(Debug, Clone)]
pub struct CodingEventRow {
    pub ordinal: u32,
    pub kind: String,
    pub payload: Value,
    pub at: i64,
}

/// What the model writes: the scorecard, and the approach and complexity of a strong answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Debrief {
    pub scorecard: Scorecard,
    pub approach: String,
    pub complexity: String,
}

#[async_trait]
pub trait Debriefer: Send + Sync {
    async fn debrief(&self, evidence: &SessionEvidence) -> Result<Debrief>;
}

pub struct CodingDebriefer<'a> {
    pub llm: &'a dyn LLMPort,
}

/// The toolchains of a reader of a finished session: nothing runs.
struct NothingRuns;

impl Toolchains for NothingRuns {
    fn runner(&self, language: &str) -> Result<Box<dyn CodeRunner>> {
        Err(AppError::InvalidState(format!(
            "A finished session runs no {language}."
        )))
    }

    fn harness(&self, language: &str) -> Result<Box<dyn CaseHarness>> {
        Err(AppError::InvalidState(format!(
            "A finished session runs no {language}."
        )))
    }
}

/// The Program kind as this adapter uses it, until the session service holds the kind it runs with.
fn program_kind() -> Kind {
    kind(Arc::new(NothingRuns))
}

fn stage_in(payload: &Value, key: &str) -> Option<Stage> {
    payload
        .get(key)
        .and_then(|value| serde_json::from_value(value.clone()).ok())
}

pub(super) fn event_of(row: &CodingEventRow) -> SessionEvent {
    let kind = match row.kind.as_str() {
        "opened" => EventKind::Opened,
        "closed" => EventKind::Closed,
        "stage" => EventKind::Moved(stage_in(&row.payload, "to")),
        _ => EventKind::Other,
    };
    SessionEvent {
        ordinal: row.ordinal,
        kind,
        at: row.at,
    }
}

pub(super) fn hint_of(hint: &PracticeHintDto) -> SessionHint {
    SessionHint {
        ordinal: hint.ordinal,
        rung: hint.rung,
        text: hint.text.clone(),
        stage: hint.stage,
        check_id: hint.run_id.clone(),
    }
}

fn check_of(run: &EvidenceRun) -> Result<SessionCheck> {
    let report = CheckReport {
        results: run
            .results
            .iter()
            .map(result_of)
            .collect::<Result<Vec<_>>>()?,
        marks: Vec::new(),
        detail: Value::Null,
    };
    Ok(SessionCheck {
        id: run.run_id.clone(),
        scope: scope_of(run.scope),
        piece: piece_of(&run.files)?,
        status: if report.all_passed() {
            CheckStatus::Passed
        } else {
            CheckStatus::Failed
        },
        report,
        at: run.created_at,
        cases: Vec::new(),
    })
}

fn engine_session(evidence: &SessionEvidence) -> Result<DebriefSession> {
    Ok(DebriefSession {
        exercise: exercise_of(&evidence.problem)?,
        form: evidence.form,
        settings: evidence.settings.clone(),
        turns: evidence.turns.clone(),
        checks: evidence
            .runs
            .iter()
            .map(check_of)
            .collect::<Result<Vec<_>>>()?,
        learner_cases: evidence
            .examples
            .iter()
            .map(|example| SessionLearnerCase {
                ordinal: example.ordinal,
                body: json!({"args": example.args, "expected": example.expected}),
                verdict: verdict_of(example.verdict),
                catches: Vec::new(),
            })
            .collect(),
        hints: evidence.hints.iter().map(hint_of).collect(),
        events: evidence.events.iter().map(event_of).collect(),
        draft: piece_of(&evidence.draft)?,
    })
}

/// What the session shows beside the scores, counted rather than judged.
pub fn facts(evidence: &SessionEvidence) -> DebriefFactsDto {
    let settings = &evidence.settings;
    let hints: Vec<_> = evidence.hints.iter().map(hint_of).collect();
    let events: Vec<_> = evidence.events.iter().map(event_of).collect();
    let counted = Counted {
        form: evidence.form,
        settings,
        scopes: evidence
            .runs
            .iter()
            .map(|run| scope_of(run.scope))
            .collect(),
        hints: &hints,
        events: &events,
    };
    let counts = engine::facts(&program_kind(), &counted);
    DebriefFactsDto {
        stage_ms: counts
            .stage_ms
            .into_iter()
            .map(|(stage, active_ms)| StageTimeDto {
                stage: stage,
                active_ms,
            })
            .collect(),
        runs: counts.checks,
        submits: counts.submits,
        hints: counts
            .hints
            .into_iter()
            .map(|(rung, count)| RungCountDto { rung: rung, count })
            .collect(),
        settings: counts.settings,
    }
}

/// Cited lines are the first and the last line of the parts the engine kept.
fn scorecard_evidence_of(evidence: Evidence) -> Result<ScorecardEvidence> {
    Ok(match evidence {
        Evidence::Quote { turn_id, text } => ScorecardEvidence::Quote { turn_id, text },
        Evidence::Part { check_id, parts } => {
            let (Some(first), Some(last)) = (parts.first(), parts.last()) else {
                return Err(AppError::InvalidState(
                    "Evidence of lines names no line.".into(),
                ));
            };
            let (path, start) = line_of(first)?;
            let (_, end) = line_of(last)?;
            ScorecardEvidence::Lines {
                run_id: check_id,
                path,
                start,
                end,
            }
        }
        Evidence::Check { check_id } => ScorecardEvidence::Run { run_id: check_id },
        Evidence::LearnerCase { ordinal } => ScorecardEvidence::Example { ordinal },
        Evidence::Hint { ordinal } => ScorecardEvidence::Hint { ordinal },
    })
}

fn stated(debrief: &engine::Debrief, key: &str) -> Result<String> {
    debrief
        .stated
        .iter()
        .find(|(known, _)| known == key)
        .map(|(_, text)| text.clone())
        .ok_or_else(|| AppError::InvalidState(format!("The debrief states no {key}.")))
}

pub(super) fn debrief_of(debrief: engine::Debrief) -> Result<Debrief> {
    let (approach, complexity) = (
        stated(&debrief, "approach")?,
        stated(&debrief, "complexity")?,
    );
    let rows = debrief
        .rows
        .into_iter()
        .map(|row| {
            Ok(ScorecardRow {
                criterion_id: row.criterion_id,
                title: row.title,
                status: row.status,
                points: row.points,
                max_points: row.max_points,
                evidence: row
                    .evidence
                    .into_iter()
                    .map(scorecard_evidence_of)
                    .collect::<Result<Vec<_>>>()?,
                feedback: row.feedback,
                next_step: row.next_step,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let scorecard = Scorecard { rows };
    scorecard.validate()?;
    Ok(Debrief {
        scorecard,
        approach,
        complexity,
    })
}

#[async_trait]
impl Debriefer for CodingDebriefer<'_> {
    async fn debrief(&self, evidence: &SessionEvidence) -> Result<Debrief> {
        let session = engine_session(evidence)?;
        debrief_of(engine::debrief(self.llm, &program_kind(), &session).await?)
    }
}

#[cfg(test)]
fn system() -> String {
    engine::system(&program_kind(), Form::Build)
}

#[cfg(test)]
fn schema() -> Value {
    engine::schema(&program_kind(), Form::Build)
}

#[cfg(test)]
fn prompt(evidence: &SessionEvidence) -> Result<String> {
    engine::prompt(&program_kind(), &engine_session(evidence)?)
}

#[cfg(test)]
mod tests;
