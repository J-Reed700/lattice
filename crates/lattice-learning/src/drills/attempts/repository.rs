//! The attempts' rows: opening, handing in, asking for a grade again, recording the grade or why there is none, and
//! reading an attempt back.

use super::{operation_id, Attempt, AttemptMode, AttemptStatus, Graded, Submission};
use crate::drills::jobs::{DRILL_GRADING, GRADING_STEPS};
use crate::drills::repository::{self as drill_repository, StoredDrill};
use crate::persistence::{db, decode, encode, hash, now};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::WriteTx;
use lattice_core::shared::runtime::jobs::{JobStore, NewJob};
use serde::de::DeserializeOwned;
use serde_json::json;
use sqlx::{sqlite::SqliteRow, Row, SqliteConnection};

/// Opens an attempt `id` on a drill.
pub(super) async fn insert_in(
    tx: &mut WriteTx,
    id: &str,
    drill_id: &str,
    mode: AttemptMode,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO learning_drill_attempts(id,drill_id,mode,started_at,status) VALUES(?,?,?,?,'open')",
    )
    .bind(id)
    .bind(drill_id)
    .bind(mode.as_str())
    .bind(now())
    .execute(&mut **tx)
    .await
    .map(drop)
    .map_err(db)
}

/// Opens an attempt on a drill the caller has taken from the bank.
pub(crate) async fn open_in(
    tx: &mut WriteTx,
    drill_id: &str,
    mode: AttemptMode,
) -> Result<Attempt> {
    let id = uuid::Uuid::new_v4().to_string();
    insert_in(tx, &id, drill_id, mode).await?;
    find_in(tx, &id)
        .await?
        .ok_or_else(|| AppError::InternalError("The attempt just opened is not there.".into()))
}

/// Stores what the learner handed in on an open attempt.
pub(super) async fn submit_in(tx: &mut WriteTx, id: &str, submission: &Submission) -> Result<()> {
    sqlx::query("UPDATE learning_drill_attempts SET status='submitted',submitted_at=?,time_spent_ms=?,findings_json=?,fix_files_json=? WHERE id=? AND status='open'")
        .bind(now())
        .bind(submission.time_spent_ms)
        .bind(encode(&submission.findings)?)
        .bind(submission.fix_files.as_ref().map(encode).transpose()?)
        .bind(id)
        .execute(&mut **tx)
        .await
        .map(drop)
        .map_err(db)
}

/// Sets an attempt that could not be graded to wait for a new grade, clearing the error of the last try.
pub(super) async fn regrade_in(tx: &mut WriteTx, id: &str) -> Result<()> {
    sqlx::query("UPDATE learning_drill_attempts SET status='submitted',grading_error=NULL,grader_model=NULL,regrades=regrades+1 WHERE id=? AND status='couldnt_grade'")
        .bind(id)
        .execute(&mut **tx)
        .await
        .map(drop)
        .map_err(db)
}

/// Writes the grade and marks the attempt graded. False when the attempt was not waiting for a grade.
pub(crate) async fn record_graded_in(
    tx: &mut WriteTx,
    id: &str,
    graded: &Graded<'_>,
) -> Result<bool> {
    Ok(sqlx::query("UPDATE learning_drill_attempts SET status='graded',assessment_json=?,score_json=?,ratings_json=?,fix_json=?,grader_model=?,graded_at=?,grading_error=NULL WHERE id=? AND status='submitted'")
        .bind(encode(graded.assessment)?)
        .bind(encode(graded.score)?)
        .bind(encode(graded.ratings)?)
        .bind(graded.fix.map(encode).transpose()?)
        .bind(graded.model)
        .bind(now())
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(db)?
        .rows_affected()
        == 1)
}

/// Marks the attempt as not graded, keeping why and the model that was tried, when one was reached. False when the
/// attempt was not waiting for a grade.
pub(crate) async fn record_ungraded_in(
    tx: &mut WriteTx,
    id: &str,
    model: Option<&str>,
    error: &str,
) -> Result<bool> {
    Ok(sqlx::query("UPDATE learning_drill_attempts SET status='couldnt_grade',grading_error=?,grader_model=? WHERE id=? AND status='submitted'")
        .bind(error)
        .bind(model)
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(db)?
        .rows_affected()
        == 1)
}

