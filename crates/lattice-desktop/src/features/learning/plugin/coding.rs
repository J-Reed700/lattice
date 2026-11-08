//! Tauri IPC commands for coding sessions (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md section 7.5).

use crate::interfaces::di::Container;
use lattice_core::shared::ipc::ApiError;
use lattice_learning::practice::coding::problem::{CodingTask, Difficulty, Setting};
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
