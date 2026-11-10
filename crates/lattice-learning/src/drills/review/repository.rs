//! What the review view reads: whether a course is there, its lessons' titles, and the language a code drill was last
//! taken in.

use crate::persistence::db;
use lattice_core::shared::error::Result;
use sqlx::SqliteConnection;

/// Whether the course is there.
pub(super) async fn course_exists_in(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<bool> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM learning_programs WHERE id=?)")
        .bind(program_id)
        .fetch_one(&mut *connection)
        .await
        .map_err(db)
}

/// The course's lessons as (lesson id, lesson title, module id, module title).
pub(super) async fn lesson_titles_in(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<(String, String, String, String)>> {
    sqlx::query_as(
        "SELECT l.id,l.title,m.id,m.title FROM learning_lessons l JOIN learning_modules m ON m.id=l.module_id WHERE l.program_id=?",
    )
    .bind(program_id)
    .fetch_all(&mut *connection)
    .await
    .map_err(db)
}

/// The language the latest code drill attempted in the course was taken in.
pub(super) async fn course_language_in(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<Option<String>> {
    sqlx::query_scalar(
        "SELECT d.language FROM learning_drill_attempts a JOIN learning_drills d ON d.id=a.drill_id WHERE d.program_id=? AND d.material='code' ORDER BY a.started_at DESC,a.rowid DESC LIMIT 1",
    )
    .bind(program_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(db)
}

/// The language the latest code drill attempted in any course was taken in.
pub(super) async fn latest_language_in(
    connection: &mut SqliteConnection,
) -> Result<Option<String>> {
    sqlx::query_scalar(
        "SELECT d.language FROM learning_drill_attempts a JOIN learning_drills d ON d.id=a.drill_id WHERE d.material='code' ORDER BY a.started_at DESC,a.rowid DESC LIMIT 1",
    )
    .fetch_optional(&mut *connection)
    .await
    .map_err(db)
}
