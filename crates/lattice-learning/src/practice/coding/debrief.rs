//! Minimal compiling stub types for debrief and scorecard DTOs.
//! Owned by cs-debrief; replaced at merge time.

use serde::{Deserialize, Serialize};

use super::hints::CodeHintRung;
use super::problem::Setting;
use super::session::StageTimingDto;
use crate::recall::study_dto::StudyRating;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ScorecardCriterionId {
    ProblemSolving,
    Code,
    Testing,
    Communication,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ScorecardStatus {
    Observed,
    InsufficientEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Quote,
    Lines,
    Run,
    Example,
    Hint,
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceItem {
    pub kind: EvidenceKind,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ScorecardRow {
    pub criterion_id: String,
    pub title: String,
    pub description: String,
    pub status: ScorecardStatus,
    pub points: Option<u32>,
    pub max_points: u32,
    pub evidence: Vec<EvidenceItem>,
    pub feedback: String,
    pub next_step: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Scorecard {
    pub session_id: String,
    pub rows: Vec<ScorecardRow>,
    pub grader_model: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PatternDebriefDto {
    pub pattern_id: String,
    pub name: String,
    pub family: String,
    pub recognition_cue: String,
    pub rating: StudyRating,
    pub due_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CodingDebriefDto {
    pub session_id: String,
    pub scorecard: Scorecard,
    pub patterns: Vec<PatternDebriefDto>,
    pub approach_summary: String,
    pub target_complexity: String,
    pub reference_solution: String,
    pub uncaught_misreadings: Vec<String>,
    pub stage_timings: Vec<StageTimingDto>,
    pub run_count: u32,
    pub hints_used: Vec<CodeHintRung>,
    pub setting: Setting,
}
