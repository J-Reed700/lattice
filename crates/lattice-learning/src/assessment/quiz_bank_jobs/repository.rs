//! The quiz jobs' statements: the learning record of a queued job, the pending written answers a grading job
//! waits for, and a lesson's heading.

use crate::persistence::db;
use futures::FutureExt;
use lattice_core::shared::error::Result;
use sqlx::SqliteConnection;

/// The course's revision number.
pub(super) async fn program_revision_in(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<i64> {
    sqlx::query_scalar("SELECT revision FROM learning_programs WHERE id=?")
        .bind(program_id)
        .fetch_one(&mut *connection)
        .boxed()
        .await
        .map_err(db)
}

/// Records that a job belongs to the course at `base_revision`.
pub(super) async fn insert_learning_job_in(
    connection: &mut SqliteConnection,
    job_id: &str,
    program_id: &str,
    base_revision: i64,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_jobs(job_id,program_id,base_revision_number,content_revision) VALUES(?,?,?,NULL)")
        .bind(job_id)
        .bind(program_id)
        .bind(base_revision)
        .execute(&mut *connection)
        .await
        .map(drop)
        .map_err(db)
}

/// How many items of the form wait for their grade.
pub(super) async fn pending_item_count_in(
    connection: &mut SqliteConnection,
    form_id: &str,
) -> Result<i64> {
    sqlx::query_scalar("SELECT count(*) FROM learning_assessment_item_results WHERE form_id=? AND grade_status='pending'")
        .bind(form_id)
        .fetch_one(&mut *connection).boxed()
        .await
        .map_err(db)
}

/// The lesson's title and objective, when it is one of the course's.
pub(super) async fn lesson_title_objective_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    lesson_id: &str,
) -> Result<Option<(String, String)>> {
    sqlx::query_as("SELECT title,objective FROM learning_lessons WHERE id=? AND program_id=?")
        .bind(lesson_id)
        .bind(program_id)
        .fetch_optional(&mut *connection)
        .boxed()
        .await
        .map_err(db)
}
