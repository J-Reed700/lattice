//! The debrief of a coding session: facts counted from what happened, and a scorecard the model writes and the app
//! checks against the session's own data (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md 4.8).

use super::dto::{
    stages_of, CodeHintDto, CodeHintRung, CodingExampleDto, CodingRunScope, CodingStage,
    DebriefFactsDto, RungCountDto, StageTimeDto,
};
use super::problem::{CaseResult, CaseStatus, CodingSetting, CodingTask, Problem};
use crate::lab_runtime::LearningLabFile;
use crate::lessons::structured_call::{complete_checked_request, reask};
use crate::model_call;
use crate::practice::practical_dto::{LearningSimulationSpeaker, LearningSimulationTurnDto};
use async_trait::async_trait;
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::grounded_generation::GroundedRequest;
use lattice_core::domain::learning::practice::{
    Scorecard, ScorecardEvidence, ScorecardRow, ScoreStatus,
};
use lattice_core::shared::error::{AppError, Result};
use serde::Deserialize;
use serde_json::{json, Value};

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

fn not_built() -> AppError {
    AppError::InvalidState("not built yet".into())
}

/// What the session shows beside the scores, counted rather than judged.
pub fn facts(_evidence: &SessionEvidence) -> DebriefFactsDto {
    DebriefFactsDto {
        stage_ms: Vec::new(),
        runs: 0,
        submits: 0,
        hints: Vec::new(),
        settings: Vec::new(),
    }
}

#[async_trait]
impl Debriefer for CodingDebriefer<'_> {
    async fn debrief(&self, _evidence: &SessionEvidence) -> Result<Debrief> {
        Err(not_built())
    }
}

#[cfg(test)]
mod tests;
