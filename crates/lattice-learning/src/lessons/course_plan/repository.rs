//! The course plan's tables: `learning_course_plan_lessons`,
//! `learning_course_plan_entry_history`, and the course plan's rows of
//! `learning_jobs`.

use crate::persistence::db;
use lattice_core::shared::error::{AppError, Result};
use sqlx::SqliteConnection;

/// The outline revision a program's plan is keyed on.
pub(super) async fn content_revision(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<i64> {
    sqlx::query_scalar("SELECT content_revision FROM learning_programs WHERE id=?")
        .bind(program_id)
        .fetch_optional(connection)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Learning program not found".into()))
}

/// A program's status and revision.
pub(super) async fn program_state(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<Option<(String, i64)>> {
    sqlx::query_as("SELECT status,revision FROM learning_programs WHERE id=?")
        .bind(program_id)
        .fetch_optional(connection)
        .await
        .map_err(db)
}

/// Each lesson's id, preparation and completed flag, in course order.
pub(super) async fn outline_rows(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<(String, String, i64)>> {
    sqlx::query_as(
        "SELECT l.id,l.preparation,l.completed FROM learning_lessons l JOIN learning_modules m ON m.id=l.module_id WHERE l.program_id=? ORDER BY m.ordinal,l.ordinal",
    )
    .bind(program_id)
    .fetch_all(connection)
    .await
    .map_err(db)
}

/// The saved entries of a revision: lesson id, entry JSON, entry revision.
pub(super) async fn saved_rows(
    connection: &mut SqliteConnection,
    program_id: &str,
    outline_revision: i64,
) -> Result<Vec<(String, String, i64)>> {
    sqlx::query_as(
        "SELECT lesson_id,entry_json,entry_revision FROM learning_course_plan_lessons WHERE program_id=? AND outline_revision=?",
    )
    .bind(program_id)
    .bind(outline_revision)
    .fetch_all(connection)
    .await
    .map_err(db)
}

/// The entries of a revision as lesson id and entry JSON.
pub(super) async fn entry_rows(
    connection: &mut SqliteConnection,
    program_id: &str,
    outline_revision: i64,
) -> Result<Vec<(String, String)>> {
    sqlx::query_as(
        "SELECT lesson_id,entry_json FROM learning_course_plan_lessons WHERE program_id=? AND outline_revision=?",
    )
    .bind(program_id)
    .bind(outline_revision)
    .fetch_all(connection)
    .await
    .map_err(db)
}

/// The newest replaced entry of each lesson: lesson id and entry JSON.
pub(super) async fn newest_replaced_rows(
    connection: &mut SqliteConnection,
    program_id: &str,
    outline_revision: i64,
) -> Result<Vec<(String, String)>> {
    sqlx::query_as(
        "SELECT h.lesson_id,h.entry_json FROM learning_course_plan_entry_history h WHERE h.program_id=? AND h.outline_revision=? AND h.entry_revision=(SELECT max(entry_revision) FROM learning_course_plan_entry_history WHERE program_id=h.program_id AND outline_revision=h.outline_revision AND lesson_id=h.lesson_id)",
    )
    .bind(program_id)
    .bind(outline_revision)
    .fetch_all(connection)
    .await
    .map_err(db)
}

/// Saves a lesson's entry unless it already has one.
pub(super) async fn insert_entry(
    connection: &mut SqliteConnection,
    program_id: &str,
    outline_revision: i64,
    lesson_id: &str,
    entry_json: &str,
    created_at: i64,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_course_plan_lessons(program_id,outline_revision,lesson_id,entry_json,created_at) VALUES(?,?,?,?,?) ON CONFLICT(program_id,outline_revision,lesson_id) DO NOTHING")
        .bind(program_id)
        .bind(outline_revision)
        .bind(lesson_id)
        .bind(entry_json)
        .bind(created_at)
        .execute(connection)
        .await
        .map_err(db)?;
    Ok(())
}

/// Replaces a lesson's entry, keeping the old one in its history.
pub(super) async fn replace_entry(
    connection: &mut SqliteConnection,
    program_id: &str,
    outline_revision: i64,
    lesson_id: &str,
    entry_json: &str,
    replaced_at: i64,
) -> Result<()> {
    let (old, revision): (String, i64) = sqlx::query_as(
        "SELECT entry_json,entry_revision FROM learning_course_plan_lessons WHERE program_id=? AND outline_revision=? AND lesson_id=?",
    )
    .bind(program_id)
    .bind(outline_revision)
    .bind(lesson_id)
    .fetch_one(&mut *connection)
    .await
    .map_err(db)?;
    sqlx::query("INSERT INTO learning_course_plan_entry_history(program_id,outline_revision,lesson_id,entry_revision,entry_json,replaced_at) VALUES(?,?,?,?,?,?)")
        .bind(program_id)
        .bind(outline_revision)
        .bind(lesson_id)
        .bind(revision)
        .bind(old)
        .bind(replaced_at)
        .execute(&mut *connection)
        .await
        .map_err(db)?;
    sqlx::query("UPDATE learning_course_plan_lessons SET entry_json=?,entry_revision=entry_revision+1 WHERE program_id=? AND outline_revision=? AND lesson_id=?")
        .bind(entry_json)
        .bind(program_id)
        .bind(outline_revision)
        .bind(lesson_id)
        .execute(&mut *connection)
        .await
        .map_err(db)?;
    Ok(())
}

/// Puts back the entry the newest change replaced and drops it from the
/// history. False when there is nothing to put back.
pub(super) async fn restore_entry(
    connection: &mut SqliteConnection,
    program_id: &str,
    outline_revision: i64,
    lesson_id: &str,
) -> Result<bool> {
    let newest: Option<(i64, String)> = sqlx::query_as(
        "SELECT entry_revision,entry_json FROM learning_course_plan_entry_history WHERE program_id=? AND outline_revision=? AND lesson_id=? ORDER BY entry_revision DESC LIMIT 1",
    )
    .bind(program_id)
    .bind(outline_revision)
    .bind(lesson_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(db)?;
    let Some((replaced, entry)) = newest else {
        return Ok(false);
    };
    sqlx::query("UPDATE learning_course_plan_lessons SET entry_json=?,entry_revision=entry_revision+1 WHERE program_id=? AND outline_revision=? AND lesson_id=?")
        .bind(entry)
        .bind(program_id)
        .bind(outline_revision)
        .bind(lesson_id)
        .execute(&mut *connection)
        .await
        .map_err(db)?;
    sqlx::query("DELETE FROM learning_course_plan_entry_history WHERE program_id=? AND outline_revision=? AND lesson_id=? AND entry_revision=?")
        .bind(program_id)
        .bind(outline_revision)
        .bind(lesson_id)
        .bind(replaced)
        .execute(&mut *connection)
        .await
        .map_err(db)?;
    Ok(true)
}

/// Every job of a program with the outline revision it was made for.
pub(super) async fn job_revisions(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<(String, Option<i64>)>> {
    sqlx::query_as("SELECT job_id,content_revision FROM learning_jobs WHERE program_id=?")
        .bind(program_id)
        .fetch_all(connection)
        .await
        .map_err(db)
}

/// Records the program, base revision and outline revision of a new job.
pub(super) async fn insert_job_row(
    connection: &mut SqliteConnection,
    job_id: &str,
    program_id: &str,
    base_revision: i64,
    outline_revision: i64,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_jobs(job_id,program_id,base_revision_number,content_revision) VALUES(?,?,?,?)")
        .bind(job_id)
        .bind(program_id)
        .bind(base_revision)
        .bind(outline_revision)
        .execute(connection)
        .await
        .map_err(db)?;
    Ok(())
}

/// Gives a new attempt the program and revisions of the job it retries.
pub(super) async fn copy_job_row(
    connection: &mut SqliteConnection,
    new_job_id: &str,
    old_job_id: &str,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_jobs(job_id,program_id,base_revision_number,content_revision) SELECT ?,program_id,base_revision_number,content_revision FROM learning_jobs WHERE job_id=?")
        .bind(new_job_id)
        .bind(old_job_id)
        .execute(connection)
        .await
        .map_err(db)?;
    Ok(())
}
