//! Durable practical activities, contained runs, and role simulations.

use super::{
    assessment_engine::LearningRubricCriterion,
    dto::{LearningPracticeCitationDto, LearningPracticeMode},
    embedded_runtime::{self, BuiltinLabExecutionSpec, LearningBuiltinRuntime},
    lab_runtime::{
        self, LearningContainerEngine, LearningLabExecutionResult, LearningLabExecutionSpec,
        LearningLabFile, LearningLabLimits, LearningLabRunStatus, LearningLabRuntimeCapability,
    },
    practical_dto::*,
    practical_generation::{GeneratedPracticalActivity, PracticalFileRole},
};
use crate::shared::error::{AppError, Result};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{LazyLock, Mutex},
};
use tokio_util::sync::CancellationToken;

static ACTIVE_RUNS: LazyLock<Mutex<HashMap<String, CancellationToken>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn db(error: sqlx::Error) -> AppError {
    AppError::Database(error.to_string())
}
fn invalid(message: impl Into<String>) -> AppError {
    AppError::InvalidInput(message.into())
}
fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
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
fn digest<T: Serialize + ?Sized>(value: &T) -> Result<String> {
    let bytes =
        serde_json::to_vec(value).map_err(|error| AppError::Serialization(error.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
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
    revision: i64,
    generator_model: String,
}

enum PreparedExecution {
    Container {
        engine: LearningContainerEngine,
        spec: LearningLabExecutionSpec,
    },
    Builtin {
        spec: BuiltinLabExecutionSpec,
        version: String,
    },
}

struct PreparedRun {
    dto: LearningPracticalRunDto,
    execution: PreparedExecution,
    outcome_ids: Vec<String>,
    practice_mode: LearningPracticeMode,
    allowed_aids: Vec<String>,
    workspace_owner_token: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SimulationTurnWrite {
    pub content: String,
    pub citations: Vec<LearningPracticeCitationDto>,
    pub model_name: String,
}

#[derive(Clone)]
pub struct LearningPracticalRepository {
    pool: SqlitePool,
}

impl LearningPracticalRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn get_draft(
        &self,
        request: &GetLearningPracticalDraftRequestDto,
    ) -> Result<LearningPracticalDraftDto> {
        uuid(&request.program_id, "program")?;
        uuid(&request.activity_id, "activity")?;
        if request.activity_revision < 0 {
            return Err(invalid("Invalid practical activity revision."));
        }
        let mut tx = self.pool.begin().await.map_err(db)?;
        let activity = sqlx::query(
            "SELECT revision FROM learning_practical_activities WHERE program_id=? AND id=?",
        )
        .bind(&request.program_id)
        .bind(&request.activity_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Practical activity not found".into()))?;
        let current_revision: i64 = activity.get("revision");
        if current_revision != request.activity_revision {
            return Err(invalid("Practical activity changed; reload and retry."));
        }
        if let Some(row) = sqlx::query("SELECT draft_revision,files_json,updated_at FROM learning_practical_drafts WHERE program_id=? AND activity_id=? AND activity_revision=?")
            .bind(&request.program_id).bind(&request.activity_id).bind(request.activity_revision)
            .fetch_optional(&mut *tx).await.map_err(db)?
        {
            let draft = LearningPracticalDraftDto {
                program_id: request.program_id.clone(), activity_id: request.activity_id.clone(),
                activity_revision: request.activity_revision, draft_revision: row.get("draft_revision"),
                files: decode(&row.get::<String, _>("files_json"))?, updated_at: Some(row.get("updated_at")),
            };
            tx.commit().await.map_err(db)?;
            return Ok(draft);
        }
        let rows = sqlx::query("SELECT path,content FROM learning_practical_files WHERE activity_id=? AND role='starter' AND editable=1 ORDER BY ordinal")
            .bind(&request.activity_id).fetch_all(&mut *tx).await.map_err(db)?;
        let draft = LearningPracticalDraftDto {
            program_id: request.program_id.clone(),
            activity_id: request.activity_id.clone(),
            activity_revision: request.activity_revision,
            draft_revision: 0,
            files: rows
                .into_iter()
                .map(|row| LearningLabFile {
                    path: row.get("path"),
                    content: row.get("content"),
                })
                .collect(),
            updated_at: None,
        };
        tx.commit().await.map_err(db)?;
        Ok(draft)
    }

    pub async fn save_draft(
        &self,
        request: &SaveLearningPracticalDraftRequestDto,
        payload_hash: &str,
    ) -> Result<LearningPracticalDraftDto> {
        for (value, label) in [
            (&request.operation_id, "operation"),
            (&request.program_id, "program"),
            (&request.activity_id, "activity"),
        ] {
            uuid(value, label)?;
        }
        if request.activity_revision < 0 || request.expected_draft_revision < 0 {
            return Err(invalid("Invalid practical draft revision."));
        }
        if request.files.len() > 64 {
            return Err(invalid(
                "A practical draft cannot contain more than 64 starter files.",
            ));
        }
        let mut seen = HashSet::new();
        let mut total_bytes = 0usize;
        for file in &request.files {
            if file.path.is_empty()
                || file.path.starts_with('/')
                || file.path.contains('\\')
                || file
                    .path
                    .split('/')
                    .any(|part| part.is_empty() || part == "." || part == "..")
            {
                return Err(invalid("Practical draft contains an invalid file path."));
            }
            if !seen.insert(file.path.as_str()) {
                return Err(invalid("Practical draft contains a duplicate file path."));
            }
            if file.content.len() > 256 * 1024 {
                return Err(invalid(
                    "A practical starter file exceeds the 256 KiB limit.",
                ));
            }
            total_bytes = total_bytes.saturating_add(file.content.len());
        }
        if total_bytes > 2 * 1024 * 1024 {
            return Err(invalid("Practical draft exceeds the 2 MiB total limit."));
        }

        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &request.program_id).await?;
        if let Some(row) = sqlx::query("SELECT program_id,activity_id,activity_revision,payload_hash,result_revision,created_at FROM learning_practical_draft_operations WHERE operation_id=?")
            .bind(&request.operation_id).fetch_optional(&mut *tx).await.map_err(db)?
        {
            if row.get::<String, _>("program_id") == request.program_id
                && row.get::<String, _>("activity_id") == request.activity_id
                && row.get::<i64, _>("activity_revision") == request.activity_revision
                && row.get::<String, _>("payload_hash") == payload_hash
            {
                let replay = LearningPracticalDraftDto {
                    program_id: request.program_id.clone(),
                    activity_id: request.activity_id.clone(),
                    activity_revision: request.activity_revision,
                    draft_revision: row.get("result_revision"),
                    files: request.files.clone(),
                    updated_at: Some(row.get("created_at")),
                };
                tx.commit().await.map_err(db)?;
                return Ok(replay);
            }
            return Err(invalid("Operation ID was reused with different practical draft data."));
        }
        Self::active_program(&mut tx, &request.program_id).await?;
        let activity = sqlx::query(
            "SELECT revision FROM learning_practical_activities WHERE program_id=? AND id=?",
        )
        .bind(&request.program_id)
        .bind(&request.activity_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Practical activity not found".into()))?;
        if activity.get::<i64, _>("revision") != request.activity_revision {
            return Err(invalid("Practical activity changed; reload and retry."));
        }
        let allowed_rows = sqlx::query("SELECT path FROM learning_practical_files WHERE activity_id=? AND role='starter' AND editable=1 ORDER BY ordinal")
            .bind(&request.activity_id).fetch_all(&mut *tx).await.map_err(db)?;
        let allowed = allowed_rows
            .into_iter()
            .map(|row| row.get::<String, _>("path"))
            .collect::<HashSet<_>>();
        if request.files.len() != allowed.len()
            || request
                .files
                .iter()
                .any(|file| !allowed.contains(&file.path))
        {
            return Err(invalid(
                "Practical drafts may only change editable starter files.",
            ));
        }
        let existing = sqlx::query("SELECT draft_revision FROM learning_practical_drafts WHERE program_id=? AND activity_id=? AND activity_revision=?")
            .bind(&request.program_id).bind(&request.activity_id).bind(request.activity_revision)
            .fetch_optional(&mut *tx).await.map_err(db)?;
        let current_revision = existing
            .map(|row| row.get::<i64, _>("draft_revision"))
            .unwrap_or(0);
        if current_revision != request.expected_draft_revision {
            return Err(invalid("Practical draft changed; reload and retry."));
        }
        let result_revision = current_revision + 1;
        let timestamp = now();
        sqlx::query("INSERT INTO learning_practical_drafts(program_id,activity_id,activity_revision,draft_revision,files_json,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(program_id,activity_id,activity_revision) DO UPDATE SET draft_revision=excluded.draft_revision,files_json=excluded.files_json,updated_at=excluded.updated_at")
            .bind(&request.program_id).bind(&request.activity_id).bind(request.activity_revision)
            .bind(result_revision).bind(json(&request.files)?).bind(timestamp).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("INSERT INTO learning_practical_draft_operations(operation_id,program_id,activity_id,activity_revision,payload_hash,result_revision,created_at) VALUES(?,?,?,?,?,?,?)")
            .bind(&request.operation_id).bind(&request.program_id).bind(&request.activity_id).bind(request.activity_revision)
            .bind(payload_hash).bind(result_revision).bind(timestamp).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(LearningPracticalDraftDto {
            program_id: request.program_id.clone(),
            activity_id: request.activity_id.clone(),
            activity_revision: request.activity_revision,
            draft_revision: result_revision,
            files: request.files.clone(),
            updated_at: Some(timestamp),
        })
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

    pub async fn workspace(&self, program_id: &str) -> Result<LearningPracticalWorkspaceDto> {
        uuid(program_id, "program")?;
        let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM learning_programs WHERE id=?")
            .bind(program_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        if exists.is_none() {
            return Err(AppError::NotFound("Learning program not found".into()));
        }
        let (docker, podman) = tokio::join!(
            lab_runtime::detect_container_engine(LearningContainerEngine::Docker),
            lab_runtime::detect_container_engine(LearningContainerEngine::Podman)
        );
        let capabilities = vec![docker, podman];
        let profiles = self.profiles().await?;
        let activity_rows = sqlx::query(
            "SELECT * FROM learning_practical_activities WHERE program_id=? ORDER BY created_at DESC,id",
        )
        .bind(program_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let mut activities = Vec::with_capacity(activity_rows.len());
        for row in activity_rows {
            activities.push(self.parse_activity(row, &capabilities, &profiles).await?);
        }
        let run_rows = sqlx::query(
            "SELECT * FROM learning_practical_runs WHERE program_id=? ORDER BY created_at DESC,id",
        )
        .bind(program_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let mut runs = Vec::with_capacity(run_rows.len());
        for row in run_rows {
            runs.push(self.parse_run(row).await?);
        }
        let simulation_rows = sqlx::query(
            "SELECT * FROM learning_simulation_sessions WHERE program_id=? ORDER BY created_at DESC,id",
        )
        .bind(program_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let mut simulations = Vec::with_capacity(simulation_rows.len());
        for row in simulation_rows {
            simulations.push(self.parse_simulation(row).await?);
        }
        Ok(LearningPracticalWorkspaceDto {
            program_id: program_id.into(),
            builtin_runtimes: embedded_runtime::capabilities(),
            runtime_capabilities: capabilities,
            runtime_profiles: profiles,
            activities,
            runs,
            simulations,
        })
    }

    async fn profiles(&self) -> Result<Vec<LearningRuntimeProfileDto>> {
        let rows = sqlx::query("SELECT * FROM learning_runtime_profiles ORDER BY name,id")
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.into_iter()
            .map(|row| {
                Ok(LearningRuntimeProfileDto {
                    id: row.get("id"),
                    name: row.get("name"),
                    engine: parse_engine(row.get("engine"))?,
                    image_id: row.get("image_id"),
                    command: decode(row.get("command_json"))?,
                    limits: decode(row.get("limits_json"))?,
                    enabled: row.get::<i64, _>("enabled") != 0,
                    revision: row.get("revision"),
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                })
            })
            .collect()
    }

    async fn parse_activity(
        &self,
        row: sqlx::sqlite::SqliteRow,
        capabilities: &[LearningLabRuntimeCapability],
        profiles: &[LearningRuntimeProfileDto],
    ) -> Result<LearningPracticalActivityDto> {
        let id: String = row.get("id");
        let file_rows = sqlx::query("SELECT * FROM learning_practical_files WHERE activity_id=? AND role IN ('starter','reference') ORDER BY ordinal")
            .bind(&id).fetch_all(&self.pool).await.map_err(db)?;
        let files = file_rows
            .into_iter()
            .map(|file| {
                Ok(LearningPracticalFileDto {
                    path: file.get("path"),
                    role: match file.get::<String, _>("role").as_str() {
                        "starter" => LearningPracticalPublicFileRole::Starter,
                        "reference" => LearningPracticalPublicFileRole::Reference,
                        _ => {
                            return Err(AppError::Database(
                                "Hidden practical file leaked into public query".into(),
                            ))
                        }
                    },
                    content: file.get("content"),
                    content_sha256: file.get("content_sha256"),
                    editable: file.get::<i64, _>("editable") != 0,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let builtin_runtime = row
            .get::<Option<String>, _>("builtin_runtime")
            .as_deref()
            .map(LearningBuiltinRuntime::parse)
            .transpose()?;
        let runtime_kind = if builtin_runtime.is_some() {
            LearningPracticalRuntimeKind::Builtin
        } else if row.get::<String, _>("runtime_kind") == "container" {
            LearningPracticalRuntimeKind::Container
        } else {
            LearningPracticalRuntimeKind::None
        };
        let profile_id: Option<String> = row.get("runtime_profile_id");
        let profile = profile_id
            .as_deref()
            .and_then(|id| profiles.iter().find(|profile| profile.id == id));
        let frozen_engine = row
            .get::<Option<String>, _>("runtime_engine")
            .as_deref()
            .map(parse_engine)
            .transpose()?;
        let capability = frozen_engine.and_then(|engine| {
            capabilities
                .iter()
                .find(|capability| capability.engine == engine)
        });
        let (runtime_available, runtime_unavailable_reason) = match runtime_kind {
            LearningPracticalRuntimeKind::Builtin if builtin_runtime.map(|runtime| runtime.version().to_owned()) != row.get::<Option<String>, _>("builtin_runtime_version") =>
                (false, Some("This activity uses an earlier built-in runtime. Its saved results remain available; generate a new activity to run with this app version.".into())),
            LearningPracticalRuntimeKind::Builtin => embedded_runtime::capabilities().into_iter()
                .find(|capability| Some(capability.id) == builtin_runtime)
                .map(|capability| (capability.available, capability.reason))
                .unwrap_or((false, Some("Built-in runtime is unavailable.".into()))),
            LearningPracticalRuntimeKind::None => (
                false,
                Some("This activity is completed and reviewed without local execution.".into()),
            ),
            LearningPracticalRuntimeKind::Container => match (profile, capability) {
                (Some(profile), Some(capability)) if profile.enabled && capability.available => {
                    (true, None)
                }
                (Some(profile), _) if !profile.enabled => (
                    false,
                    Some("The frozen runtime profile is disabled.".into()),
                ),
                (_, Some(capability)) => (
                    false,
                    capability
                        .reason
                        .clone()
                        .or_else(|| Some("The container runtime is unavailable.".into())),
                ),
                _ => (
                    false,
                    Some("The frozen runtime profile is unavailable.".into()),
                ),
            },
        };
        Ok(LearningPracticalActivityDto {
            id,
            program_id: row.get("program_id"),
            lesson_id: row.get("lesson_id"),
            predecessor_id: row.get("predecessor_id"),
            kind: parse_kind(row.get("kind"))?,
            title: row.get("title"),
            brief: row.get("brief"),
            status: match row.get::<String, _>("status").as_str() {
                "draft" => LearningPracticalActivityStatus::Draft,
                "ready" => LearningPracticalActivityStatus::Ready,
                "retired" => LearningPracticalActivityStatus::Retired,
                _ => {
                    return Err(AppError::Database(
                        "Invalid practical activity status".into(),
                    ))
                }
            },
            practice_mode: parse_mode(row.get("practice_mode"))?,
            allowed_aids: decode(row.get("allowed_aids_json"))?,
            outcome_ids: decode(row.get("outcome_ids_json"))?,
            source_version_ids: decode(row.get("source_version_ids_json"))?,
            rubric: decode(row.get("rubric_json"))?,
            runtime_kind,
            builtin_runtime,
            runtime_profile_id: profile_id,
            runtime_engine: frozen_engine,
            runtime_image_id: row.get("runtime_image_id"),
            runtime_command: row
                .get::<Option<String>, _>("runtime_command_json")
                .as_deref()
                .map(decode)
                .transpose()?,
            runtime_limits: row
                .get::<Option<String>, _>("runtime_limits_json")
                .as_deref()
                .map(decode)
                .transpose()?,
            runtime_available,
            runtime_unavailable_reason,
            generator_model: row.get("generator_model"),
            files,
            revision: row.get("revision"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
    }

    async fn parse_run(&self, row: sqlx::sqlite::SqliteRow) -> Result<LearningPracticalRunDto> {
        let id: String = row.get("id");
        let checks = sqlx::query(
            "SELECT * FROM learning_practical_run_checks WHERE run_id=? ORDER BY ordinal",
        )
        .bind(&id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?
        .into_iter()
        .map(|check| {
            Ok(LearningPracticalCheckResultDto {
                name: check.get("name"),
                status: parse_check_status(check.get("status"))?,
                message: check.get("message"),
                duration_ms: check.get("duration_ms"),
            })
        })
        .collect::<Result<Vec<_>>>()?;
        Ok(LearningPracticalRunDto {
            id,
            program_id: row.get("program_id"),
            activity_id: row.get("activity_id"),
            activity_revision: row.get("activity_revision"),
            practice_session_id: row.get("practice_session_id"),
            status: parse_run_status(row.get("status"))?,
            builtin_runtime: row
                .get::<Option<String>, _>("builtin_runtime")
                .as_deref()
                .map(LearningBuiltinRuntime::parse)
                .transpose()?,
            engine: row
                .get::<Option<String>, _>("engine")
                .as_deref()
                .map(parse_engine)
                .transpose()?,
            image_id: row.get("image_id"),
            learner_files: decode(row.get("learner_files_json"))?,
            stdout: row.get("stdout"),
            stderr: row.get("stderr"),
            output_truncated: row.get::<i64, _>("output_truncated") != 0,
            exit_code: row
                .get::<Option<i64>, _>("exit_code")
                .map(|value| value as i32),
            duration_ms: row.get("duration_ms"),
            checks,
            created_at: row.get("created_at"),
            completed_at: row.get("completed_at"),
        })
    }

    async fn parse_simulation(
        &self,
        row: sqlx::sqlite::SqliteRow,
    ) -> Result<LearningSimulationSessionDto> {
        let id: String = row.get("id");
        let turns = sqlx::query(
            "SELECT * FROM learning_simulation_turns WHERE session_id=? ORDER BY ordinal",
        )
        .bind(&id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?
        .into_iter()
        .map(|turn| {
            Ok(LearningSimulationTurnDto {
                id: turn.get("id"),
                ordinal: turn.get("ordinal"),
                speaker: parse_speaker(turn.get("speaker"))?,
                content: turn.get("content"),
                citations: decode(turn.get("citations_json"))?,
                model_name: turn.get("model_name"),
                created_at: turn.get("created_at"),
            })
        })
        .collect::<Result<Vec<_>>>()?;
        Ok(LearningSimulationSessionDto {
            id,
            program_id: row.get("program_id"),
            activity_id: row.get("activity_id"),
            activity_revision: row.get("activity_revision"),
            practice_session_id: row.get("practice_session_id"),
            learner_role: row.get("learner_role"),
            counterpart_role: row.get("counterpart_role"),
            status: simulation_status(row.get("status"))?,
            revision: row.get("revision"),
            turns,
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            submitted_at: row.get("submitted_at"),
        })
    }

    pub async fn run(&self, run_id: &str) -> Result<LearningPracticalRunDto> {
        uuid(run_id, "run")?;
        let row = sqlx::query("SELECT * FROM learning_practical_runs WHERE id=?")
            .bind(run_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Practical run not found".into()))?;
        self.parse_run(row).await
    }

    pub async fn simulation(&self, session_id: &str) -> Result<LearningSimulationSessionDto> {
        uuid(session_id, "simulation")?;
        let row = sqlx::query("SELECT * FROM learning_simulation_sessions WHERE id=?")
            .bind(session_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Simulation not found".into()))?;
        self.parse_simulation(row).await
    }
}

impl LearningPracticalRepository {
    async fn prepare_run(
        &self,
        request: &StartLearningPracticalRunRequestDto,
        payload_hash: &str,
    ) -> Result<Option<PreparedRun>> {
        for (value, label) in [
            (&request.operation_id, "operation"),
            (&request.run_id, "run"),
            (&request.program_id, "program"),
            (&request.activity_id, "activity"),
        ] {
            uuid(value, label)?;
        }
        if let Some(session_id) = request.practice_session_id.as_deref() {
            uuid(session_id, "practice session")?;
        }
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &request.program_id).await?;
        if let Some(row) = sqlx::query("SELECT program_id,run_id,payload_hash FROM learning_practical_run_operations WHERE operation_id=?")
            .bind(&request.operation_id).fetch_optional(&mut *tx).await.map_err(db)?
        {
            if row.get::<String, _>("program_id") == request.program_id
                && row.get::<String, _>("run_id") == request.run_id
                && row.get::<String, _>("payload_hash") == payload_hash
            {
                tx.commit().await.map_err(db)?;
                return Ok(None);
            }
            return Err(invalid("Operation ID was reused with different run data."));
        }
        Self::active_program(&mut tx, &request.program_id).await?;
        let activity =
            Self::internal_activity(&mut tx, &request.program_id, &request.activity_id).await?;
        if activity.revision != request.expected_activity_revision {
            return Err(invalid("Practical activity changed; reload and retry."));
        }
        if activity.runtime_profile_id.is_some() && !activity.runtime_enabled {
            return Err(AppError::ServiceNotAvailable(
                "The practical activity's runtime profile is disabled.".into(),
            ));
        }
        if let Some(session_id) = request.practice_session_id.as_deref() {
            let owned: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM learning_practice_sessions WHERE program_id=? AND id=?",
            )
            .bind(&request.program_id)
            .bind(session_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
            if owned.is_none() {
                return Err(invalid("Practice session does not belong to this program."));
            }
        }
        let required = activity
            .files
            .iter()
            .filter(|file| file.role == "starter" && file.editable)
            .map(|file| file.path.as_str())
            .collect::<HashSet<_>>();
        let reserved = activity
            .files
            .iter()
            .filter(|file| file.role != "starter")
            .map(|file| file.path.as_str())
            .collect::<HashSet<_>>();
        let learner_paths = request
            .learner_files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<HashSet<_>>();
        if !required.is_subset(&learner_paths) {
            return Err(invalid("The run is missing an editable starter file."));
        }
        if learner_paths.iter().any(|path| reserved.contains(path)) {
            return Err(invalid(
                "Learner files cannot replace reference or evaluator files.",
            ));
        }
        let protected_files = activity
            .files
            .iter()
            .filter(|file| file.role == "check" || file.role == "reference")
            .collect::<Vec<_>>();
        let read_only_paths = protected_files
            .iter()
            .map(|file| file.path.clone())
            .collect::<Vec<_>>();
        let mut execution_files = request.learner_files.clone();
        execution_files.extend(protected_files.into_iter().map(|file| LearningLabFile {
            path: file.path.clone(),
            content: file.content.clone(),
        }));
        let execution = if let Some(runtime) = activity.builtin_runtime {
            let spec = BuiltinLabExecutionSpec {
                run_id: request.run_id.clone(),
                runtime,
                files: execution_files,
                read_only_paths,
                limits: LearningLabLimits::default(),
            };
            spec.validate()?;
            PreparedExecution::Builtin {
                spec,
                version: activity
                    .builtin_runtime_version
                    .clone()
                    .ok_or_else(|| invalid("Built-in activity lost its runtime version."))?,
            }
        } else {
            let engine = activity.runtime_engine.ok_or_else(|| {
                AppError::ServiceNotAvailable(
                    "This practical activity does not have a local execution runtime.".into(),
                )
            })?;
            let spec = LearningLabExecutionSpec {
                run_id: request.run_id.clone(),
                image_id: activity.runtime_image_id.clone().ok_or_else(|| {
                    AppError::InvalidState("Practical activity lost its runtime image.".into())
                })?,
                command: activity.runtime_command.clone().ok_or_else(|| {
                    AppError::InvalidState("Practical activity lost its runtime command.".into())
                })?,
                files: execution_files,
                read_only_paths,
                limits: activity.runtime_limits.clone().ok_or_else(|| {
                    AppError::InvalidState("Practical activity lost its runtime limits.".into())
                })?,
            };
            lab_runtime::validate_execution_spec(&spec)?;
            PreparedExecution::Container { engine, spec }
        };
        let workspace_owner_token = match &execution {
            PreparedExecution::Container { .. } => Some(uuid::Uuid::new_v4().to_string()),
            PreparedExecution::Builtin { .. } => None,
        };
        let timestamp = now();
        let mut run_snapshot = activity.clone();
        run_snapshot.workspace_owner_token = workspace_owner_token.clone();
        let snapshot = json(&run_snapshot)?;
        sqlx::query("INSERT INTO learning_practical_runs(id,program_id,activity_id,activity_revision,activity_snapshot_json,practice_session_id,operation_id,payload_hash,engine,image_id,status,learner_files_json,created_at,builtin_runtime,builtin_runtime_version) VALUES(?,?,?,?,?,?,?,?,?,?,'running',?,?,?,?)")
            .bind(&request.run_id).bind(&request.program_id).bind(&request.activity_id)
            .bind(activity.revision).bind(snapshot).bind(&request.practice_session_id)
            .bind(&request.operation_id).bind(payload_hash).bind(activity.runtime_engine.map(engine_name)).bind(&activity.runtime_image_id)
            .bind(json(&request.learner_files)?).bind(timestamp).bind(activity.builtin_runtime.map(LearningBuiltinRuntime::as_str))
            .bind(&activity.builtin_runtime_version)
            .execute(&mut *tx).await.map_err(db)?;
        sqlx::query("INSERT INTO learning_practical_run_operations(operation_id,program_id,run_id,kind,payload_hash,created_at) VALUES(?,?,?,'start',?,?)")
            .bind(&request.operation_id).bind(&request.program_id).bind(&request.run_id)
            .bind(payload_hash).bind(timestamp).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        let dto = self.run(&request.run_id).await?;
        Ok(Some(PreparedRun {
            dto,
            execution,
            outcome_ids: activity.outcome_ids,
            practice_mode: activity.practice_mode,
            allowed_aids: activity.allowed_aids,
            workspace_owner_token,
        }))
    }

    pub async fn start_run(
        &self,
        request: &StartLearningPracticalRunRequestDto,
        payload_hash: &str,
    ) -> Result<LearningPracticalRunDto> {
        let Some(prepared) = self.prepare_run(request, payload_hash).await? else {
            return self.run(&request.run_id).await;
        };
        let unavailable_reason = match &prepared.execution {
            PreparedExecution::Container { engine, .. } => {
                let capability = lab_runtime::detect_container_engine(*engine).await;
                (!capability.available).then(|| capability.reason.unwrap_or_else(|| "The selected container runtime is unavailable.".into()))
            },
            PreparedExecution::Builtin {spec, version} if version != spec.runtime.version() =>
                Some("The activity's built-in runtime version is no longer available. Generate a new activity to use the current runtime.".into()),
            PreparedExecution::Builtin {spec, ..} => embedded_runtime::capabilities().into_iter()
                .find(|capability| capability.id == spec.runtime && !capability.available)
                .map(|capability| capability.reason.unwrap_or_else(|| "The built-in runtime is unavailable.".into())),
        };
        if let Some(reason) = unavailable_reason {
            let result = LearningLabExecutionResult {
                status: LearningLabRunStatus::RuntimeUnavailable,
                exit_code: None,
                stdout: String::new(),
                stderr: reason,
                output_truncated: false,
                duration_ms: 0,
            };
            self.finish_run(&prepared, result).await?;
            return self.run(&request.run_id).await;
        }
        let token = CancellationToken::new();
        {
            let mut active = ACTIVE_RUNS.lock().map_err(|_| {
                AppError::InternalError("Practical run registry is unavailable.".into())
            })?;
            if active
                .insert(request.run_id.clone(), token.clone())
                .is_some()
            {
                return Err(AppError::ConcurrentModification {
                    resource: "practical run".into(),
                    details: "This run is already active.".into(),
                });
            }
        }
        // A cancellation may have committed between run creation and registry
        // registration. Never launch a container for a terminal run.
        if self.run(&request.run_id).await?.status != LearningPracticalRunStatus::Running {
            ACTIVE_RUNS
                .lock()
                .map_err(|_| {
                    AppError::InternalError("Practical run registry is unavailable.".into())
                })?
                .remove(&request.run_id);
            return self.run(&request.run_id).await;
        }
        let root = run_workspace(&request.run_id);
        let mut workspace_owned = false;
        let mut ownership_marker_created = false;
        let outcome = async {
            match &prepared.execution {
                PreparedExecution::Builtin { spec, .. } => {
                    embedded_runtime::execute_builtin_lab(spec.clone(), token).await
                }
                PreparedExecution::Container { engine, spec } => {
                    if let Some(parent) = root.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    if root.exists() {
                        return Err(AppError::Security(
                            "A practical run workspace already exists unexpectedly.".into(),
                        ));
                    }
                    lab_runtime::materialize_workspace(&root, spec)?;
                    // The workspace is ours only after materialization has
                    // atomically created the previously absent directory.
                    workspace_owned = true;
                    let owner_token =
                        prepared.workspace_owner_token.as_deref().ok_or_else(|| {
                            AppError::InvalidState(
                                "Container run is missing its workspace ownership token.".into(),
                            )
                        })?;
                    create_workspace_owner_marker(&request.run_id, owner_token)?;
                    ownership_marker_created = true;
                    lab_runtime::execute_container_lab(*engine, &root, spec, token).await
                }
            }
        }
        .await;
        cleanup_invocation_workspace(&request.run_id, workspace_owned, ownership_marker_created);
        ACTIVE_RUNS
            .lock()
            .map_err(|_| AppError::InternalError("Practical run registry is unavailable.".into()))?
            .remove(&request.run_id);
        let execution = match outcome {
            Ok(result) => result,
            Err(error) => LearningLabExecutionResult {
                status: if matches!(&error, AppError::ServiceNotAvailable(_)) {
                    LearningLabRunStatus::RuntimeUnavailable
                } else {
                    LearningLabRunStatus::Failed
                },
                exit_code: None,
                stdout: String::new(),
                stderr: error.to_string().chars().take(2_000).collect(),
                output_truncated: false,
                duration_ms: 0,
            },
        };
        self.finish_run(&prepared, execution).await?;
        self.run(&request.run_id).await
    }

    async fn finish_run(
        &self,
        prepared: &PreparedRun,
        execution: LearningLabExecutionResult,
    ) -> Result<()> {
        let status = match execution.status {
            LearningLabRunStatus::Passed => LearningPracticalRunStatus::Passed,
            LearningLabRunStatus::Failed => LearningPracticalRunStatus::Failed,
            LearningLabRunStatus::TimedOut => LearningPracticalRunStatus::TimedOut,
            LearningLabRunStatus::Cancelled => LearningPracticalRunStatus::Cancelled,
            LearningLabRunStatus::RuntimeUnavailable => {
                LearningPracticalRunStatus::RuntimeUnavailable
            }
        };
        let check_status = match status {
            LearningPracticalRunStatus::Passed => LearningPracticalCheckStatus::Passed,
            LearningPracticalRunStatus::Failed => LearningPracticalCheckStatus::Failed,
            LearningPracticalRunStatus::TimedOut
            | LearningPracticalRunStatus::Cancelled
            | LearningPracticalRunStatus::RuntimeUnavailable => {
                LearningPracticalCheckStatus::NotRun
            }
            _ => LearningPracticalCheckStatus::Error,
        };
        let message = match status {
            LearningPracticalRunStatus::Passed => "The contained evaluator completed successfully.",
            LearningPracticalRunStatus::Failed => "The contained evaluator reported a failure.",
            LearningPracticalRunStatus::TimedOut => {
                "The contained evaluator exceeded its time limit."
            }
            LearningPracticalRunStatus::Cancelled => {
                "The run was cancelled and its contained evaluator stopped."
            }
            LearningPracticalRunStatus::RuntimeUnavailable => {
                "No compatible local runtime was available; no evaluation occurred."
            }
            _ => "The evaluator did not complete.",
        };
        let check = LearningPracticalCheckResultDto {
            name: "Contained evaluator".into(),
            status: check_status,
            message: message.into(),
            duration_ms: Some(execution.duration_ms),
        };
        let timestamp = now();
        let mut tx = self.pool.begin().await.map_err(db)?;
        let updated = sqlx::query("UPDATE learning_practical_runs SET status=?,stdout=?,stderr=?,output_truncated=?,exit_code=?,duration_ms=?,check_results_json=?,completed_at=? WHERE id=? AND status='running'")
            .bind(status_name(&status)).bind(&execution.stdout).bind(&execution.stderr)
            .bind(execution.output_truncated as i64).bind(execution.exit_code.map(i64::from))
            .bind(execution.duration_ms).bind(json(&vec![check.clone()])?).bind(timestamp)
            .bind(&prepared.dto.id).execute(&mut *tx).await.map_err(db)?;
        if updated.rows_affected() == 1 {
            sqlx::query("INSERT INTO learning_practical_run_checks(run_id,ordinal,name,status,message,duration_ms) VALUES(?,0,?,?,?,?)")
                .bind(&prepared.dto.id).bind(&check.name).bind(check_status_name(check.status))
                .bind(&check.message).bind(check.duration_ms).execute(&mut *tx).await.map_err(db)?;
            if !matches!(
                status,
                LearningPracticalRunStatus::RuntimeUnavailable
                    | LearningPracticalRunStatus::Cancelled
                    | LearningPracticalRunStatus::TimedOut
            ) {
                for outcome_id in &prepared.outcome_ids {
                    sqlx::query("INSERT INTO learning_evidence_events(id,program_id,outcome_id,source_kind,source_id,dimension,result,score,observation,evidence_quote,assistance_json,observed_at) VALUES(?,?,?,'practical',?,'application',?,?,?,?,?,?)")
                        .bind(uuid::Uuid::new_v4().to_string()).bind(&prepared.dto.program_id).bind(outcome_id)
                        .bind(&prepared.dto.id)
                        .bind(if status==LearningPracticalRunStatus::Passed{"observed"}else{"not_observed"})
                        .bind(if status==LearningPracticalRunStatus::Passed{1.0}else{0.0})
                        .bind(message).bind(Option::<String>::None)
                        .bind(json(&serde_json::json!({"mode":prepared.practice_mode,"allowedAids":prepared.allowed_aids}))?)
                        .bind(timestamp).execute(&mut *tx).await.map_err(db)?;
                }
            }
        }
        tx.commit().await.map_err(db)?;
        Ok(())
    }

    pub async fn cancel_run(
        &self,
        request: &CancelLearningPracticalRunRequestDto,
        payload_hash: &str,
    ) -> Result<LearningPracticalRunDto> {
        uuid(&request.operation_id, "operation")?;
        uuid(&request.program_id, "program")?;
        uuid(&request.run_id, "run")?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &request.program_id).await?;
        if let Some(row) = sqlx::query("SELECT program_id,run_id,payload_hash FROM learning_practical_run_operations WHERE operation_id=?")
            .bind(&request.operation_id).fetch_optional(&mut *tx).await.map_err(db)?
        {
            if row.get::<String, _>("program_id") == request.program_id
                && row.get::<String, _>("run_id") == request.run_id
                && row.get::<String, _>("payload_hash") == payload_hash
            {
                tx.commit().await.map_err(db)?;
                return self.run(&request.run_id).await;
            }
            return Err(invalid("Operation ID was reused with different cancellation data."));
        }
        let timestamp = now();
        let changed = sqlx::query("UPDATE learning_practical_runs SET status='cancelled',stderr='Run cancelled by the learner.',completed_at=? WHERE id=? AND program_id=? AND status IN ('pending','running')")
            .bind(timestamp).bind(&request.run_id).bind(&request.program_id)
            .execute(&mut *tx).await.map_err(db)?;
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM learning_practical_runs WHERE id=? AND program_id=?")
                .bind(&request.run_id)
                .bind(&request.program_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        if exists.is_none() {
            return Err(AppError::NotFound("Practical run not found".into()));
        }
        sqlx::query("INSERT INTO learning_practical_run_operations(operation_id,program_id,run_id,kind,payload_hash,created_at) VALUES(?,?,?,'cancel',?,?)")
            .bind(&request.operation_id).bind(&request.program_id).bind(&request.run_id)
            .bind(payload_hash).bind(timestamp).execute(&mut *tx).await.map_err(db)?;
        let changed = changed.rows_affected() == 1;
        if changed {
            sqlx::query("INSERT OR IGNORE INTO learning_practical_run_checks(run_id,ordinal,name,status,message,duration_ms) VALUES(?,0,'Contained evaluator','not_run','The run was cancelled; its contained evaluator was told to stop.',NULL)")
                .bind(&request.run_id).execute(&mut *tx).await.map_err(db)?;
        }
        tx.commit().await.map_err(db)?;
        if changed {
            // Cancel only after ownership and operation replay have been
            // validated. A UUID from another program must not stop its guest.
            if let Some(token) = ACTIVE_RUNS
                .lock()
                .map_err(|_| {
                    AppError::InternalError("Practical run registry is unavailable.".into())
                })?
                .get(&request.run_id)
                .cloned()
            {
                token.cancel();
            }
            let (engine, snapshot): (Option<String>, String) = sqlx::query(
                "SELECT engine,activity_snapshot_json FROM learning_practical_runs WHERE id=? AND program_id=?",
            )
            .bind(&request.run_id)
            .bind(&request.program_id)
            .fetch_one(&self.pool)
            .await
            .map(|row| (row.get("engine"), row.get("activity_snapshot_json")))
            .map_err(db)?;
            let is_container_run = engine.is_some();
            if let Some(engine) = engine {
                lab_runtime::cleanup_interrupted_run(parse_engine(&engine)?, &request.run_id)
                    .await?;
            }
            // If a runner is still active it owns the directory and removes it
            // only after the process exits. For an orphaned run, the persisted
            // random token proves this directory was created by that run.
            let runner_is_active = ACTIVE_RUNS
                .lock()
                .map_err(|_| {
                    AppError::InternalError("Practical run registry is unavailable.".into())
                })?
                .contains_key(&request.run_id);
            if is_container_run && !runner_is_active {
                if let Some(token) = workspace_owner_token_from_snapshot(&snapshot) {
                    remove_owned_workspace(&request.run_id, &token);
                }
            }
        }
        self.run(&request.run_id).await
    }

    pub async fn recover_running_runs(&self) -> Result<usize> {
        let rows = sqlx::query(
            "SELECT id,engine,activity_snapshot_json FROM learning_practical_runs WHERE status IN ('pending','running')",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        for row in &rows {
            let id: String = row.get("id");
            if let Some(engine) = row.get::<Option<String>, _>("engine") {
                let _ = lab_runtime::cleanup_interrupted_run(parse_engine(&engine)?, &id).await;
                if let Some(token) =
                    workspace_owner_token_from_snapshot(row.get("activity_snapshot_json"))
                {
                    remove_owned_workspace(&id, &token);
                }
            }
        }
        let timestamp = now();
        let changed = sqlx::query("UPDATE learning_practical_runs SET status='interrupted',stderr='The desktop process stopped before the run completed.',completed_at=? WHERE status IN ('pending','running')")
            .bind(timestamp).execute(&self.pool).await.map_err(db)?;
        Ok(changed.rows_affected() as usize)
    }
}

pub(super) fn run_workspace(run_id: &str) -> PathBuf {
    std::env::temp_dir()
        .join("lattice-learning-labs")
        .join(run_id)
}

pub(super) fn workspace_owner_marker(run_id: &str) -> PathBuf {
    run_workspace(run_id).with_extension("owner")
}

fn workspace_owner_token_from_snapshot(snapshot: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(snapshot)
        .ok()?
        .get("workspaceOwnerToken")?
        .as_str()
        .map(str::to_owned)
}

pub(super) fn cleanup_invocation_workspace(
    run_id: &str,
    workspace_owned: bool,
    ownership_marker_created: bool,
) {
    if !workspace_owned {
        return;
    }
    // This invocation created the directory. If marker creation failed,
    // direct cleanup is still safe because materialize_workspace succeeded.
    let _ = std::fs::remove_dir_all(run_workspace(run_id));
    if ownership_marker_created {
        let _ = std::fs::remove_file(workspace_owner_marker(run_id));
    }
}

fn create_workspace_owner_marker(run_id: &str, token: &str) -> Result<()> {
    use std::io::Write;
    let path = workspace_owner_marker(run_id);
    let mut marker = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    marker.write_all(token.as_bytes())?;
    marker.sync_all()?;
    Ok(())
}

pub(super) fn remove_owned_workspace(run_id: &str, token: &str) {
    let marker_path = workspace_owner_marker(run_id);
    if std::fs::read_to_string(&marker_path).is_ok_and(|marker| marker == token) {
        let _ = std::fs::remove_dir_all(run_workspace(run_id));
        let _ = std::fs::remove_file(marker_path);
    }
}

impl LearningPracticalRepository {
    pub async fn replay_activity_operation(
        &self,
        program_id: &str,
        operation_id: &str,
        payload_hash: &str,
    ) -> Result<Option<LearningPracticalWorkspaceDto>> {
        let row = sqlx::query("SELECT program_id,payload_hash FROM learning_practical_activity_operations WHERE operation_id=?")
            .bind(operation_id).fetch_optional(&self.pool).await.map_err(db)?;
        match row {
            Some(row)
                if row.get::<String, _>("program_id") == program_id
                    && row.get::<String, _>("payload_hash") == payload_hash =>
            {
                Ok(Some(self.workspace(program_id).await?))
            }
            Some(_) => Err(invalid(
                "Operation ID was reused with different practical activity data.",
            )),
            None => Ok(None),
        }
    }

    pub async fn replay_simulation_operation(
        &self,
        program_id: &str,
        session_id: &str,
        operation_id: &str,
        payload_hash: &str,
    ) -> Result<Option<LearningSimulationSessionDto>> {
        let row = sqlx::query("SELECT program_id,session_id,payload_hash FROM learning_simulation_operations WHERE operation_id=?")
            .bind(operation_id).fetch_optional(&self.pool).await.map_err(db)?;
        match row {
            Some(row)
                if row.get::<String, _>("program_id") == program_id
                    && row.get::<String, _>("session_id") == session_id
                    && row.get::<String, _>("payload_hash") == payload_hash =>
            {
                Ok(Some(self.simulation(session_id).await?))
            }
            Some(_) => Err(invalid(
                "Operation ID was reused with different simulation data.",
            )),
            None => Ok(None),
        }
    }

    pub async fn simulation_generation_context(
        &self,
        program_id: &str,
        activity_id: &str,
    ) -> Result<(
        super::practical_generation::SimulationActivityContext,
        Vec<String>,
    )> {
        uuid(program_id, "program")?;
        uuid(activity_id, "activity")?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        let activity = Self::internal_activity(&mut tx, program_id, activity_id).await?;
        tx.commit().await.map_err(db)?;
        Ok((
            super::practical_generation::SimulationActivityContext {
                title: activity.title,
                brief: activity.brief,
                rubric: activity.rubric,
            },
            activity.source_version_ids,
        ))
    }

    pub async fn start_simulation(
        &self,
        request: &StartLearningSimulationRequestDto,
        payload_hash: &str,
    ) -> Result<LearningSimulationSessionDto> {
        for (value, label) in [
            (&request.operation_id, "operation"),
            (&request.session_id, "simulation"),
            (&request.program_id, "program"),
            (&request.activity_id, "activity"),
        ] {
            uuid(value, label)?;
        }
        if let Some(session) = request.practice_session_id.as_deref() {
            uuid(session, "practice session")?;
        }
        for (value, label) in [
            (&request.learner_role, "Learner role"),
            (&request.counterpart_role, "Counterpart role"),
        ] {
            if value.trim().is_empty() || value.chars().count() > 100 {
                return Err(invalid(format!("{label} must contain 1–100 characters.")));
            }
        }
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &request.program_id).await?;
        if let Some(row) = sqlx::query("SELECT program_id,session_id,payload_hash FROM learning_simulation_operations WHERE operation_id=?")
            .bind(&request.operation_id).fetch_optional(&mut *tx).await.map_err(db)?
        {
            if row.get::<String, _>("program_id") == request.program_id
                && row.get::<String, _>("session_id") == request.session_id
                && row.get::<String, _>("payload_hash") == payload_hash
            {
                tx.commit().await.map_err(db)?;
                return self.simulation(&request.session_id).await;
            }
            return Err(invalid(
                "Operation ID was reused with different simulation data.",
            ));
        }
        Self::active_program(&mut tx, &request.program_id).await?;
        let activity =
            Self::internal_activity(&mut tx, &request.program_id, &request.activity_id).await?;
        if activity.revision != request.expected_activity_revision {
            return Err(invalid("Practical activity changed; reload and retry."));
        }
        if let Some(practice_id) = request.practice_session_id.as_deref() {
            let owned: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM learning_practice_sessions WHERE program_id=? AND id=?",
            )
            .bind(&request.program_id)
            .bind(practice_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
            if owned.is_none() {
                return Err(invalid("Practice session does not belong to this program."));
            }
        }
        let timestamp = now();
        sqlx::query("INSERT INTO learning_simulation_sessions(id,program_id,activity_id,activity_revision,activity_snapshot_json,practice_session_id,operation_id,payload_hash,learner_role,counterpart_role,status,revision,created_at,updated_at,submitted_at) VALUES(?,?,?,?,?,?,?,?,?,?,'active',0,?,?,NULL)")
            .bind(&request.session_id).bind(&request.program_id).bind(&request.activity_id)
            .bind(activity.revision).bind(json(&activity)?).bind(&request.practice_session_id)
            .bind(&request.operation_id).bind(payload_hash).bind(request.learner_role.trim())
            .bind(request.counterpart_role.trim()).bind(timestamp).bind(timestamp)
            .execute(&mut *tx).await.map_err(db)?;
        sqlx::query("INSERT INTO learning_simulation_operations(operation_id,program_id,session_id,kind,payload_hash,result_id,created_at) VALUES(?,?,?,'start',?,?,?)")
            .bind(&request.operation_id).bind(&request.program_id).bind(&request.session_id)
            .bind(payload_hash).bind(&request.session_id).bind(timestamp)
            .execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        self.simulation(&request.session_id).await
    }

    pub async fn append_simulation_turn(
        &self,
        request: &SendLearningSimulationTurnRequestDto,
        reply: &SimulationTurnWrite,
        payload_hash: &str,
    ) -> Result<LearningSimulationSessionDto> {
        uuid(&request.operation_id, "operation")?;
        uuid(&request.program_id, "program")?;
        uuid(&request.session_id, "simulation")?;
        if request.content.trim().is_empty() || request.content.chars().count() > 8_000 {
            return Err(invalid("Simulation turns must contain 1–8000 characters."));
        }
        if reply.content.trim().is_empty() || reply.content.chars().count() > 6_000 {
            return Err(invalid("Simulation reply must contain 1–6000 characters."));
        }
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &request.program_id).await?;
        if let Some(row) = sqlx::query("SELECT program_id,session_id,payload_hash FROM learning_simulation_operations WHERE operation_id=?")
            .bind(&request.operation_id).fetch_optional(&mut *tx).await.map_err(db)?
        {
            if row.get::<String, _>("program_id") == request.program_id
                && row.get::<String, _>("session_id") == request.session_id
                && row.get::<String, _>("payload_hash") == payload_hash
            {
                tx.commit().await.map_err(db)?;
                return self.simulation(&request.session_id).await;
            }
            return Err(invalid("Operation ID was reused with different turn data."));
        }
        let session = sqlx::query(
            "SELECT status,revision FROM learning_simulation_sessions WHERE id=? AND program_id=?",
        )
        .bind(&request.session_id)
        .bind(&request.program_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Simulation not found".into()))?;
        if session.get::<String, _>("status") != "active" {
            return Err(invalid("Only active simulations accept turns."));
        }
        if session.get::<i64, _>("revision") != request.expected_revision {
            return Err(invalid("Simulation changed; reload and retry."));
        }
        let ordinal: i64 = sqlx::query_scalar(
            "SELECT coalesce(max(ordinal),-1)+1 FROM learning_simulation_turns WHERE session_id=?",
        )
        .bind(&request.session_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        if ordinal >= 160 {
            return Err(invalid("This simulation has reached its turn limit."));
        }
        let timestamp = now();
        let learner_turn_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO learning_simulation_turns(id,program_id,session_id,operation_id,payload_hash,ordinal,speaker,content,citations_json,model_name,created_at) VALUES(?,?,?,?,?,?,'learner',?,'[]',NULL,?)")
            .bind(&learner_turn_id).bind(&request.program_id).bind(&request.session_id)
            .bind(&request.operation_id).bind(payload_hash).bind(ordinal)
            .bind(request.content.trim()).bind(timestamp).execute(&mut *tx).await.map_err(db)?;
        let reply_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO learning_simulation_turns(id,program_id,session_id,operation_id,payload_hash,ordinal,speaker,content,citations_json,model_name,created_at) VALUES(?,?,?,?,?,?,'counterpart',?,?,?,?)")
            .bind(&reply_id).bind(&request.program_id).bind(&request.session_id)
            .bind(uuid::Uuid::new_v4().to_string()).bind(digest(reply)?).bind(ordinal+1)
            .bind(reply.content.trim()).bind(json(&reply.citations)?).bind(&reply.model_name)
            .bind(timestamp).execute(&mut *tx).await.map_err(db)?;
        let updated = sqlx::query("UPDATE learning_simulation_sessions SET revision=revision+1,updated_at=? WHERE id=? AND program_id=? AND revision=? AND status='active'")
            .bind(timestamp).bind(&request.session_id).bind(&request.program_id)
            .bind(request.expected_revision).execute(&mut *tx).await.map_err(db)?;
        if updated.rows_affected() != 1 {
            return Err(invalid("Simulation changed; reload and retry."));
        }
        sqlx::query("INSERT INTO learning_simulation_operations(operation_id,program_id,session_id,kind,payload_hash,result_id,created_at) VALUES(?,?,?,'turn',?,?,?)")
            .bind(&request.operation_id).bind(&request.program_id).bind(&request.session_id)
            .bind(payload_hash).bind(&reply_id).bind(timestamp).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        self.simulation(&request.session_id).await
    }

    pub async fn finish_simulation(
        &self,
        request: &FinishLearningSimulationRequestDto,
        feedback: &SimulationTurnWrite,
        payload_hash: &str,
    ) -> Result<LearningSimulationSessionDto> {
        uuid(&request.operation_id, "operation")?;
        uuid(&request.program_id, "program")?;
        uuid(&request.session_id, "simulation")?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &request.program_id).await?;
        if let Some(row) = sqlx::query("SELECT program_id,session_id,payload_hash FROM learning_simulation_operations WHERE operation_id=?")
            .bind(&request.operation_id).fetch_optional(&mut *tx).await.map_err(db)?
        {
            if row.get::<String, _>("program_id") == request.program_id
                && row.get::<String, _>("session_id") == request.session_id
                && row.get::<String, _>("payload_hash") == payload_hash
            {
                tx.commit().await.map_err(db)?;
                return self.simulation(&request.session_id).await;
            }
            return Err(invalid("Operation ID was reused with different finish data."));
        }
        let session =
            sqlx::query("SELECT * FROM learning_simulation_sessions WHERE id=? AND program_id=?")
                .bind(&request.session_id)
                .bind(&request.program_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                .ok_or_else(|| AppError::NotFound("Simulation not found".into()))?;
        if session.get::<String, _>("status") != "active" {
            return Err(invalid("Only active simulations can be submitted."));
        }
        if session.get::<i64, _>("revision") != request.expected_revision {
            return Err(invalid("Simulation changed; reload and retry."));
        }
        let learner_turns = sqlx::query("SELECT content FROM learning_simulation_turns WHERE session_id=? AND speaker='learner' ORDER BY ordinal")
            .bind(&request.session_id).fetch_all(&mut *tx).await.map_err(db)?;
        if learner_turns.is_empty() {
            return Err(invalid(
                "Respond at least once before submitting a simulation.",
            ));
        }
        let activity: InternalActivitySnapshot = decode(session.get("activity_snapshot_json"))?;
        let ordinal: i64 = sqlx::query_scalar(
            "SELECT coalesce(max(ordinal),-1)+1 FROM learning_simulation_turns WHERE session_id=?",
        )
        .bind(&request.session_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        let timestamp = now();
        let feedback_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO learning_simulation_turns(id,program_id,session_id,operation_id,payload_hash,ordinal,speaker,content,citations_json,model_name,created_at) VALUES(?,?,?,?,?,?,'coach',?,?,?,?)")
            .bind(&feedback_id).bind(&request.program_id).bind(&request.session_id)
            .bind(uuid::Uuid::new_v4().to_string()).bind(digest(feedback)?).bind(ordinal)
            .bind(feedback.content.trim()).bind(json(&feedback.citations)?).bind(&feedback.model_name)
            .bind(timestamp).execute(&mut *tx).await.map_err(db)?;
        let updated = sqlx::query("UPDATE learning_simulation_sessions SET status='submitted',revision=revision+1,updated_at=?,submitted_at=? WHERE id=? AND program_id=? AND revision=? AND status='active'")
            .bind(timestamp).bind(timestamp).bind(&request.session_id).bind(&request.program_id)
            .bind(request.expected_revision).execute(&mut *tx).await.map_err(db)?;
        if updated.rows_affected() != 1 {
            return Err(invalid("Simulation changed; reload and retry."));
        }
        let quote = learner_turns.first().map(|row| {
            row.get::<String, _>("content")
                .chars()
                .take(500)
                .collect::<String>()
        });
        for outcome_id in &activity.outcome_ids {
            sqlx::query("INSERT INTO learning_evidence_events(id,program_id,outcome_id,source_kind,source_id,dimension,result,score,observation,evidence_quote,assistance_json,observed_at) VALUES(?,?,?,'simulation',?,'transfer','uncertain',NULL,?,?,?,?)")
                .bind(uuid::Uuid::new_v4().to_string()).bind(&request.program_id).bind(outcome_id)
                .bind(&request.session_id).bind(&feedback.content).bind(&quote)
                .bind(json(&serde_json::json!({"mode":activity.practice_mode,"allowedAids":activity.allowed_aids}))?)
                .bind(timestamp).execute(&mut *tx).await.map_err(db)?;
        }
        sqlx::query("INSERT INTO learning_simulation_operations(operation_id,program_id,session_id,kind,payload_hash,result_id,created_at) VALUES(?,?,?,'finish',?,?,?)")
            .bind(&request.operation_id).bind(&request.program_id).bind(&request.session_id)
            .bind(payload_hash).bind(&feedback_id).bind(timestamp).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        self.simulation(&request.session_id).await
    }
}

impl LearningPracticalRepository {
    /// Validate setup before contacting an engine or downloading an image, and
    /// recover a committed setup when the renderer lost its response.
    pub async fn preflight_runtime_setup(
        &self,
        request: &PrepareLearningRuntimePresetRequestDto,
        payload_hash: &str,
    ) -> Result<Option<LearningPracticalWorkspaceDto>> {
        uuid(&request.operation_id, "operation")?;
        uuid(&request.program_id, "program")?;
        uuid(&request.profile_id, "runtime profile")?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &request.program_id).await?;
        Self::active_program(&mut tx, &request.program_id).await?;
        if let Some(row) = sqlx::query("SELECT profile_id,payload_hash FROM learning_runtime_profile_operations WHERE operation_id=?")
            .bind(&request.operation_id).fetch_optional(&mut *tx).await.map_err(db)?
        {
            if row.get::<String, _>("profile_id") != request.profile_id
                || row.get::<String, _>("payload_hash") != payload_hash
            {
                return Err(invalid("Operation ID was reused with different runtime setup data."));
            }
            tx.commit().await.map_err(db)?;
            return self.workspace(&request.program_id).await.map(Some);
        }
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM learning_runtime_profiles WHERE id=?")
                .bind(&request.profile_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        if exists.is_some() {
            return Err(invalid(
                "This runtime profile already exists; refresh the available environments.",
            ));
        }
        tx.commit().await.map_err(db)?;
        Ok(None)
    }

    pub async fn runtime_generation_context(
        &self,
        profile_id: Option<&str>,
        builtin: Option<LearningBuiltinRuntime>,
    ) -> Result<Option<super::practical_generation::PracticalRuntimeContext>> {
        if let Some(runtime) = builtin {
            if profile_id.is_some() {
                return Err(invalid("Choose one execution environment for an activity."));
            }
            let capability = embedded_runtime::capabilities()
                .into_iter()
                .find(|item| item.id == runtime)
                .ok_or_else(|| invalid("Unknown built-in runtime."))?;
            if !capability.available {
                return Err(AppError::ServiceNotAvailable(
                    capability
                        .reason
                        .unwrap_or_else(|| "The built-in runtime is unavailable.".into()),
                ));
            }
            return Ok(Some(runtime.context()));
        }
        let Some(profile_id) = profile_id else {
            return Ok(None);
        };
        uuid(profile_id, "runtime profile")?;
        let profile = self
            .profiles()
            .await?
            .into_iter()
            .find(|profile| profile.id == profile_id && profile.enabled)
            .ok_or_else(|| invalid("Selected runtime profile is unavailable."))?;
        let contract = super::runtime_catalog::learning_runtime_catalog().into_iter()
            .find(|preset| preset.command == profile.command)
            .map(|preset| preset.entrypoint_contract)
            .unwrap_or_else(|| "Create files compatible with this exact runtime command. Network access and external dependency installation are unavailable.".into());
        Ok(Some(super::practical_generation::PracticalRuntimeContext {
            name: profile.name,
            command: profile.command,
            contract,
        }))
    }

    pub async fn save_runtime_profile(
        &self,
        request: &SaveLearningRuntimeProfileRequestDto,
        payload_hash: &str,
    ) -> Result<LearningPracticalWorkspaceDto> {
        uuid(&request.operation_id, "operation")?;
        uuid(&request.program_id, "program")?;
        uuid(&request.profile_id, "runtime profile")?;
        let name = request.name.trim();
        if name.is_empty() || name.chars().count() > 120 {
            return Err(invalid(
                "Runtime profile names must contain 1–120 characters.",
            ));
        }
        // Reuse the execution boundary to validate immutable image IDs, trusted
        // argv, and bounded limits. The placeholder file is never executed.
        lab_runtime::validate_execution_spec(&LearningLabExecutionSpec {
            run_id: uuid::Uuid::new_v4().to_string(),
            image_id: request.image_id.clone(),
            command: request.command.clone(),
            files: vec![LearningLabFile {
                path: "profile-validation.txt".into(),
                content: "validation".into(),
            }],
            read_only_paths: vec![],
            limits: request.limits.clone(),
        })?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &request.program_id).await?;
        Self::active_program(&mut tx, &request.program_id).await?;
        if let Some(row) = sqlx::query("SELECT profile_id,payload_hash FROM learning_runtime_profile_operations WHERE operation_id=?")
            .bind(&request.operation_id).fetch_optional(&mut *tx).await.map_err(db)?
        {
            if row.get::<String, _>("profile_id") == request.profile_id
                && row.get::<String, _>("payload_hash") == payload_hash
            {
                tx.commit().await.map_err(db)?;
                return self.workspace(&request.program_id).await;
            }
            return Err(invalid(
                "Operation ID was reused with different runtime profile data.",
            ));
        }
        let timestamp = now();
        let result_revision = if let Some(expected) = request.expected_revision {
            let updated = sqlx::query("UPDATE learning_runtime_profiles SET name=?,engine=?,image_id=?,command_json=?,limits_json=?,revision=revision+1,updated_at=? WHERE id=? AND revision=?")
                .bind(name).bind(engine_name(request.engine)).bind(&request.image_id)
                .bind(json(&request.command)?).bind(json(&request.limits)?).bind(timestamp)
                .bind(&request.profile_id).bind(expected).execute(&mut *tx).await.map_err(db)?;
            if updated.rows_affected() != 1 {
                return Err(invalid("Runtime profile changed; reload and retry."));
            }
            expected + 1
        } else {
            sqlx::query("INSERT INTO learning_runtime_profiles(id,name,engine,image_id,command_json,limits_json,enabled,revision,created_at,updated_at) VALUES(?,?,?,?,?,?,1,0,?,?)")
                .bind(&request.profile_id).bind(name).bind(engine_name(request.engine))
                .bind(&request.image_id).bind(json(&request.command)?).bind(json(&request.limits)?)
                .bind(timestamp).bind(timestamp).execute(&mut *tx).await.map_err(db)?;
            0
        };
        sqlx::query("INSERT INTO learning_runtime_profile_operations(operation_id,profile_id,payload_hash,result_revision,created_at) VALUES(?,?,?,?,?)")
            .bind(&request.operation_id).bind(&request.profile_id).bind(payload_hash)
            .bind(result_revision).bind(timestamp).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        self.workspace(&request.program_id).await
    }

    pub async fn save_generated_activity(
        &self,
        request: &GenerateLearningPracticalActivityRequestDto,
        generated: &GeneratedPracticalActivity,
        generator_model: &str,
        payload_hash: &str,
    ) -> Result<LearningPracticalWorkspaceDto> {
        if request.builtin_runtime.is_some() && request.runtime_profile_id.is_some() {
            return Err(invalid("Choose one execution environment for an activity."));
        }
        if let Some(runtime) = request.builtin_runtime {
            let spec = BuiltinLabExecutionSpec {
                run_id: request.activity_id.clone(),
                runtime,
                files: generated
                    .files
                    .iter()
                    .map(|file| LearningLabFile {
                        path: file.path.clone(),
                        content: file.content.clone(),
                    })
                    .collect(),
                read_only_paths: generated
                    .files
                    .iter()
                    .filter(|file| {
                        file.role == PracticalFileRole::Check
                            || file.role == PracticalFileRole::Reference
                    })
                    .map(|file| file.path.clone())
                    .collect(),
                limits: LearningLabLimits::default(),
            };
            spec.validate()?;
            if !generated.files.iter().any(|file| {
                file.path == runtime.entrypoint() && file.role == PracticalFileRole::Check
            }) {
                return Err(invalid("Built-in activities require an independent evaluator at the runtime entrypoint."));
            }
        }
        for (value, label) in [
            (&request.operation_id, "operation"),
            (&request.activity_id, "activity"),
            (&request.program_id, "program"),
            (&request.lesson_id, "lesson"),
        ] {
            uuid(value, label)?;
        }
        if request.allowed_aids.len() > 24
            || request
                .allowed_aids
                .iter()
                .any(|aid| aid.trim().is_empty() || aid.chars().count() > 160)
        {
            return Err(invalid("Practical aid descriptions must be bounded."));
        }
        if generator_model.trim().is_empty() || generator_model.chars().count() > 240 {
            return Err(invalid(
                "Practical generation must record a model identity.",
            ));
        }
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &request.program_id).await?;
        if let Some(row) = sqlx::query("SELECT program_id,activity_id,payload_hash FROM learning_practical_activity_operations WHERE operation_id=?")
            .bind(&request.operation_id).fetch_optional(&mut *tx).await.map_err(db)?
        {
            if row.get::<String, _>("program_id") == request.program_id
                && row.get::<String, _>("activity_id") == request.activity_id
                && row.get::<String, _>("payload_hash") == payload_hash
            {
                tx.commit().await.map_err(db)?;
                return self.workspace(&request.program_id).await;
            }
            return Err(invalid(
                "Operation ID was reused with different practical activity data.",
            ));
        }
        let revision = Self::active_program(&mut tx, &request.program_id).await?;
        if revision != request.expected_program_revision {
            return Err(invalid(
                "The learning program changed; regenerate this activity.",
            ));
        }
        let module_id: Option<String> = sqlx::query_scalar(
            "SELECT module_id FROM learning_lessons WHERE id=? AND program_id=?",
        )
        .bind(&request.lesson_id)
        .bind(&request.program_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        let module_id =
            module_id.ok_or_else(|| AppError::NotFound("Learning lesson not found".into()))?;
        let outcome_ids = sqlx::query_scalar::<_, String>("SELECT id FROM learning_outcome_definitions WHERE program_id=? AND (lesson_id=? OR (lesson_id IS NULL AND module_id=?)) ORDER BY ordinal,id")
            .bind(&request.program_id).bind(&request.lesson_id).bind(&module_id)
            .fetch_all(&mut *tx).await.map_err(db)?;
        for source_id in &generated.source_version_ids {
            let owned: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM learning_source_versions WHERE program_id=? AND id=?",
            )
            .bind(&request.program_id)
            .bind(source_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
            if owned.is_none() {
                return Err(invalid(
                    "Practical activity source version is outside this program.",
                ));
            }
        }
        let (runtime_kind, profile_id, runtime_engine, image_id, command, limits) =
            if let Some(profile_id) = request.runtime_profile_id.as_deref() {
                uuid(profile_id, "runtime profile")?;
                let profile =
                    sqlx::query("SELECT * FROM learning_runtime_profiles WHERE id=? AND enabled=1")
                        .bind(profile_id)
                        .fetch_optional(&mut *tx)
                        .await
                        .map_err(db)?
                        .ok_or_else(|| invalid("Selected runtime profile is unavailable."))?;
                (
                    "container",
                    Some(profile_id.to_owned()),
                    Some(profile.get::<String, _>("engine")),
                    Some(profile.get::<String, _>("image_id")),
                    Some(profile.get::<String, _>("command_json")),
                    Some(profile.get::<String, _>("limits_json")),
                )
            } else {
                ("none", None, None, None, None, None)
            };
        if (runtime_kind == "container" || request.builtin_runtime.is_some())
            && generated
                .files
                .iter()
                .all(|file| file.role != PracticalFileRole::Check)
        {
            return Err(invalid(
                "Executable practical activities require independent evaluator files.",
            ));
        }
        let timestamp = now();
        sqlx::query("INSERT INTO learning_practical_activities(id,program_id,lesson_id,predecessor_id,kind,title,brief,status,practice_mode,allowed_aids_json,outcome_ids_json,source_version_ids_json,rubric_json,runtime_kind,runtime_profile_id,runtime_engine,runtime_image_id,runtime_command_json,runtime_limits_json,generator_model,revision,created_at,updated_at) VALUES(?,?,?,NULL,?,?,?,'ready',?,?,?,?,?,?,?,?,?,?,?,?,0,?,?)")
            .bind(&request.activity_id).bind(&request.program_id).bind(&request.lesson_id)
            .bind(kind_name(request.kind)).bind(generated.title.trim()).bind(generated.brief.trim())
            .bind(mode_name(&request.practice_mode)).bind(json(&request.allowed_aids)?)
            .bind(json(&outcome_ids)?).bind(json(&generated.source_version_ids)?)
            .bind(json(&generated.rubric)?).bind(runtime_kind).bind(profile_id)
            .bind(runtime_engine).bind(image_id).bind(command).bind(limits).bind(generator_model)
            .bind(timestamp).bind(timestamp).execute(&mut *tx).await.map_err(db)?;
        if let Some(runtime) = request.builtin_runtime {
            sqlx::query("UPDATE learning_practical_activities SET builtin_runtime=?,builtin_runtime_version=? WHERE id=?")
                .bind(runtime.as_str()).bind(runtime.version()).bind(&request.activity_id).execute(&mut *tx).await.map_err(db)?;
        }
        let mut paths = HashSet::new();
        for (ordinal, file) in generated.files.iter().enumerate() {
            if !paths.insert(file.path.as_str()) {
                return Err(invalid("Practical activity contains duplicate file paths."));
            }
            let role = match file.role {
                PracticalFileRole::Starter => "starter",
                PracticalFileRole::Check => "check",
                PracticalFileRole::Reference => "reference",
            };
            sqlx::query("INSERT INTO learning_practical_files(activity_id,ordinal,path,role,content,content_sha256,editable) VALUES(?,?,?,?,?,?,?)")
                .bind(&request.activity_id).bind(ordinal as i64).bind(&file.path).bind(role)
                .bind(&file.content).bind(format!("{:x}",Sha256::digest(file.content.as_bytes())))
                .bind(file.editable as i64).execute(&mut *tx).await.map_err(db)?;
        }
        sqlx::query("INSERT INTO learning_practical_activity_operations(operation_id,program_id,activity_id,kind,payload_hash,result_revision,created_at) VALUES(?,?,?,'generate',?,0,?)")
            .bind(&request.operation_id).bind(&request.program_id).bind(&request.activity_id)
            .bind(payload_hash).bind(timestamp).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        self.workspace(&request.program_id).await
    }

    async fn internal_activity(
        tx: &mut Transaction<'_, Sqlite>,
        program_id: &str,
        activity_id: &str,
    ) -> Result<InternalActivitySnapshot> {
        let row = sqlx::query("SELECT a.*,p.enabled AS runtime_enabled FROM learning_practical_activities a LEFT JOIN learning_runtime_profiles p ON p.id=a.runtime_profile_id WHERE a.program_id=? AND a.id=?")
            .bind(program_id).bind(activity_id).fetch_optional(&mut **tx).await.map_err(db)?
            .ok_or_else(||AppError::NotFound("Practical activity not found".into()))?;
        if row.get::<String, _>("status") != "ready" {
            return Err(invalid("Only ready practical activities can be used."));
        }
        let file_rows = sqlx::query(
            "SELECT * FROM learning_practical_files WHERE activity_id=? ORDER BY ordinal",
        )
        .bind(activity_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(db)?;
        let files = file_rows
            .into_iter()
            .map(|file| InternalActivityFile {
                path: file.get("path"),
                role: file.get("role"),
                content: file.get("content"),
                content_sha256: file.get("content_sha256"),
                editable: file.get::<i64, _>("editable") != 0,
            })
            .collect();
        Ok(InternalActivitySnapshot {
            id: row.get("id"),
            program_id: row.get("program_id"),
            lesson_id: row.get("lesson_id"),
            kind: parse_kind(row.get("kind"))?,
            title: row.get("title"),
            brief: row.get("brief"),
            practice_mode: parse_mode(row.get("practice_mode"))?,
            allowed_aids: decode(row.get("allowed_aids_json"))?,
            outcome_ids: decode(row.get("outcome_ids_json"))?,
            source_version_ids: decode(row.get("source_version_ids_json"))?,
            rubric: decode(row.get("rubric_json"))?,
            runtime_profile_id: row.get("runtime_profile_id"),
            builtin_runtime: row
                .get::<Option<String>, _>("builtin_runtime")
                .as_deref()
                .map(LearningBuiltinRuntime::parse)
                .transpose()?,
            builtin_runtime_version: row.get("builtin_runtime_version"),
            workspace_owner_token: None,
            runtime_enabled: row.get::<Option<i64>, _>("runtime_enabled").unwrap_or(0) != 0,
            runtime_engine: row
                .get::<Option<String>, _>("runtime_engine")
                .as_deref()
                .map(parse_engine)
                .transpose()?,
            runtime_image_id: row.get("runtime_image_id"),
            runtime_command: row
                .get::<Option<String>, _>("runtime_command_json")
                .as_deref()
                .map(decode)
                .transpose()?,
            runtime_limits: row
                .get::<Option<String>, _>("runtime_limits_json")
                .as_deref()
                .map(decode)
                .transpose()?,
            files,
            revision: row.get("revision"),
            generator_model: row.get("generator_model"),
        })
    }
}
