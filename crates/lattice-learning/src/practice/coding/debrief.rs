//! The debrief of a coding session: the engine's debrief, over a program's roles, in the vocabulary the rest of the
//! coding code speaks (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md 4.8).

use super::dto::{
    CodeHintDto, CodingExampleDto, CodingRunScope, CodingStage, DebriefFactsDto, RungCountDto,
    StageTimeDto,
};
use super::problem::{CaseResult, CodingSetting, CodingTask, Problem};
use crate::lab_runtime::LearningLabFile;
use crate::practice::kinds::program::kind;
use crate::practice::kinds::program::parts::line_of;
use crate::practice::kinds::program::shape::{
    coding_rung_of, coding_setting_of, coding_stage_of, exercise_of, form_of, piece_of, result_of,
    rung_of, scope_of, setting_of, stage_of, verdict_of,
};
use crate::practice::kinds::program::toolchain::NoToolchains;
use crate::practice::practical_dto::LearningSimulationTurnDto;
use crate::practice::session::debrief::{self as engine, Evidence};
use crate::practice::session::kind::Kind;
use crate::practice::session::record::{
    Counted, DebriefSession, EventKind, SessionCheck, SessionEvent, SessionHint, SessionLearnerCase,
};
use async_trait::async_trait;
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::practice::work::CheckReport;
use lattice_core::domain::learning::practice::{Scorecard, ScorecardEvidence, ScorecardRow};
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};
use std::sync::Arc;

/// Everything the debrief reads about one session.
#[derive(Debug, Clone)]
pub struct SessionEvidence {
    pub problem: Problem,
    pub task: CodingTask,
    pub settings: Vec<CodingSetting>,
    pub turns: Vec<LearningSimulationTurnDto>,
    pub runs: Vec<EvidenceRun>,
    pub examples: Vec<CodingExampleDto>,
    pub hints: Vec<CodeHintDto>,
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

/// The Program kind as a reader of a finished session: nothing runs.
fn program_kind() -> Kind {
    kind(Arc::new(NoToolchains))
}

fn stage_in(payload: &Value, key: &str) -> Option<CodingStage> {
    payload
        .get(key)
        .and_then(|value| serde_json::from_value(value.clone()).ok())
}

fn event_of(row: &CodingEventRow) -> SessionEvent {
    let kind = match row.kind.as_str() {
        "opened" => EventKind::Opened,
        "closed" => EventKind::Closed,
        "stage" => EventKind::Moved(stage_in(&row.payload, "to").map(stage_of)),
        _ => EventKind::Other,
    };
    SessionEvent {
        ordinal: row.ordinal,
        kind,
        at: row.at,
    }
}

fn hint_of(hint: &CodeHintDto) -> SessionHint {
    SessionHint {
        ordinal: hint.ordinal,
        rung: rung_of(hint.rung),
        text: hint.text.clone(),
        stage: stage_of(hint.stage),
    }
}

fn check_of(run: &EvidenceRun) -> Result<SessionCheck> {
    Ok(SessionCheck {
        id: run.run_id.clone(),
        scope: scope_of(run.scope),
        piece: piece_of(&run.files)?,
        report: CheckReport {
            results: run
                .results
                .iter()
                .map(result_of)
                .collect::<Result<Vec<_>>>()?,
            marks: Vec::new(),
            detail: Value::Null,
        },
        at: run.created_at,
    })
}

fn engine_session(evidence: &SessionEvidence) -> Result<DebriefSession> {
    Ok(DebriefSession {
        exercise: exercise_of(&evidence.problem)?,
        form: form_of(evidence.task),
        settings: evidence.settings.iter().copied().map(setting_of).collect(),
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
            })
            .collect(),
        hints: evidence.hints.iter().map(hint_of).collect(),
        events: evidence.events.iter().map(event_of).collect(),
        draft: piece_of(&evidence.draft)?,
    })
}

/// What the session shows beside the scores, counted rather than judged.
pub fn facts(evidence: &SessionEvidence) -> DebriefFactsDto {
    let settings: Vec<_> = evidence.settings.iter().copied().map(setting_of).collect();
    let hints: Vec<_> = evidence.hints.iter().map(hint_of).collect();
    let events: Vec<_> = evidence.events.iter().map(event_of).collect();
    let counted = Counted {
        form: form_of(evidence.task),
        settings: &settings,
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
                stage: coding_stage_of(stage),
                active_ms,
            })
            .collect(),
        runs: counts.checks,
        submits: counts.submits,
        hints: counts
            .hints
            .into_iter()
            .map(|(rung, count)| RungCountDto {
                rung: coding_rung_of(rung),
                count,
            })
            .collect(),
        settings: counts.settings.into_iter().map(coding_setting_of).collect(),
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

fn debrief_of(debrief: engine::Debrief) -> Result<Debrief> {
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
    engine::system(&program_kind(), form_of(CodingTask::Problem))
}

#[cfg(test)]
fn schema() -> Value {
    engine::schema(&program_kind(), form_of(CodingTask::Problem))
}

#[cfg(test)]
fn prompt(evidence: &SessionEvidence) -> Result<String> {
    engine::prompt(&program_kind(), &engine_session(evidence)?)
}

#[cfg(test)]
mod tests;
