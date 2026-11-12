//! What the learner sees of the course plan: its state, each lesson's topics,
//! whether they can still be changed, and the entry the last change replaced.

use super::repository::{
    content_revision, latest_attempt, locked_lessons, newest_replaced, outline_lessons,
    saved_entries,
};
use super::PlannedLesson;
use crate::persistence::db;
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::Database;
use lattice_core::shared::runtime::jobs::JobStatus;
use serde::{Deserialize, Serialize};

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

/// A lesson of the outline, in course order.
pub(super) struct OutlineLesson {
    pub id: String,
    /// Still in outline and not completed: nothing has been written from its
    /// plan.
    pub open: bool,
}

/// The course plan as the learner sees it, read in one snapshot.
pub async fn view(database: &Database, program_id: &str) -> Result<CoursePlanViewDto> {
    let mut tx = database.begin_read().await.map_err(db)?;
    let revision = content_revision(&mut tx, program_id).await?;
    let lessons = outline_lessons(&mut tx, program_id).await?;
    let mut saved = saved_entries(&mut tx, program_id, revision).await?;
    let planned =
        !lessons.is_empty() && lessons.iter().all(|lesson| saved.contains_key(&lesson.id));
    if !planned {
        let (state, error) = match latest_attempt(&mut tx, program_id, revision).await? {
            Some(job) if matches!(job.status, JobStatus::Pending | JobStatus::Running) => {
                (CoursePlanStateDto::Planning, None)
            }
            Some(job) if matches!(job.status, JobStatus::Failed | JobStatus::Interrupted) => (
                CoursePlanStateDto::Failed,
                Some(
                    job.error_message
                        .unwrap_or_else(|| "The course plan could not be made.".to_owned()),
                ),
            ),
            _ => (CoursePlanStateDto::None, None),
        };
        return Ok(CoursePlanViewDto {
            program_id: program_id.to_owned(),
            outline_revision: revision,
            state,
            error,
            entries: Vec::new(),
        });
    }
    let locked = locked_lessons(&mut tx, program_id).await?;
    let mut replaced = newest_replaced(&mut tx, program_id, revision).await?;
    let entries = lessons
        .iter()
        .filter_map(|lesson| {
            let (topics, entry_revision) = saved.remove(&lesson.id)?;
            Some(CoursePlanEntryDto {
                topics,
                entry_revision,
                editable: lesson.open && !locked.contains(&lesson.id),
                previous: replaced.remove(&lesson.id),
            })
        })
        .collect();
    Ok(CoursePlanViewDto {
        program_id: program_id.to_owned(),
        outline_revision: revision,
        state: CoursePlanStateDto::Planned,
        error: None,
        entries,
    })
}
