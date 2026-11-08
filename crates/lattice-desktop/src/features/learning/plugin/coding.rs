//! Tauri IPC commands for coding sessions (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md section 7.5).

use crate::interfaces::di::Container;
use lattice_core::shared::ipc::ApiError;
use lattice_learning::practice::coding::jobs::{CODING_BUILD, CODING_BUILD_STEPS};
use lattice_learning::practice::coding::problem::{CodingTask, Difficulty, Setting};
use serde::{Deserialize, Serialize};
use sha2::Digest;
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

    let job = container.jobs().submit(&new_job).await.map_err(ApiError::from)?;
    container.jobs().submitted(&job.id).await;

    Ok(StartedCodingBuildDto { job_id: job.id })
}
