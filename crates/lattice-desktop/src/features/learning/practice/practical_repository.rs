//! Durable practical activities, contained runs, and role simulations.

use crate::features::learning::{
    assessment_engine::LearningRubricCriterion,
    dto::{LearningPracticeCitationDto, LearningPracticeMode},
    embedded_runtime::{BuiltinLabExecutionSpec, LearningBuiltinRuntime},
    lab_runtime::{LearningContainerEngine, LearningLabExecutionSpec, LearningLabLimits},
    operations::Operation,
    persistence::db,
    practical_dto::*,
};
use lattice_core::shared::error::{AppError, Result};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use std::collections::HashMap;

mod activities;
mod drafts;
mod reads;
#[cfg(test)]
mod replay_tests;
mod runs;
mod simulations;

pub(in crate::features::learning) use runs::RunCleanup;

fn invalid(message: impl Into<String>) -> AppError {
    AppError::InvalidInput(message.into())
}
fn uuid(value: &str, label: &str) -> Result<()> {
    uuid::Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| invalid(format!("Invalid {label} ID")))
}
fn json<T: Serialize + ?Sized>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|error| AppError::Serialization(error.to_string()))
}
fn decode<T: DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_str(value).map_err(|error| AppError::Serialization(error.to_string()))
}

fn engine_name(value: LearningContainerEngine) -> &'static str {
    match value {
        LearningContainerEngine::Docker => "docker",
        LearningContainerEngine::Podman => "podman",
    }
}
fn parse_engine(value: &str) -> Result<LearningContainerEngine> {
    match value {
        "docker" => Ok(LearningContainerEngine::Docker),
        "podman" => Ok(LearningContainerEngine::Podman),
        _ => Err(AppError::Database(
            "Invalid practical runtime engine".into(),
        )),
    }
}
fn kind_name(value: LearningPracticalActivityKind) -> &'static str {
    match value {
        LearningPracticalActivityKind::CodeLab => "code_lab",
        LearningPracticalActivityKind::Debugging => "debugging",
        LearningPracticalActivityKind::CodeReview => "code_review",
        LearningPracticalActivityKind::Incident => "incident",
        LearningPracticalActivityKind::SystemDesign => "system_design",
        LearningPracticalActivityKind::Project => "project",
        LearningPracticalActivityKind::Interview => "interview",
        LearningPracticalActivityKind::Conversation => "conversation",
        LearningPracticalActivityKind::WritingRevision => "writing_revision",
        LearningPracticalActivityKind::Custom => "custom",
    }
}
fn parse_kind(value: &str) -> Result<LearningPracticalActivityKind> {
    match value {
        "code_lab" => Ok(LearningPracticalActivityKind::CodeLab),
        "debugging" => Ok(LearningPracticalActivityKind::Debugging),
        "code_review" => Ok(LearningPracticalActivityKind::CodeReview),
        "incident" => Ok(LearningPracticalActivityKind::Incident),
        "system_design" => Ok(LearningPracticalActivityKind::SystemDesign),
        "project" => Ok(LearningPracticalActivityKind::Project),
        "interview" => Ok(LearningPracticalActivityKind::Interview),
        "conversation" => Ok(LearningPracticalActivityKind::Conversation),
        "writing_revision" => Ok(LearningPracticalActivityKind::WritingRevision),
        "custom" => Ok(LearningPracticalActivityKind::Custom),
        _ => Err(AppError::Database("Invalid practical activity kind".into())),
    }
}
fn mode_name(value: &LearningPracticeMode) -> &'static str {
    match value {
        LearningPracticeMode::Explore => "explore",
        LearningPracticeMode::Practice => "practice",
        LearningPracticeMode::Demonstrate => "demonstrate",
    }
}
fn parse_mode(value: &str) -> Result<LearningPracticeMode> {
    match value {
        "explore" => Ok(LearningPracticeMode::Explore),
        "practice" => Ok(LearningPracticeMode::Practice),
        "demonstrate" => Ok(LearningPracticeMode::Demonstrate),
        _ => Err(AppError::Database("Invalid practical practice mode".into())),
    }
}
fn status_name(value: &LearningPracticalRunStatus) -> &'static str {
    match value {
        LearningPracticalRunStatus::Pending => "pending",
        LearningPracticalRunStatus::Running => "running",
        LearningPracticalRunStatus::Passed => "passed",
        LearningPracticalRunStatus::Failed => "failed",
        LearningPracticalRunStatus::TimedOut => "timed_out",
        LearningPracticalRunStatus::Cancelled => "cancelled",
        LearningPracticalRunStatus::Interrupted => "interrupted",
        LearningPracticalRunStatus::RuntimeUnavailable => "runtime_unavailable",
    }
}
fn parse_run_status(value: &str) -> Result<LearningPracticalRunStatus> {
    match value {
        "pending" => Ok(LearningPracticalRunStatus::Pending),
        "running" => Ok(LearningPracticalRunStatus::Running),
        "passed" => Ok(LearningPracticalRunStatus::Passed),
        "failed" => Ok(LearningPracticalRunStatus::Failed),
        "timed_out" => Ok(LearningPracticalRunStatus::TimedOut),
        "cancelled" => Ok(LearningPracticalRunStatus::Cancelled),
        "interrupted" => Ok(LearningPracticalRunStatus::Interrupted),
        "runtime_unavailable" => Ok(LearningPracticalRunStatus::RuntimeUnavailable),
        _ => Err(AppError::Database("Invalid practical run status".into())),
    }
}
fn check_status_name(value: LearningPracticalCheckStatus) -> &'static str {
    match value {
        LearningPracticalCheckStatus::Passed => "passed",
        LearningPracticalCheckStatus::Failed => "failed",
        LearningPracticalCheckStatus::Error => "error",
        LearningPracticalCheckStatus::NotRun => "not_run",
    }
}
fn parse_check_status(value: &str) -> Result<LearningPracticalCheckStatus> {
    match value {
        "passed" => Ok(LearningPracticalCheckStatus::Passed),
        "failed" => Ok(LearningPracticalCheckStatus::Failed),
        "error" => Ok(LearningPracticalCheckStatus::Error),
        "not_run" => Ok(LearningPracticalCheckStatus::NotRun),
        _ => Err(AppError::Database("Invalid practical check status".into())),
    }
}
fn simulation_status(value: &str) -> Result<LearningSimulationStatus> {
    match value {
        "active" => Ok(LearningSimulationStatus::Active),
        "submitted" => Ok(LearningSimulationStatus::Submitted),
        "cancelled" => Ok(LearningSimulationStatus::Cancelled),
        _ => Err(AppError::Database("Invalid simulation status".into())),
    }
}
fn parse_speaker(value: &str) -> Result<LearningSimulationSpeaker> {
    match value {
        "learner" => Ok(LearningSimulationSpeaker::Learner),
        "counterpart" => Ok(LearningSimulationSpeaker::Counterpart),
        "coach" => Ok(LearningSimulationSpeaker::Coach),
        _ => Err(AppError::Database("Invalid simulation speaker".into())),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InternalActivityFile {
    path: String,
    role: String,
    content: String,
    content_sha256: String,
    editable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InternalActivitySnapshot {
    id: String,
    program_id: String,
    lesson_id: String,
    kind: LearningPracticalActivityKind,
    title: String,
    brief: String,
    practice_mode: LearningPracticeMode,
    allowed_aids: Vec<String>,
    outcome_ids: Vec<String>,
    source_version_ids: Vec<String>,
    rubric: Vec<LearningRubricCriterion>,
    runtime_profile_id: Option<String>,
    #[serde(default)]
    builtin_runtime: Option<LearningBuiltinRuntime>,
    #[serde(default)]
    builtin_runtime_version: Option<String>,
    /// Random ownership proof for this run's temp workspace. It is stored in
    /// the immutable run snapshot so startup recovery can clean only a folder
    /// created by the corresponding container invocation.
    #[serde(default)]
    workspace_owner_token: Option<String>,
    runtime_enabled: bool,
    runtime_engine: Option<LearningContainerEngine>,
    runtime_image_id: Option<String>,
    runtime_command: Option<Vec<String>>,
    runtime_limits: Option<LearningLabLimits>,
    files: Vec<InternalActivityFile>,
    #[serde(default)]
    pub interview_plan: Option<LearningInterviewPlanDto>,
    revision: i64,
    generator_model: String,
}

pub(in crate::features::learning) enum PreparedExecution {
    Container {
        engine: LearningContainerEngine,
        spec: LearningLabExecutionSpec,
    },
    Builtin {
        spec: BuiltinLabExecutionSpec,
        version: String,
    },
}

pub(in crate::features::learning) struct PreparedRun {
    pub(in crate::features::learning) dto: LearningPracticalRunDto,
    pub(in crate::features::learning) execution: PreparedExecution,
    pub(in crate::features::learning) outcome_ids: Vec<String>,
    pub(in crate::features::learning) practice_mode: LearningPracticeMode,
    pub(in crate::features::learning) allowed_aids: Vec<String>,
    pub(in crate::features::learning) workspace_owner_token: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SimulationTurnWrite {
    pub content: String,
    pub citations: Vec<LearningPracticeCitationDto>,
    pub model_name: String,
}

/// Saved workspace data. Runtime availability is resolved by `practical_workspace`.
/// The inner DTO is private until the service has applied the current capabilities.
#[derive(Debug, Clone)]
pub struct PracticalWorkspaceSnapshot {
    pub(in crate::features::learning) workspace: LearningPracticalWorkspaceDto,
    pub(in crate::features::learning) builtin_versions: HashMap<String, Option<String>>,
}

#[derive(Clone)]
pub struct LearningPracticalRepository {
    pool: SqlitePool,
}

impl LearningPracticalRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    async fn active_program(tx: &mut Transaction<'_, Sqlite>, program_id: &str) -> Result<i64> {
        let row = sqlx::query("SELECT status,revision FROM learning_programs WHERE id=?")
            .bind(program_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Learning program not found".into()))?;
        if row.get::<String, _>("status") != "active" {
            return Err(invalid(
                "Practical work requires an active learning program.",
            ));
        }
        Ok(row.get("revision"))
    }

    async fn writer_lock(tx: &mut Transaction<'_, Sqlite>, program_id: &str) -> Result<()> {
        sqlx::query("UPDATE learning_programs SET revision=revision WHERE id=?")
            .bind(program_id)
            .execute(&mut **tx)
            .await
            .map_err(db)?;
        Ok(())
    }
}

fn workspace_owner_token_from_snapshot(snapshot: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(snapshot)
        .ok()?
        .get("workspaceOwnerToken")?
        .as_str()
        .map(str::to_owned)
}

fn practical_operation<'a>(
    scope: &'static str,
    operation_id: &'a str,
    program_id: Option<&'a str>,
    subject_id: &'a str,
    kind: &'a str,
    payload_hash: &'a str,
    conflict: &'static str,
) -> Operation<'a> {
    Operation {
        id: operation_id,
        scope,
        kind,
        program_id,
        subject_id: Some(subject_id),
        payload_hash,
        conflict,
    }
}

/// What a saved practical draft produced, so a replay answers with the same
/// draft revision and timestamp.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PracticalDraftOperationResult {
    draft_revision: i64,
    updated_at: i64,
}
