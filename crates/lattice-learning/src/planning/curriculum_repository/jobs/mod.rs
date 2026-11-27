//! Lesson generation jobs: starting, cancelling, retrying and reading them with their scope.

mod actions;
mod queries;
mod start;

use crate::dto::LearningProgramStatus;
use lattice_core::shared::error::{AppError, Result};
use sqlx::Row;
use std::collections::HashSet;

pub use actions::CancelPreflight;
pub(super) use queries::{job_scope, learning_job, preparable_in};

/// The lessons a job may prepare: in outline and unfinished.
pub(crate) const PREPARABLE_LESSON: &str =
    "preparation = 'outline' AND completed = 0 AND curriculum_state = 'outline'";

/// Lessons that do not wait for the user's review (the table is aliased `l`).
const NOT_HELD_FOR_REVIEW: &str = "NOT EXISTS (SELECT 1 FROM learning_lesson_reviews r WHERE r.lesson_id=l.id AND r.state IN ('needs_review','stopped'))";

/// Validates a program ID and returns non-empty deduplicated lesson IDs.
pub(super) fn validate_program_and_lessons(
    program_id: &str,
    lesson_ids: &[String],
) -> Result<Vec<String>> {
    uuid::Uuid::parse_str(program_id)
        .map_err(|_| AppError::InvalidInput("Invalid program ID.".into()))?;
    if lesson_ids.is_empty() {
        return Err(AppError::InvalidInput("Select at least one lesson.".into()));
    }
    let mut seen = HashSet::with_capacity(lesson_ids.len());
    let mut deduped = Vec::with_capacity(lesson_ids.len());
    for id in lesson_ids {
        uuid::Uuid::parse_str(id)
            .map_err(|_| AppError::InvalidInput("Invalid lesson ID.".into()))?;
        if seen.insert(id.clone()) {
            deduped.push(id.clone());
        }
    }
    Ok(deduped)
}

/// The outline revision of a program that is active at `expected_revision`.
pub(super) async fn check_program_open(
    tx: &mut sqlx::SqliteConnection,
    program_id: &str,
    expected_revision: i64,
) -> Result<i64> {
    let program =
        sqlx::query("SELECT revision,status,content_revision FROM learning_programs WHERE id=?")
            .bind(program_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| AppError::NotFound("Learning program not found".into()))?;
    let status = LearningProgramStatus::parse(&program.get::<String, _>("status"))?;
    if program.get::<i64, _>("revision") != expected_revision
        || status != LearningProgramStatus::Active
    {
        return Err(AppError::Conflict(
            "Program changed or is not active; reload and retry.".into(),
        ));
    }
    Ok(program.get("content_revision"))
}
