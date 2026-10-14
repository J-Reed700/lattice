//! Preconditions for moving a course: no generation job may be running under
//! an export or a replacement, an export needs a course worth importing, and
//! a pack only imports into the same database schema that wrote it.
use super::{db, invalid, validation::validate_program};
use crate::features::learning::{dto::LearningProgramDto, repository::LearningRepository};
use crate::shared::error::{AppError, Result};
use sqlx::SqliteConnection;

/// A pending or running lesson job for this course. Replacing the course
/// would delete the job's rows under its worker; exporting would capture a
/// half-prepared course.
pub(super) async fn active_job_exists(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<bool> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM jobs j JOIN learning_jobs lj ON lj.job_id=j.id WHERE lj.program_id=? AND j.status IN ('pending','running'))")
        .bind(program_id)
        .fetch_one(&mut *connection)
        .await
        .map_err(db)
}

pub(super) const PREPARING: &str =
    "This course is still being prepared. Export or replace it after its lessons finish generating.";

/// Refuse an export the receiving computer could not import, answering with
/// the course that can go. Evidence tables over the import's row cap are
/// refused as they are encoded, which covers replacement backups too.
pub(super) async fn require_exportable(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<LearningProgramDto> {
    if active_job_exists(connection, program_id).await? {
        return Err(invalid(PREPARING));
    }
    let program = LearningRepository::get_on(connection, program_id).await?;
    if program.modules.is_empty() {
        return Err(invalid(
            "This course has no modules yet. Finish its outline before exporting it.",
        ));
    }
    validate_program(&program)?;
    Ok(program)
}

/// The highest applied migration. Two computers on the same build agree.
pub(super) async fn schema_version(connection: &mut SqliteConnection) -> Result<i64> {
    let version: Option<i64> = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_one(&mut *connection)
        .await
        .map_err(db)?;
    version
        .ok_or_else(|| AppError::Database("This database has no recorded schema version.".into()))
}