/// The ids of the latest attempts on drills of the course, newest first, at most `limit`.
pub(super) async fn recent_ids_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    limit: u32,
) -> Result<Vec<String>> {
    sqlx::query_scalar(
        "SELECT a.id FROM learning_drill_attempts a JOIN learning_drills d ON d.id=a.drill_id WHERE d.program_id=? ORDER BY a.started_at DESC,a.rowid DESC LIMIT ?",
    )
    .bind(program_id)
    .bind(i64::from(limit))
    .fetch_all(&mut *connection)
    .await
    .map_err(db)
}

/// The attempt with this id.
pub(super) async fn find_in(
    connection: &mut SqliteConnection,
    id: &str,
) -> Result<Option<Attempt>> {
    sqlx::query("SELECT id,drill_id,mode,started_at,submitted_at,time_spent_ms,findings_json,fix_files_json,status,assessment_json,score_json,ratings_json,fix_json,grader_model,graded_at,grading_error,regrades FROM learning_drill_attempts WHERE id=?")
        .bind(id)
        .fetch_optional(connection)
        .await
        .map_err(db)?
        .as_ref()
        .map(read)
        .transpose()
}

fn read(row: &SqliteRow) -> Result<Attempt> {
    let regrades: i64 = row.try_get("regrades").map_err(db)?;
    Ok(Attempt {
        id: row.try_get("id").map_err(db)?,
        drill_id: row.try_get("drill_id").map_err(db)?,
        mode: AttemptMode::parse(&row.try_get::<String, _>("mode").map_err(db)?)?,
        started_at: row.try_get("started_at").map_err(db)?,
        submitted_at: row.try_get("submitted_at").map_err(db)?,
        time_spent_ms: row.try_get("time_spent_ms").map_err(db)?,
        findings: stored(row, "findings_json")?,
        fix_files: stored(row, "fix_files_json")?,
        status: AttemptStatus::parse(&row.try_get::<String, _>("status").map_err(db)?)?,
        assessment: stored(row, "assessment_json")?,
        score: stored(row, "score_json")?,
        ratings: stored(row, "ratings_json")?,
        fix: stored(row, "fix_json")?,
        grader_model: row.try_get("grader_model").map_err(db)?,
        graded_at: row.try_get("graded_at").map_err(db)?,
        error: row.try_get("grading_error").map_err(db)?,
        regrades: u32::try_from(regrades).map_err(|_| {
            AppError::Database("An attempt has an impossible regrade count.".into())
        })?,
    })
}

/// A JSON column that may be null.
fn stored<T: DeserializeOwned>(row: &SqliteRow, column: &str) -> Result<Option<T>> {
    row.try_get::<Option<String>, _>(column)
        .map_err(db)?
        .as_deref()
        .map(decode::<T>)
        .transpose()
}

/// The latest attempts on drills of the course, newest first, at most `limit`, each with the drill it is on.
pub(crate) async fn recent_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    limit: u32,
) -> Result<Vec<(Attempt, StoredDrill)>> {
    let ids = recent_ids_in(connection, program_id, limit).await?;
    let mut recent = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(attempt) = find_in(connection, &id).await? else {
            continue;
        };
        let Some(drill) = drill_repository::find_in(connection, &attempt.drill_id).await? else {
            continue;
        };
        recent.push((attempt, drill));
    }
    Ok(recent)
}

/// Queues the job that grades `attempt` as it stands. The operation ID derives from the attempt and the times it was
/// asked for again, so queueing it twice finds the same job.
pub(super) async fn queue_grading_in(tx: &mut WriteTx, attempt: &Attempt) -> Result<String> {
    let requested = json!({"attemptId": attempt.id, "regrades": attempt.regrades});
    let payload_hash = hash(&requested)?;
    let admitted = JobStore::submit_in(
        tx,
        &NewJob {
            kind: DRILL_GRADING.into(),
            subject_id: Some(attempt.id.clone()),
            operation_id: operation_id(&attempt.id, attempt.regrades),
            payload_hash,
            requested,
            progress_total: GRADING_STEPS,
            message: "Waiting to grade your review".into(),
        },
    )
    .await?;
    Ok(admitted.id)
}
