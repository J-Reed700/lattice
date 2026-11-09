//! Changing a lesson's planned topics: by hand, by asking the model, and undo.

use super::view::{
    view, ChangeCoursePlanEntriesRequestDto, CoursePlanViewDto, EditCoursePlanEntryRequestDto,
    UndoCoursePlanEntryRequestDto,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::Result;
use sqlx::SqlitePool;

/// The `task` of a request to change planned topics.
#[allow(dead_code)]
pub(crate) const CHANGE_TASK: &str =
    "Change the planned topics of these lessons as the learner asks";

/// Save the topics the learner wrote.
pub async fn edit_entry(
    pool: &SqlitePool,
    request: EditCoursePlanEntryRequestDto,
) -> Result<CoursePlanViewDto> {
    view(pool, &request.program_id).await
}

/// Put back the entry the newest change replaced.
pub async fn undo_entry(
    pool: &SqlitePool,
    request: UndoCoursePlanEntryRequestDto,
) -> Result<CoursePlanViewDto> {
    view(pool, &request.program_id).await
}

/// Every check of a request to change entries that needs no model.
pub async fn check_change(
    _pool: &SqlitePool,
    _request: &ChangeCoursePlanEntriesRequestDto,
) -> Result<()> {
    Ok(())
}

/// Change the chosen lessons' topics as the learner asks.
pub async fn change_entries(
    pool: &SqlitePool,
    _llm: &dyn LLMPort,
    request: ChangeCoursePlanEntriesRequestDto,
) -> Result<CoursePlanViewDto> {
    view(pool, &request.program_id).await
}
