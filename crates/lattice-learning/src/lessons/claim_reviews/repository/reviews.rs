//! Which lessons of a course are under review, and which sources are gone.

use std::collections::HashSet;

use lattice_core::domain::learning::claim_review::LessonReviewState;
use lattice_core::shared::error::{AppError, Result};
use sqlx::{Row, SqliteConnection, SqlitePool};

use crate::persistence::db;

/// The course's lessons held for review: waiting for the user, or being
/// verified.
pub(in crate::lessons::claim_reviews) async fn held_lessons(
    pool: &SqlitePool,
    program_id: &str,
) -> Result<Vec<String>> {
    sqlx::query_scalar(
        "SELECT lesson_id FROM learning_lesson_reviews WHERE program_id=? AND state IN (?,?) ORDER BY opened_at,lesson_id",
    )
    .bind(program_id)
    .bind(LessonReviewState::NeedsReview.as_str())
    .bind(LessonReviewState::Verifying.as_str())
    .fetch_all(pool)
    .await
    .map_err(db)
}

/// Every lesson of the program that has a review.
pub(in crate::lessons::claim_reviews) async fn reviewed_lessons(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<String>> {
    sqlx::query_scalar::<_, String>(
        "SELECT lesson_id FROM learning_lesson_reviews WHERE program_id=? ORDER BY lesson_id",
    )
    .bind(program_id)
    .fetch_all(&mut *connection)
    .await
    .map_err(db)
}

/// The ids of every version of the source.
pub(in crate::lessons::claim_reviews) async fn source_versions(
    connection: &mut SqliteConnection,
    program_id: &str,
    source_id: &str,
) -> Result<HashSet<String>> {
    Ok(sqlx::query_scalar::<_, String>(
        "SELECT id FROM learning_source_versions WHERE program_id=? AND source_id=?",
    )
    .bind(program_id)
    .bind(source_id)
    .fetch_all(&mut *connection)
    .await
    .map_err(db)?
    .into_iter()
    .collect())
}

/// The versions of the program's deleted sources.
pub(in crate::lessons::claim_reviews) async fn deleted_versions(
    pool: &SqlitePool,
    program_id: &str,
) -> Result<HashSet<String>> {
    Ok(sqlx::query_scalar::<_, String>(
        "SELECT v.id FROM learning_source_versions v \
         JOIN learning_source_library s ON s.program_id=v.program_id AND s.id=v.source_id \
         WHERE v.program_id=? AND s.deleted_at IS NOT NULL",
    )
    .bind(program_id)
    .fetch_all(pool)
    .await
    .map_err(db)?
    .into_iter()
    .collect())
}

pub(in crate::lessons::claim_reviews) async fn mark_needs_review(
    connection: &mut SqliteConnection,
    lesson_id: &str,
    at: i64,
) -> Result<()> {
    sqlx::query("UPDATE learning_lesson_reviews SET state=?,updated_at=? WHERE lesson_id=?")
        .bind(LessonReviewState::NeedsReview.as_str())
        .bind(at)
        .bind(lesson_id)
        .execute(&mut *connection)
        .await
        .map_err(db)?;
    Ok(())
}

/// Lessons with a claim the gate weighed: the only ones a trust change can
/// touch.
pub(in crate::lessons::claim_reviews) async fn weighed_lessons(
    pool: &SqlitePool,
) -> Result<Vec<String>> {
    let rows = sqlx::query(
        "SELECT DISTINCT c.lesson_id FROM learning_lesson_claim_events e \
         JOIN learning_lesson_claims c ON c.claim_id = e.claim_id \
         WHERE e.kind = 'standing_assessed' ORDER BY c.lesson_id",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| AppError::Database(error.to_string()))?;
    rows.iter()
        .map(|row| {
            row.try_get::<String, _>("lesson_id")
                .map_err(|error| AppError::Database(error.to_string()))
        })
        .collect()
}
