//! What the learner sees of the course plan: its state, each lesson's topics,
//! whether they can still be changed, and the entry the last change replaced.

use super::{outline_revision, PlannedLesson};
use lattice_core::shared::error::Result;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CoursePlanStateDto {
    None,
    Planning,
    Planned,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CoursePlanEntryDto {
    pub topics: PlannedLesson,
    pub entry_revision: i64,
    pub editable: bool,
    pub previous: Option<PlannedLesson>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CoursePlanViewDto {
    pub program_id: String,
    pub outline_revision: i64,
    pub state: CoursePlanStateDto,
    pub error: Option<String>,
    pub entries: Vec<CoursePlanEntryDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanLearningCourseRequestDto {
    pub program_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EditCoursePlanEntryRequestDto {
    pub program_id: String,
    pub outline_revision: i64,
    pub expected_entry_revision: i64,
    pub topics: PlannedLesson,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CoursePlanEntryRefDto {
    pub lesson_id: String,
    pub expected_entry_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChangeCoursePlanEntriesRequestDto {
    pub program_id: String,
    pub outline_revision: i64,
    pub lessons: Vec<CoursePlanEntryRefDto>,
    pub instruction: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UndoCoursePlanEntryRequestDto {
    pub program_id: String,
    pub outline_revision: i64,
    pub lesson_id: String,
    pub expected_entry_revision: i64,
}

/// The course plan as the learner sees it.
pub async fn view(pool: &SqlitePool, program_id: &str) -> Result<CoursePlanViewDto> {
    Ok(CoursePlanViewDto {
        program_id: program_id.to_owned(),
        outline_revision: outline_revision(pool, program_id).await?,
        state: CoursePlanStateDto::None,
        error: None,
        entries: Vec::new(),
    })
}
