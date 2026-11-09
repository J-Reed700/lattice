//! Coding session models and DTOs
//! (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md section 4).

use crate::practice::coding::debrief::CodingDebriefDto;
use crate::practice::coding::hints::{CheckedCodeHint, CodeHintRung};
use crate::practice::coding::problem::{Case, CodingTask, Setting, Signature, Subgoal};
use crate::practice::practical_dto::{
    LearningLabFile, LearningPracticalFileDto, LearningPracticalRunDto, LearningSimulationTurnDto,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CodingStage {
    Understand,
    Plan,
    Code,
    Verify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ExampleVerdict {
    Right,
    Wrong,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum RunScope {
    Examples,
    All,
    Input,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CodingExampleDto {
    pub ordinal: u32,
    pub args_json: String,
    pub expected_json: String,
    pub verdict: ExampleVerdict,
    /// Descriptions of misreadings this example catches (or count).
    pub catches: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PublicProblemDto {
    pub title: String,
    pub statement: String,
    pub signature: Signature,
    pub examples: Vec<Case>,
    pub constraints: Vec<String>,
    pub subgoals: Vec<Subgoal>,
    pub files: Vec<LearningPracticalFileDto>,
    /// Present only when `setting.discloses(Hidden::WorkedSolution, submitted)` is true.
    pub worked_solution: Option<String>,
    pub target_complexity: Option<String>,
    /// Total number of misreadings in the problem, so the learner sees e.g. "Covers 2 of 3 common misreadings".
    pub total_misreadings: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StageTimingDto {
    pub stage: CodingStage,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CodingSessionDto {
    pub id: String,
    pub program_id: String,
    pub activity_id: String,
    pub task: CodingTask,
    pub setting: Setting,
    pub stage: CodingStage,
    pub budget_seconds: Option<u32>,
    pub active_ms: u64,
    pub opened_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub revision: i64,
    pub problem: PublicProblemDto,
    pub draft_files: Vec<LearningLabFile>,
    pub examples: Vec<CodingExampleDto>,
    pub hints: Vec<CheckedCodeHint>,
    pub available_next_rung: Option<CodeHintRung>,
    pub turns: Vec<LearningSimulationTurnDto>,
    pub runs: Vec<LearningPracticalRunDto>,
    pub stage_timings: Vec<StageTimingDto>,
    pub debrief: Option<CodingDebriefDto>,
}
