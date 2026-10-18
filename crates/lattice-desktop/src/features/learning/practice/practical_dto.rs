//! Renderer-safe contracts for practical work, contained labs, and simulations.
//! Hidden checks and reference solutions deliberately have no public file DTO.

use crate::features::learning::lab_runtime::{
    LearningContainerEngine, LearningLabFile, LearningLabLimits, LearningLabRuntimeCapability,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningPracticalActivityKind {
    CodeLab,
    Debugging,
    CodeReview,
    Incident,
    SystemDesign,
    Project,
    Interview,
    Conversation,
    WritingRevision,
    Custom,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningPracticalActivityStatus {
    Draft,
    Ready,
    Retired,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningPracticalRuntimeKind {
    None,
    Container,
    Builtin,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningPracticalPublicFileRole {
    Starter,
    Reference,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningRuntimeProfileDto {
    pub id: String,
    pub name: String,
    pub engine: LearningContainerEngine,
    pub image_id: String,
    pub command: Vec<String>,
    pub limits: LearningLabLimits,
    pub enabled: bool,
    pub revision: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningPracticalFileDto {
    pub path: String,
    pub role: LearningPracticalPublicFileRole,
    pub content: String,
    pub content_sha256: String,
    pub editable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningPracticalActivityDto {
    pub id: String,
    pub program_id: String,
    pub lesson_id: String,
    pub predecessor_id: Option<String>,
    pub kind: LearningPracticalActivityKind,
    pub title: String,
    pub brief: String,
    pub status: LearningPracticalActivityStatus,
    pub practice_mode: crate::features::learning::dto::LearningPracticeMode,
    pub allowed_aids: Vec<String>,
    pub outcome_ids: Vec<String>,
    pub source_version_ids: Vec<String>,
    pub rubric: Vec<crate::features::learning::assessment_engine::LearningRubricCriterion>,
    pub runtime_kind: LearningPracticalRuntimeKind,
    #[serde(default)]
    pub builtin_runtime:
        Option<crate::features::learning::embedded_runtime::LearningBuiltinRuntime>,
    pub runtime_profile_id: Option<String>,
    pub runtime_engine: Option<LearningContainerEngine>,
    pub runtime_image_id: Option<String>,
    pub runtime_command: Option<Vec<String>>,
    pub runtime_limits: Option<LearningLabLimits>,
    pub runtime_available: bool,
    pub runtime_unavailable_reason: Option<String>,
    pub generator_model: String,
    pub files: Vec<LearningPracticalFileDto>,
    pub revision: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningPracticalCheckStatus {
    Passed,
    Failed,
    Error,
    NotRun,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningPracticalRunStatus {
    Pending,
    Running,
    Passed,
    Failed,
    TimedOut,
    Cancelled,
    Interrupted,
    RuntimeUnavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningPracticalCheckResultDto {
    pub name: String,
    pub status: LearningPracticalCheckStatus,
    pub message: String,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningPracticalRunDto {
    pub id: String,
    pub program_id: String,
    pub activity_id: String,
    pub activity_revision: i64,
    pub practice_session_id: Option<String>,
    pub status: LearningPracticalRunStatus,
    #[serde(default)]
    pub builtin_runtime:
        Option<crate::features::learning::embedded_runtime::LearningBuiltinRuntime>,
    pub engine: Option<LearningContainerEngine>,
    pub image_id: Option<String>,
    pub learner_files: Vec<LearningLabFile>,
    pub stdout: String,
    pub stderr: String,
    pub output_truncated: bool,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<i64>,
    pub checks: Vec<LearningPracticalCheckResultDto>,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningSimulationStatus {
    Active,
    Submitted,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningSimulationSpeaker {
    Learner,
    Counterpart,
    Coach,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningSimulationTurnDto {
    pub id: String,
    pub ordinal: i64,
    pub speaker: LearningSimulationSpeaker,
    pub content: String,
    pub citations: Vec<crate::features::learning::dto::LearningPracticeCitationDto>,
    pub model_name: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningSimulationSessionDto {
    pub id: String,
    pub program_id: String,
    pub activity_id: String,
    pub activity_revision: i64,
    pub practice_session_id: Option<String>,
    pub learner_role: String,
    pub counterpart_role: String,
    pub status: LearningSimulationStatus,
    pub revision: i64,
    pub turns: Vec<LearningSimulationTurnDto>,
    pub created_at: i64,
    pub updated_at: i64,
    pub submitted_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LearningPracticalWorkspaceDto {
    pub program_id: String,
    pub builtin_runtimes:
        Vec<crate::features::learning::embedded_runtime::LearningBuiltinRuntimeCapability>,
    pub runtime_capabilities: Vec<LearningLabRuntimeCapability>,
    pub runtime_profiles: Vec<LearningRuntimeProfileDto>,
    pub activities: Vec<LearningPracticalActivityDto>,
    pub runs: Vec<LearningPracticalRunDto>,
    pub simulations: Vec<LearningSimulationSessionDto>,
}

/// Learner-owned starter files for one immutable practical activity revision.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningPracticalDraftDto {
    pub program_id: String,
    pub activity_id: String,
    pub activity_revision: i64,
    pub draft_revision: i64,
    pub files: Vec<LearningLabFile>,
    pub updated_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetLearningPracticalDraftRequestDto {
    pub program_id: String,
    pub activity_id: String,
    pub activity_revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveLearningPracticalDraftRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub activity_id: String,
    pub activity_revision: i64,
    pub expected_draft_revision: i64,
    pub files: Vec<LearningLabFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveLearningRuntimeProfileRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub profile_id: String,
    pub expected_revision: Option<i64>,
    pub name: String,
    pub engine: LearningContainerEngine,
    pub image_id: String,
    pub command: Vec<String>,
    pub limits: LearningLabLimits,
}

/// Installs an app-authored language environment. Images and commands are
/// selected by the catalog, never supplied by generated lesson content.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrepareLearningRuntimePresetRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub profile_id: String,
    pub preset: crate::features::learning::runtime_catalog::LearningRuntimePresetId,
    pub engine: LearningContainerEngine,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GenerateLearningPracticalActivityRequestDto {
    pub operation_id: String,
    pub activity_id: String,
    pub program_id: String,
    pub lesson_id: String,
    pub expected_program_revision: i64,
    pub kind: LearningPracticalActivityKind,
    pub learner_brief: String,
    pub practice_mode: crate::features::learning::dto::LearningPracticeMode,
    pub allowed_aids: Vec<String>,
    pub runtime_profile_id: Option<String>,
    #[serde(default)]
    pub builtin_runtime:
        Option<crate::features::learning::embedded_runtime::LearningBuiltinRuntime>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StartLearningPracticalRunRequestDto {
    pub operation_id: String,
    pub run_id: String,
    pub program_id: String,
    pub activity_id: String,
    pub expected_activity_revision: i64,
    pub practice_session_id: Option<String>,
    pub learner_files: Vec<LearningLabFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CancelLearningPracticalRunRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub run_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StartLearningSimulationRequestDto {
    pub operation_id: String,
    pub session_id: String,
    pub program_id: String,
    pub activity_id: String,
    pub expected_activity_revision: i64,
    pub practice_session_id: Option<String>,
    pub learner_role: String,
    pub counterpart_role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SendLearningSimulationTurnRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub session_id: String,
    pub expected_revision: i64,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FinishLearningSimulationRequestDto {
    pub operation_id: String,
    pub program_id: String,
    pub session_id: String,
    pub expected_revision: i64,
}
