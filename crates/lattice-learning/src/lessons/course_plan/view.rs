//! What the learner sees of the course plan: its state, each lesson's topics,
//! whether they can still be changed, and the entry the last change replaced.

use super::job::latest_attempt;
use super::repository::{self, content_revision};
use super::PlannedLesson;
use crate::persistence::db;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Transactions;
use lattice_core::shared::runtime::jobs::{JobStatus, JobStore};
use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, SqlitePool};
use std::collections::{HashMap, HashSet};

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

pub(super) async fn outline_lessons(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<OutlineLesson>> {
    let rows = repository::outline_rows(connection, program_id).await?;
    Ok(rows
        .into_iter()
        .map(|(id, preparation, completed)| OutlineLesson {
            id,
            open: preparation == "outline" && completed == 0,
        })
        .collect())
}

/// The lessons a running `lesson_preparation` job covers. A pending job does
/// not lock: the writer reads the plan only once its job is claimed.
pub(super) async fn locked_lessons(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<HashSet<String>> {
    let jobs = JobStore::list_for_subject_in(
        connection,
        crate::curriculum_repository::LESSON_PREPARATION,
        program_id,
    )
    .await?;
    Ok(jobs
        .into_iter()
        .filter(|job| job.status == JobStatus::Running)
        .flat_map(|job| {
            job.requested
                .get("lessonIds")
                .and_then(serde_json::Value::as_array)
                .map(|ids| {
                    ids.iter()
                        .filter_map(|id| id.as_str().map(str::to_owned))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
        .collect())
}

/// The saved entries of a revision with their entry revisions, by lesson.
pub(super) async fn saved_entries(
    connection: &mut SqliteConnection,
    program_id: &str,
    outline_revision: i64,
) -> Result<HashMap<String, (PlannedLesson, i64)>> {
    let rows = repository::saved_rows(connection, program_id, outline_revision).await?;
    rows.into_iter()
        .map(|(id, entry, revision)| {
            let entry: PlannedLesson = serde_json::from_str(&entry).map_err(|error| {
                AppError::InternalError(format!(
                    "The course plan entry for lesson {id} cannot be read: {error}"
                ))
            })?;
            Ok((id, (entry, revision)))
        })
        .collect()
}

/// The newest replaced entry of each lesson.
async fn newest_replaced(
    connection: &mut SqliteConnection,
    program_id: &str,
    outline_revision: i64,
) -> Result<HashMap<String, PlannedLesson>> {
    let rows = repository::newest_replaced_rows(connection, program_id, outline_revision).await?;
    rows.into_iter()
        .map(|(id, entry)| {
            let entry: PlannedLesson = serde_json::from_str(&entry).map_err(|error| {
                AppError::InternalError(format!(
                    "The replaced course plan entry for lesson {id} cannot be read: {error}"
                ))
            })?;
            Ok((id, entry))
        })
        .collect()
}

/// The course plan as the learner sees it, read in one snapshot.
pub async fn view(pool: &SqlitePool, program_id: &str) -> Result<CoursePlanViewDto> {
    let mut tx = pool.begin_read().await.map_err(db)?;
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
