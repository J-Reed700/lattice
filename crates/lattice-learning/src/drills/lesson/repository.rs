//! What a drill's lesson is read from: the lesson with its module and course, its blocks, and the bodies of a course's
//! prepared lessons.

use crate::persistence::db;
use lattice_core::shared::error::Result;
use sqlx::{SqliteConnection, SqlitePool};

/// A lesson with its course and module, as `LessonContext::load` reads it.
pub(super) type LessonRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    i64,
    String,
);

/// The lesson `lesson_id` of the course `program_id` with its course and module: the course's title, goal and prior
/// knowledge, the module's title, summary and outcomes, then the lesson's title, objective, minutes and preparation.
pub(super) async fn lesson_in(
    pool: &SqlitePool,
    program_id: &str,
    lesson_id: &str,
) -> Result<Option<LessonRow>> {
    sqlx::query_as(
        "SELECT p.title,p.goal,p.prior_knowledge,m.title,m.summary,m.outcomes_json,l.title,l.objective,l.estimated_minutes,l.preparation FROM learning_lessons l JOIN learning_modules m ON m.id=l.module_id JOIN learning_programs p ON p.id=l.program_id WHERE l.id=? AND l.program_id=?",
    )
    .bind(lesson_id)
    .bind(program_id)
    .fetch_optional(pool)
    .await
    .map_err(db)
}

/// The lesson's blocks as (kind, body, source ids as stored), in order.
pub(super) async fn blocks_in(
    pool: &SqlitePool,
    lesson_id: &str,
) -> Result<Vec<(String, String, String)>> {
    sqlx::query_as(
        "SELECT kind,body,source_ids_json FROM learning_blocks WHERE lesson_id=? ORDER BY ordinal",
    )
    .bind(lesson_id)
    .fetch_all(pool)
    .await
    .map_err(db)
}

/// The bodies of every block of the course's prepared lessons, other than replaced or skipped ones.
pub(crate) async fn prepared_bodies_in(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<String>> {
    sqlx::query_scalar(
        "SELECT b.body FROM learning_blocks b JOIN learning_lessons l ON l.id=b.lesson_id WHERE l.program_id=? AND l.preparation='ready' AND l.curriculum_state NOT IN ('replaced','skipped') ORDER BY l.id,b.ordinal",
    )
    .bind(program_id)
    .fetch_all(connection)
    .await
    .map_err(db)
}
