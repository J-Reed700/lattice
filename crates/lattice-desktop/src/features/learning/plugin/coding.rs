//! Tauri IPC commands for coding sessions
//! (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md section 7.5).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use tauri::State;
use tokio_util::sync::CancellationToken;

use crate::interfaces::di::Container;
use lattice_core::shared::error::Result as CoreResult;
use lattice_core::shared::ipc::ApiError;
use lattice_learning::drills::runner::{runner_for, CodeRunner, RunOutcome};
use lattice_learning::embedded_runtime::BuiltinRuntimes;
use lattice_learning::lab_runtime::LearningLabFile;
use lattice_learning::practice::coding::harness::python::PythonHarness;
use lattice_learning::practice::coding::jobs::{CODING_BUILD, CODING_BUILD_STEPS};
use lattice_learning::practice::coding::problem::{CodingTask, Difficulty, Setting};
use lattice_learning::practice::coding::session::{
    self, CodingExampleDto, CodingSessionDto, CodingStage, RunScope,
};

/// Fallback runner when no built-in Python bundle is available.
struct FallbackRunner;

#[async_trait]
impl CodeRunner for FallbackRunner {
    fn name(&self) -> &str {
        "fallback"
    }
    fn entrypoint(&self) -> &str {
        "checks.py"
    }
    fn contract(&self) -> &str {
        "fallback"
    }
    fn running(&self, _path: &str) -> String {
        String::new()
    }
    async fn run(
        &self,
        _files: &[lattice_core::domain::learning::drills::DrillFile],
        _test: &str,
        _cancel: CancellationToken,
    ) -> CoreResult<RunOutcome> {
        Ok(RunOutcome::Failed {
            output: String::new(),
        })
    }
    async fn run_capturing(
        &self,
        _files: &[LearningLabFile],
        _read_only_paths: &[String],
        _cancel: CancellationToken,
    ) -> CoreResult<(RunOutcome, String)> {
        Ok((
            RunOutcome::Failed {
                output: String::new(),
            },
            String::new(),
        ))
    }
}

/// Request payload to queue a coding problem build job.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StartCodingBuildRequestDto {
    pub program_id: String,
    pub lesson_id: Option<String>,
    pub task: CodingTask,
    pub setting: Setting,
    pub language: String,
    pub difficulty: Difficulty,
    pub focus: Option<String>,
}

/// Response payload containing the queued job identifier.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StartedCodingBuildDto {
    pub job_id: String,
}

/// Queues a coding problem build job that generates and proves a problem before saving it.
#[tauri::command]
#[specta::specta]
pub async fn start_coding_build(
    request: StartCodingBuildRequestDto,
    container: State<'_, Container>,
) -> Result<StartedCodingBuildDto, ApiError> {
    let operation_id = uuid::Uuid::new_v4().to_string();
    let requested = serde_json::to_value(&request)
        .map_err(|e| ApiError::internal(format!("Failed to serialize build request: {e}")))?;
    let payload_hash = format!(
        "{:x}",
        sha2::Sha256::digest(requested.to_string().as_bytes())
    );

    let new_job = lattice_core::shared::runtime::jobs::NewJob {
        kind: CODING_BUILD.into(),
        subject_id: Some(format!("{}:coding", request.program_id)),
        operation_id,
        payload_hash,
        requested,
        progress_total: CODING_BUILD_STEPS,
        message: "Waiting to build coding problem".into(),
    };

    let job = container
        .jobs()
        .submit(&new_job)
        .await
        .map_err(ApiError::from)?;
    container.jobs().submitted(&job.id).await;

    Ok(StartedCodingBuildDto { job_id: job.id })
}

/// Request to create and start a new coding session.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StartCodingSessionRequestDto {
    pub operation_id: String,
    pub session_id: String,
    pub program_id: String,
    pub activity_id: String,
    pub setting: Setting,
    pub budget_seconds: Option<u32>,
}

/// Creates a new coding session and returns the initial session state.
#[tauri::command]
#[specta::specta]
pub async fn start_coding_session(
    request: StartCodingSessionRequestDto,
    container: State<'_, Container>,
) -> Result<CodingSessionDto, ApiError> {
    session::start_coding_session(
        container.db_pool(),
        &request.session_id,
        &request.program_id,
        &request.activity_id,
        request.setting,
        request.budget_seconds,
    )
    .await
    .map_err(ApiError::from)
}

/// Retrieves the current state of a coding session, redacted by setting and stage.
#[tauri::command]
#[specta::specta]
pub async fn get_coding_session(
    session_id: String,
    container: State<'_, Container>,
) -> Result<CodingSessionDto, ApiError> {
    session::get_coding_session(container.db_pool(), &session_id)
        .await
        .map_err(ApiError::from)
}

/// Request to advance a coding session to a new stage.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MoveCodingStageRequestDto {
    pub operation_id: String,
    pub session_id: String,
    pub stage: CodingStage,
    pub expected_revision: i64,
}

/// Transitions the session to a new stage and cues the counterpart if appropriate.
#[tauri::command]
#[specta::specta]
pub async fn move_coding_stage(
    request: MoveCodingStageRequestDto,
    container: State<'_, Container>,
) -> Result<CodingSessionDto, ApiError> {
    session::move_coding_stage(
        container.db_pool(),
        &request.session_id,
        request.stage,
        request.expected_revision,
    )
    .await
    .map_err(ApiError::from)
}

