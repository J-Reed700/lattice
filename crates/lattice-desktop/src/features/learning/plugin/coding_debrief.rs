//! Tauri commands for coding session finish/debrief and practice due items
//! (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md sections 4.8 and 4.9).

use crate::interfaces::di::Container;
use lattice_core::shared::ipc::ApiError;
use lattice_learning::practice::coding::patterns::PracticeDueDto;
use lattice_learning::practice::coding::session::CodingSessionDto;
use serde::{Deserialize, Serialize};
use tauri::State;

/// Request to finish a coding session and compute its debrief.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FinishCodingSessionRequestDto {
    pub operation_id: String,
    pub session_id: String,
    pub expected_revision: i64,
}

/// Finishes a coding session: validates expected_revision, computes scorecard
/// and debrief, updates pattern schedules, and returns the final CodingSessionDto.
#[tauri::command]
#[specta::specta]
pub async fn finish_coding_session(
    _request: FinishCodingSessionRequestDto,
    _container: State<'_, Container>,
) -> Result<CodingSessionDto, ApiError> {
    Err(ApiError::internal("finish_coding_session stub"))
}

/// Loads all due items for the Practice area: problem patterns and due lessons.
/// Secret pattern names remain hidden behind "Problem patterns".
#[tauri::command]
#[specta::specta]
pub async fn practice_due(
    _program_id: String,
    _container: State<'_, Container>,
) -> Result<PracticeDueDto, ApiError> {
    Err(ApiError::internal("practice_due stub"))
}
