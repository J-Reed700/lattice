//! The code fit check's verdicts: one for each lesson, kept with the basis it was made on.

use lattice_core::shared::error::AppError;
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::Database;

/// A verdict to store for a lesson.
pub(super) struct Verdict<'a> {
    pub(super) program_id: &'a str,
    pub(super) lesson_id: &'a str,
    pub(super) basis: &'a str,
    pub(super) teaches: bool,
    pub(super) reason: &'a str,
    pub(super) model: &'a str,
    pub(super) checked_at: i64,
}

/// Whether the verdict stored for the lesson on `basis` says it teaches something code can show, if one is stored.
pub(super) async fn teaches_on(
    database: &Database,
    program_id: &str,
    lesson_id: &str,
    basis: &str,
) -> Result<Option<bool>> {
    let mut read = database.begin_read().await?;
    let teaches: Option<i64> = sqlx::query_scalar(
        "SELECT teaches FROM learning_drill_code_checks WHERE program_id=? AND lesson_id=? AND basis=?",
    )
    .bind(program_id)
    .bind(lesson_id)
    .bind(basis)
    .fetch_optional(&mut *read)
    .await?;
    read.commit().await?;
    Ok(teaches.map(|teaches| teaches != 0))
}

/// Stores the verdict for its lesson, replacing the earlier one; nothing, when the lesson is no longer one of the
/// course's.
pub(super) async fn record(database: &Database, verdict: &Verdict<'_>) -> Result<()> {
    let mut tx = database.begin_write().await?;
    sqlx::query(
        "INSERT INTO learning_drill_code_checks(program_id,lesson_id,basis,teaches,reason,model,checked_at) SELECT ?1,?2,?3,?4,?5,?6,?7 WHERE EXISTS(SELECT 1 FROM learning_lessons WHERE program_id=?1 AND id=?2) ON CONFLICT(program_id,lesson_id) DO UPDATE SET basis=excluded.basis,teaches=excluded.teaches,reason=excluded.reason,model=excluded.model,checked_at=excluded.checked_at",
    )
    .bind(verdict.program_id)
    .bind(verdict.lesson_id)
    .bind(verdict.basis)
    .bind(i64::from(verdict.teaches))
    .bind(verdict.reason)
    .bind(verdict.model)
    .bind(verdict.checked_at)
    .execute(&mut *tx)
    .await
    .map(drop)?;
    tx.commit().await.map_err(AppError::from)
}
