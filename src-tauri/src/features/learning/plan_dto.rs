use super::curriculum::{
    LearningCurriculumChange, LearningCurriculumOperation, LearningCurriculumRevision,
    LearningGenerationJob,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningPlanDto {
    pub program_id: String,
    pub accepted_revision: Option<LearningCurriculumRevision>,
    pub draft_revision: Option<LearningCurriculumRevision>,
    pub preview_changes: Vec<LearningCurriculumChange>,
    pub required_lesson_count_before: usize,
    pub required_lesson_count_after: usize,
    pub resume_lesson_id: Option<String>,
    pub jobs: Vec<LearningGenerationJob>,
    pub latest_diagnostic: Option<LearningDiagnosticAttemptDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PreviewLearningCurriculumRevisionRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub expected_revision: i64,
    pub reason: String,
    pub operations: Vec<LearningCurriculumOperation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningCurriculumRevisionActionRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub revision_id: String,
    pub expected_revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiscardLearningCurriculumRevisionRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub revision_id: String,
    pub expected_revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningDiagnosticStatus {
    Active,
    Submitted,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningDiagnosticPromptDto {
    pub id: String,
    pub prompt: String,
    pub outcome_id: String,
    pub outcome_title: String,
    pub source_version_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningDiagnosticResponseDto {
    pub prompt_id: String,
    pub response: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningDiagnosticCoverageGapDto {
    pub outcome_id: String,
    pub outcome_title: String,
    pub source_version_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningDiagnosticAttemptDto {
    pub id: String,
    pub program_id: String,
    pub status: LearningDiagnosticStatus,
    #[serde(default)]
    pub revision: i64,
    pub prompts: Vec<LearningDiagnosticPromptDto>,
    pub responses: Vec<LearningDiagnosticResponseDto>,
    pub source_coverage_gaps: Vec<LearningDiagnosticCoverageGapDto>,
    pub interpretation: String,
    #[serde(default)]
    pub findings: Vec<LearningDiagnosticFindingDto>,
    pub created_at: i64,
    pub submitted_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningDiagnosticSignal {
    NeedsPractice,
    ReadyForChallenge,
    Uncertain,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningDiagnosticFindingDto {
    pub prompt_id: String,
    pub outcome_id: String,
    pub signal: LearningDiagnosticSignal,
    pub feedback: String,
    pub evidence_quote: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StartLearningDiagnosticRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub expected_revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SubmitLearningDiagnosticRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub diagnostic_id: String,
    pub expected_revision: i64,
    pub responses: Vec<LearningDiagnosticResponseDto>,
    #[serde(default)]
    pub save_only: bool,
    #[serde(default)]
    pub expected_diagnostic_revision: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SkipLearningDiagnosticRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub expected_revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StartLearningGenerationJobRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub expected_revision: i64,
    pub kind: super::curriculum::LearningGenerationJobKind,
    pub request_json: String,
    pub progress_total: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningGenerationJobActionRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub job_id: String,
    pub expected_revision: i64,
}
