//! What a practice session sends to the screen and takes from it, for every kind of work. What is particular to a
//! kind is in `work`, a value the kind's own screen role writes.

use crate::practice::practical_dto::LearningSimulationTurnDto;
use lattice_core::domain::learning::drills::PatternRating;
use lattice_core::domain::learning::practice::work::{
    CheckScope, CheckStatus, Difficulty, Form, Piece, Rung, Setting, Stage,
};
use lattice_core::domain::learning::practice::Scorecard;
use serde::{Deserialize, Serialize};

/// Why the screen reports the draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum DraftObservation {
    /// The learner stopped typing.
    Pause,
    /// Nothing has happened for a while.
    Idle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClockDto {
    /// Time the session was open on screen, closed spans only.
    pub active_ms: u64,
    /// When the open span began, while one is open.
    pub open_since: Option<i64>,
    pub budget_seconds: Option<u32>,
}

/// A stage of the session's form, under the label the counterpart also says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StageLabelDto {
    pub stage: Stage,
    pub label: String,
}

/// One check as every kind has it. What it found is in the kind's `work`, under the same id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PracticeCheckDto {
    pub check_id: String,
    pub scope: CheckScope,
    pub status: CheckStatus,
    pub passed: u32,
    pub total: u32,
    pub at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PracticeHintDto {
    pub ordinal: u32,
    pub rung: Rung,
    pub text: String,
    pub reason: String,
    pub stage: Stage,
    /// The last check made when the hint was given.
    pub check_id: Option<String>,
    /// Watched: given by the interviewer, in the thread.
    pub in_role: bool,
    pub at: i64,
}

/// How many of the exercise's misreadings the learner's right cases catch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MisreadingCoverageDto {
    pub caught: u32,
    pub total: u32,
    /// Coached: the description of one misreading not caught yet.
    pub uncaught: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MisreadingViewDto {
    pub description: String,
    pub caught: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PracticeSessionDto {
    pub session_id: String,
    pub program_id: String,
    pub activity_id: String,
    pub lesson_id: Option<String>,
    pub kind: String,
    pub title: String,
    pub difficulty: Difficulty,
    pub form: Form,
    pub setting: Setting,
    pub settings_used: Vec<Setting>,
    pub stage: Stage,
    /// The stages of the form, in order.
    pub stages: Vec<StageLabelDto>,
    pub clock: ClockDto,
    /// Oldest first.
    pub checks: Vec<PracticeCheckDto>,
    pub hints: Vec<PracticeHintDto>,
    pub next_rung: Rung,
    pub turns: Vec<LearningSimulationTurnDto>,
    pub cue_pending: bool,
    /// Some exactly when the kind has cases the learner writes.
    pub coverage: Option<MisreadingCoverageDto>,
    pub submitted: bool,
    pub solution_available: bool,
    /// The learner has the reference open.
    pub solution_shown: bool,
    pub finished: bool,
    pub revision: i64,
    /// What the kind's screen draws of the work.
    pub work: serde_json::Value,
    pub debrief: Option<PracticeDebriefDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PracticeSessionSummaryDto {
    pub session_id: String,
    pub activity_id: String,
    pub kind: String,
    pub title: String,
    pub form: Form,
    pub setting: Setting,
    pub stage: Stage,
    pub finished: bool,
    /// Cases passed on the last submit.
    pub passed: Option<u32>,
    pub total: Option<u32>,
    pub created_at: i64,
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PracticeDebriefDto {
    pub scorecard: Scorecard,
    pub facts: DebriefFactsDto,
    /// Empty when the kind has no cases the learner writes.
    pub misreadings: Vec<MisreadingViewDto>,
    pub techniques: Vec<TechniqueRatingDto>,
    pub model: String,
    /// What the kind's screen draws of the debrief.
    pub work: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DebriefFactsDto {
    pub stage_ms: Vec<StageTimeDto>,
    pub checks: u32,
    pub submits: u32,
    pub hints: Vec<RungCountDto>,
    pub settings: Vec<Setting>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StageTimeDto {
    pub stage: Stage,
    pub active_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RungCountDto {
    pub rung: Rung,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TechniqueRatingDto {
    pub technique_id: String,
    pub name: String,
    pub family: String,
    pub cue: String,
    pub rating: PatternRating,
    pub due_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum DueKind {
    CodingPatterns,
    DrillLesson,
    DrillPatterns,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DueItemDto {
    pub kind: DueKind,
    /// Never a pattern name: "3 problem patterns", or the lesson title.
    pub label: String,
    pub count: u32,
    pub due_at: i64,
    pub lesson_id: Option<String>,
    pub material: Option<String>,
}

/// Most overdue first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PracticeDueDto {
    pub items: Vec<DueItemDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StartPracticeSessionRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub activity_id: String,
    pub setting: Setting,
    pub budget_seconds: Option<u32>,
}

/// Names a session: get and respond.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PracticeSessionRequestDto {
    pub program_id: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ListPracticeSessionsRequestDto {
    pub program_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MovePracticeStageRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub session_id: String,
    pub stage: Stage,
    pub expected_revision: i64,
}

/// The engine's check runs the scopes `shown` and `all`; a kind's own commands run anything else.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CheckPracticePieceRequestDto {
    pub operation_id: String,
    pub check_id: String,
    pub program_id: String,
    pub session_id: String,
    pub scope: CheckScope,
    pub piece: Piece,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AskPracticeHintRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub session_id: String,
    pub piece: Piece,
    pub expected_revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SendPracticeTurnRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub session_id: String,
    pub content: String,
    pub spoken: bool,
    pub piece: Piece,
    pub expected_revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ObservePracticeDraftRequestDto {
    pub program_id: String,
    pub session_id: String,
    pub piece: Piece,
    pub reason: DraftObservation,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SetPracticeClockRequestDto {
    pub program_id: String,
    pub session_id: String,
    pub open: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SetPracticeSettingRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub session_id: String,
    pub setting: Setting,
    pub expected_revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RevealPracticeSolutionRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FinishPracticeSessionRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub session_id: String,
    pub piece: Piece,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PracticeDueRequestDto {
    pub program_id: String,
}
