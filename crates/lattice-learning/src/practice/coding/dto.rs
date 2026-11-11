//! What a coding session sends to the screen and takes from it. Everything here is already cut by the session's
//! setting: nothing a setting keeps back is in a view.

use super::problem::{CaseStatus, CaseVisibility, ProblemDifficulty, ProblemSignature};
use crate::lab_runtime::LearningLabFile;
use crate::practical_dto::{LearningPracticalRunStatus, LearningSimulationTurnDto};
use lattice_core::domain::learning::drills::PatternRating;
use lattice_core::domain::learning::practice::work::{Form, Rung, Setting, Stage};
use lattice_core::domain::learning::practice::Scorecard;
use serde::{Deserialize, Serialize};

/// Which cases a run covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CodingRunScope {
    /// The statement's examples and the learner's right examples.
    Shown,
    /// Every case of the problem: a submit.
    All,
    /// One input of the learner's choosing.
    Input,
}

impl CodingRunScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shown => "shown",
            Self::All => "all",
            Self::Input => "input",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        [Self::Shown, Self::All, Self::Input]
            .into_iter()
            .find(|scope| scope.as_str() == name)
    }
}

/// What the reference said about a learner's example.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ExampleVerdict {
    Right,
    Wrong,
    Invalid,
}

impl ExampleVerdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Right => "right",
            Self::Wrong => "wrong",
            Self::Invalid => "invalid",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        [Self::Right, Self::Wrong, Self::Invalid]
            .into_iter()
            .find(|verdict| verdict.as_str() == name)
    }
}

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

/// One case as the learner sees it. A field is None when the setting keeps it back, or the case has none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CaseViewDto {
    pub case_id: String,
    pub category: String,
    pub visibility: CaseVisibility,
    /// A learner example run as a case.
    pub mine: bool,
    pub scale: bool,
    pub status: CaseStatus,
    pub args: Option<Vec<String>>,
    pub expected: Option<String>,
    pub actual: Option<String>,
    pub printed: Option<String>,
    pub error: Option<String>,
    pub elapsed_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PracticeCheckDto {
    pub run_id: String,
    pub scope: CodingRunScope,
    pub status: LearningPracticalRunStatus,
    /// In case order; the screen lists failures first.
    pub cases: Vec<CaseViewDto>,
    /// What the program printed outside any case.
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: Option<i64>,
    /// An input run in Coached: what the reference returns.
    pub reference_actual: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CodingExampleDto {
    pub ordinal: u32,
    pub args: Vec<String>,
    pub expected: String,
    pub verdict: ExampleVerdict,
    /// Coached, verdict Wrong: what the problem expects.
    pub expects: Option<String>,
    pub why_invalid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExampleCoverageDto {
    /// Misreadings caught by right examples.
    pub caught: u32,
    /// Misreadings in the problem.
    pub total: u32,
    /// Coached: the description of one misreading not caught yet.
    pub uncaught: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PracticeHintDto {
    pub ordinal: u32,
    pub rung: Rung,
    pub text: String,
    pub reason: String,
    /// Rung Point: 1-based lines of `path` in the draft the hint was written for.
    pub lines: Vec<u32>,
    pub path: Option<String>,
    pub stage: Stage,
    pub run_id: Option<String>,
    /// Watched: given by the interviewer, in the thread.
    pub in_role: bool,
    pub at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PublicProblemDto {
    pub title: String,
    pub statement: String,
    pub signature: ProblemSignature,
    pub constraints: Vec<String>,
    pub difficulty: ProblemDifficulty,
    pub language: String,
    /// The statement's examples, in full.
    pub examples: Vec<CaseViewDto>,
    pub starter: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PracticeSessionDto {
    pub session_id: String,
    pub program_id: String,
    pub activity_id: String,
    pub lesson_id: Option<String>,
    pub form: Form,
    pub setting: Setting,
    pub settings_used: Vec<Setting>,
    pub stage: Stage,
    pub stages: Vec<Stage>,
    pub problem: PublicProblemDto,
    pub clock: ClockDto,
    pub examples: Vec<CodingExampleDto>,
    pub coverage: ExampleCoverageDto,
    /// Oldest first.
    pub runs: Vec<PracticeCheckDto>,
    pub hints: Vec<PracticeHintDto>,
    pub next_rung: Rung,
    pub turns: Vec<LearningSimulationTurnDto>,
    pub cue_pending: bool,
    pub draft: Vec<LearningLabFile>,
    pub submitted: bool,
    pub solution_available: bool,
    /// Some once revealed, or in the debrief.
    pub worked_solution: Option<String>,
    pub debrief: Option<PracticeDebriefDto>,
    pub finished: bool,
    pub revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PracticeSessionSummaryDto {
    pub session_id: String,
    pub activity_id: String,
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
    pub strong_answer: StrongAnswerDto,
    pub patterns: Vec<PatternRatingDto>,
    /// The last submit, nothing kept back.
    pub cases: Vec<CaseViewDto>,
    pub misreadings: Vec<MisreadingViewDto>,
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DebriefFactsDto {
    pub stage_ms: Vec<StageTimeDto>,
    pub runs: u32,
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
pub struct StrongAnswerDto {
    pub approach: String,
    pub complexity: String,
    pub reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MisreadingViewDto {
    pub description: String,
    pub caught: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PatternRatingDto {
    pub pattern_id: String,
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
pub struct StartPracticeBuildRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub lesson_id: Option<String>,
    pub form: Form,
    pub language: String,
    pub difficulty: ProblemDifficulty,
    pub focus: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StartPracticeBuildResultDto {
    pub job_id: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CheckCodingExampleRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub session_id: String,
    /// Some edits that example.
    pub ordinal: Option<u32>,
    pub args: Vec<String>,
    pub expected: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RemoveCodingExampleRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub session_id: String,
    pub ordinal: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CheckPracticePieceRequestDto {
    pub operation_id: String,
    pub run_id: String,
    pub program_id: String,
    pub session_id: String,
    pub scope: CodingRunScope,
    pub files: Vec<LearningLabFile>,
    /// Scope Input: one literal per parameter.
    pub input: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AskPracticeHintRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub session_id: String,
    pub files: Vec<LearningLabFile>,
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
    pub files: Vec<LearningLabFile>,
    pub expected_revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ObservePracticeDraftRequestDto {
    pub program_id: String,
    pub session_id: String,
    pub files: Vec<LearningLabFile>,
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
    pub files: Vec<LearningLabFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PracticeDueRequestDto {
    pub program_id: String,
}
