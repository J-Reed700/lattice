//! What the Program kind sends to the screen in a session's `work`, and what its own commands take.

use crate::lab_runtime::LearningLabFile;
use crate::practical_dto::LearningPracticalRunStatus;
use crate::practice::coding::problem::{
    CaseStatus, CaseVisibility, ProblemDifficulty, ProblemSignature,
};
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StrongAnswerDto {
    pub approach: String,
    pub complexity: String,
    pub reference: String,
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
pub struct RunCodingInputRequestDto {
    pub operation_id: String,
    pub run_id: String,
    pub program_id: String,
    pub session_id: String,
    pub files: Vec<LearningLabFile>,
    /// One literal per parameter.
    pub input: Vec<String>,
}

/// What a session's `work` holds for the program kind.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProgramWorkDto {
    pub problem: PublicProblemDto,
    pub draft: Vec<LearningLabFile>,
    pub examples: Vec<CodingExampleDto>,
    /// Oldest first; each `run_id` is the `check_id` of the engine's summary of the check.
    pub runs: Vec<ProgramRunDto>,
    /// One for each hint that points at lines.
    pub hint_marks: Vec<ProgramHintMarkDto>,
    /// Some once revealed, or in the debrief.
    pub worked_solution: Option<String>,
}

/// What one run of the learner's code found.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProgramRunDto {
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

/// Where a hint points in the draft it was written for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProgramHintMarkDto {
    pub ordinal: u32,
    /// The file of the first line.
    pub path: Option<String>,
    /// 1-based lines.
    pub lines: Vec<u32>,
}

/// What a finished session's `work` holds for the program kind.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProgramDebriefWorkDto {
    pub strong_answer: StrongAnswerDto,
    /// The last submit, nothing kept back.
    pub cases: Vec<CaseViewDto>,
}
