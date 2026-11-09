//! The verification report and the review's state at publication.

use futures::FutureExt;
use lattice_core::shared::error::{AppError, Result};
use sqlx::SqliteConnection;

fn failed(error: sqlx::Error) -> AppError {
    AppError::Database(error.to_string())
}

/// The text of the source's active version `version_id`, when the source is
/// not deleted.
pub(super) async fn active_version_text(
    connection: &mut SqliteConnection,
    program_id: &str,
    version_id: &str,
) -> Result<Option<String>> {
    sqlx::query_scalar("SELECT v.full_text FROM learning_source_versions v JOIN learning_source_library s ON s.active_version_id=v.id AND s.program_id=v.program_id WHERE v.id=? AND v.program_id=? AND s.deleted_at IS NULL")
        .bind(version_id)
        .bind(program_id)
        .fetch_optional(&mut *connection).boxed()
        .await
        .map_err(failed)
}

pub(super) struct StoredReport<'a> {
    pub lesson_id: &'a str,
    pub program_id: &'a str,
    pub policy: &'a str,
    pub content_sha256: &'a str,
    pub report_json: &'a str,
    pub checked_at: i64,
}

pub(super) async fn insert_report(
    connection: &mut SqliteConnection,
    report: &StoredReport<'_>,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_lesson_verifications(lesson_id,program_id,policy,content_sha256,report_json,checked_at) VALUES(?,?,?,?,?,?)")
        .bind(report.lesson_id)
        .bind(report.program_id)
        .bind(report.policy)
        .bind(report.content_sha256)
        .bind(report.report_json)
        .bind(report.checked_at)
        .execute(&mut *connection)
        .await
        .map_err(failed)?;
    Ok(())
}

/// Ends the lesson's claim review with its publication.
pub(super) async fn mark_review_ready(
    connection: &mut SqliteConnection,
    lesson_id: &str,
    at: i64,
) -> Result<()> {
    sqlx::query("UPDATE learning_lesson_reviews SET state='ready',revision=revision+1,updated_at=? WHERE lesson_id=?")
        .bind(at)
        .bind(lesson_id)
        .execute(&mut *connection)
        .await
        .map_err(failed)?;
    Ok(())
}
