//! The passage check's verdicts: one for each passage of a lesson, kept with the basis it was made on.

use crate::persistence::db;
use lattice_core::shared::error::Result;
use sqlx::SqlitePool;

/// A verdict to store for a passage of a lesson.
pub(super) struct Verdict<'a> {
    pub(super) program_id: &'a str,
    pub(super) lesson_id: &'a str,
    pub(super) version_id: &'a str,
    pub(super) start_byte: i64,
    pub(super) end_byte: i64,
    pub(super) basis: &'a str,
    pub(super) teaches: bool,
    pub(super) reason: &'a str,
    pub(super) model: &'a str,
    pub(super) checked_at: i64,
}

/// The verdicts stored for the lesson on `basis` as (version id, start byte, end byte, teaches).
pub(super) async fn verdicts_on(
    pool: &SqlitePool,
    program_id: &str,
    lesson_id: &str,
    basis: &str,
) -> Result<Vec<(String, i64, i64, i64)>> {
    sqlx::query_as(
        "SELECT version_id,start_byte,end_byte,teaches FROM learning_drill_passage_checks WHERE program_id=? AND lesson_id=? AND basis=?",
    )
    .bind(program_id)
    .bind(lesson_id)
    .bind(basis)
    .fetch_all(pool)
    .await
    .map_err(db)
}

/// Stores the verdict for its passage, replacing the passage's earlier one; nothing, when the lesson is no longer one
/// of the course's.
pub(super) async fn record(pool: &SqlitePool, verdict: &Verdict<'_>) -> Result<()> {
    sqlx::query(
        "INSERT INTO learning_drill_passage_checks(program_id,lesson_id,version_id,start_byte,end_byte,basis,teaches,reason,model,checked_at) SELECT ?1,?2,?3,?4,?5,?6,?7,?8,?9,?10 WHERE EXISTS(SELECT 1 FROM learning_lessons WHERE program_id=?1 AND id=?2) ON CONFLICT(program_id,lesson_id,version_id,start_byte,end_byte) DO UPDATE SET basis=excluded.basis,teaches=excluded.teaches,reason=excluded.reason,model=excluded.model,checked_at=excluded.checked_at",
    )
    .bind(verdict.program_id)
    .bind(verdict.lesson_id)
    .bind(verdict.version_id)
    .bind(verdict.start_byte)
    .bind(verdict.end_byte)
    .bind(verdict.basis)
    .bind(i64::from(verdict.teaches))
    .bind(verdict.reason)
    .bind(verdict.model)
    .bind(verdict.checked_at)
    .execute(pool)
    .await
    .map(drop)
    .map_err(db)
}