/// Request to verify a learner-provided example.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CheckCodingExampleRequestDto {
    pub operation_id: String,
    pub session_id: String,
    pub args_json: String,
    pub expected_json: String,
}

/// Checks a learner example against the reference solution and misreadings.
#[tauri::command]
#[specta::specta]
pub async fn check_coding_example(
    request: CheckCodingExampleRequestDto,
    container: State<'_, Container>,
    runtimes: State<'_, BuiltinRuntimes>,
) -> Result<CodingExampleDto, ApiError> {
    let runner = runner_for("python", &runtimes).unwrap_or_else(|| Box::new(FallbackRunner));
    let harness = PythonHarness::new();
    session::check_coding_example(
        container.db_pool(),
        runner.as_ref(),
        &harness,
        &request.session_id,
        &request.args_json,
        &request.expected_json,
    )
    .await
    .map_err(ApiError::from)
}

/// Request to execute code under a specified test scope.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RunCodingSessionRequestDto {
    pub operation_id: String,
    pub session_id: String,
    pub scope: RunScope,
    pub files: Vec<LearningLabFile>,
    pub custom_input_json: Option<String>,
}

/// Runs draft code against examples, all cases (submit), or custom input.
#[tauri::command]
#[specta::specta]
pub async fn run_coding_session(
    request: RunCodingSessionRequestDto,
    container: State<'_, Container>,
    runtimes: State<'_, BuiltinRuntimes>,
) -> Result<CodingSessionDto, ApiError> {
    let runner = runner_for("python", &runtimes).unwrap_or_else(|| Box::new(FallbackRunner));
    let harness = PythonHarness::new();
    session::run_coding_session(
        container.db_pool(),
        runner.as_ref(),
        &harness,
        &request.session_id,
        request.scope,
        request.files,
        request.custom_input_json,
    )
    .await
    .map_err(ApiError::from)
}

/// Request to unlock the next rung of help on the hint ladder.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AskCodingHintRequestDto {
    pub operation_id: String,
    pub session_id: String,
    pub expected_revision: i64,
}

/// Unlocks and returns the next checked hint rung.
#[tauri::command]
#[specta::specta]
pub async fn ask_coding_hint(
    request: AskCodingHintRequestDto,
    container: State<'_, Container>,
    runtimes: State<'_, BuiltinRuntimes>,
) -> Result<CodingSessionDto, ApiError> {
    let pin = container
        .learning_llm(Default::default())
        .await
        .map_err(ApiError::from)?;
    let runner = runner_for("python", &runtimes).unwrap_or_else(|| Box::new(FallbackRunner));
    let harness = PythonHarness::new();
    session::ask_coding_hint(
        container.db_pool(),
        pin.llm.as_ref(),
        runner.as_ref(),
        &harness,
        &request.session_id,
        request.expected_revision,
    )
    .await
    .map_err(ApiError::from)
}

/// Request to send a learner message in the conversation thread.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SendCodingTurnRequestDto {
    pub operation_id: String,
    pub session_id: String,
    pub content: String,
    pub spoken: bool,
    pub expected_revision: i64,
}

/// Records a learner turn and generates a counterpart reply if cued.
#[tauri::command]
#[specta::specta]
pub async fn send_coding_turn(
    request: SendCodingTurnRequestDto,
    container: State<'_, Container>,
    runtimes: State<'_, BuiltinRuntimes>,
) -> Result<CodingSessionDto, ApiError> {
    let pin = container
        .learning_llm(Default::default())
        .await
        .map_err(ApiError::from)?;
    let runner = runner_for("python", &runtimes).unwrap_or_else(|| Box::new(FallbackRunner));
    let harness = PythonHarness::new();
    session::send_coding_turn(
        container.db_pool(),
        pin.llm.as_ref(),
        runner.as_ref(),
        &harness,
        &request.session_id,
        &request.content,
        request.spoken,
        request.expected_revision,
    )
    .await
    .map_err(ApiError::from)
}

/// Request to register a draft checkpoint after typing pauses in Watched setting.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ObserveCodingDraftRequestDto {
    pub session_id: String,
    pub files: Vec<LearningLabFile>,
}

/// Evaluates draft checkpoint in Watched setting; counterpart may speak or remain silent.
#[tauri::command]
#[specta::specta]
pub async fn observe_coding_draft(
    request: ObserveCodingDraftRequestDto,
    container: State<'_, Container>,
    runtimes: State<'_, BuiltinRuntimes>,
) -> Result<CodingSessionDto, ApiError> {
    let pin = container
        .learning_llm(Default::default())
        .await
        .map_err(ApiError::from)?;
    let runner = runner_for("python", &runtimes).unwrap_or_else(|| Box::new(FallbackRunner));
    let harness = PythonHarness::new();
    session::observe_coding_draft(
        container.db_pool(),
        pin.llm.as_ref(),
        runner.as_ref(),
        &harness,
        &request.session_id,
        request.files,
    )
    .await
    .map_err(ApiError::from)
}

/// Request to update active clock accounting.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SetCodingClockRequestDto {
    pub session_id: String,
    pub open: bool,
}

/// Pauses or resumes the active elapsed clock for the session.
#[tauri::command]
#[specta::specta]
pub async fn set_coding_clock(
    request: SetCodingClockRequestDto,
    container: State<'_, Container>,
) -> Result<CodingSessionDto, ApiError> {
    session::set_coding_clock(container.db_pool(), &request.session_id, request.open)
        .await
        .map_err(ApiError::from)
}
