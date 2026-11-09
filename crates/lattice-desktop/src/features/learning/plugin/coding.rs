//! Tauri IPC commands for coding sessions (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md section 7.5).

use crate::interfaces::di::Container;
use lattice_core::shared::ipc::ApiError;
use lattice_learning::lab_runtime::LearningLabFile;
use lattice_learning::practice::coding::problem::{CodingTask, Difficulty, Setting};
use lattice_learning::practice::coding::session::{
    CodingExampleDto, CodingSessionDto, CodingStage, RunScope,
};
use serde::{Deserialize, Serialize};
use tauri::State;

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
///
/// Stage 1 non-panicking stub: returns an error indicating it is not yet implemented.
#[tauri::command]
#[specta::specta]
pub async fn start_coding_build(
    _request: StartCodingBuildRequestDto,
    _container: State<'_, Container>,
) -> Result<StartedCodingBuildDto, ApiError> {
    Err(ApiError::internal(
        "Stage 1 stub: start_coding_build not implemented",
    ))
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
    _request: StartCodingSessionRequestDto,
    _container: State<'_, Container>,
) -> Result<CodingSessionDto, ApiError> {
    Err(ApiError::internal("not implemented"))
}

/// Retrieves the current state of a coding session, redacted by setting and stage.
#[tauri::command]
#[specta::specta]
pub async fn get_coding_session(
    _session_id: String,
    _container: State<'_, Container>,
) -> Result<CodingSessionDto, ApiError> {
    Err(ApiError::internal("not implemented"))
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
    _request: MoveCodingStageRequestDto,
    _container: State<'_, Container>,
) -> Result<CodingSessionDto, ApiError> {
    Err(ApiError::internal("not implemented"))
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
    _request: CheckCodingExampleRequestDto,
    _container: State<'_, Container>,
) -> Result<CodingExampleDto, ApiError> {
    Err(ApiError::internal("not implemented"))
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
    _request: RunCodingSessionRequestDto,
    _container: State<'_, Container>,
) -> Result<CodingSessionDto, ApiError> {
    Err(ApiError::internal("not implemented"))
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
    _request: AskCodingHintRequestDto,
    _container: State<'_, Container>,
) -> Result<CodingSessionDto, ApiError> {
    Err(ApiError::internal("not implemented"))
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
    _request: SendCodingTurnRequestDto,
    _container: State<'_, Container>,
) -> Result<CodingSessionDto, ApiError> {
    Err(ApiError::internal("not implemented"))
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
    _request: ObserveCodingDraftRequestDto,
    _container: State<'_, Container>,
) -> Result<CodingSessionDto, ApiError> {
    Err(ApiError::internal("not implemented"))
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
    _request: SetCodingClockRequestDto,
    _container: State<'_, Container>,
) -> Result<CodingSessionDto, ApiError> {
    Err(ApiError::internal("not implemented"))
}
