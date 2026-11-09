//! The tables the Verifier audit reads and writes: `learning_lesson_audits`
//! and the lesson's quiz bank.

use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};

use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Transactions;

use super::runs::{AuditRun, BatchRecord, CourseAuditUsage, RunKey, RunState};
use crate::persistence::{db, decode, encode};

pub(super) async fn latest_run(pool: &SqlitePool, lesson_id: &str) -> Result<Option<AuditRun>> {
    sqlx::query(
        "SELECT * FROM learning_lesson_audits WHERE lesson_id=? \
         ORDER BY started_at DESC, rowid DESC LIMIT 1",
    )
    .bind(lesson_id)
    .fetch_optional(pool)
    .await
    .map_err(db)?
    .map(|row| run_from_row(&row))
    .transpose()
}

pub(super) async fn course_usage(pool: &SqlitePool, program_id: &str) -> Result<CourseAuditUsage> {
    let row = sqlx::query(
        "SELECT count(*) AS audits, coalesce(sum(a.input_tokens),0) AS input_tokens, \
         coalesce(sum(a.output_tokens),0) AS output_tokens \
         FROM learning_lesson_audits a \
         JOIN learning_lesson_reviews r ON r.lesson_id=a.lesson_id WHERE r.program_id=?",
    )
    .bind(program_id)
    .fetch_one(pool)
    .await
    .map_err(db)?;
    let count = |column: &str| u64::try_from(row.get::<i64, _>(column)).unwrap_or_default();
    Ok(CourseAuditUsage {
        audits: count("audits"),
        input_tokens: count("input_tokens"),
        output_tokens: count("output_tokens"),
    })
}

fn run_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<AuditRun> {
    let tokens = |column: &str| u64::try_from(row.get::<i64, _>(column)).unwrap_or_default();
    Ok(AuditRun {
        key: RunKey {
            lesson_id: row.get("lesson_id"),
            draft_sha256: row.get("draft_sha256"),
            verifier_model: row.get("verifier_model"),
        },
        policy: row.get("policy"),
        state: RunState::from_column(row.get("state"))?,
        batches: decode(row.get("batches_json"))?,
        input_tokens: tokens("input_tokens"),
        output_tokens: tokens("output_tokens"),
        started_at: row.get("started_at"),
        finished_at: row.get("finished_at"),
    })
}

fn signed(tokens: u64) -> i64 {
    i64::try_from(tokens).unwrap_or(i64::MAX)
}

/// Opens the run for `key`, or reopens it keeping its batches, as running,
/// and returns it.
pub(super) async fn open_run(
    pool: &SqlitePool,
    key: &RunKey,
    policy: &str,
    at: i64,
) -> Result<AuditRun> {
    let mut tx = pool.begin_write().await.map_err(db)?;
    sqlx::query(
        "INSERT INTO learning_lesson_audits \
         (lesson_id, draft_sha256, verifier_model, policy, state, batches_json, started_at) \
         VALUES (?, ?, ?, ?, 'running', '[]', ?) \
         ON CONFLICT(lesson_id, draft_sha256, verifier_model) \
         DO UPDATE SET state='running', policy=excluded.policy, finished_at=NULL",
    )
    .bind(&key.lesson_id)
    .bind(&key.draft_sha256)
    .bind(&key.verifier_model)
    .bind(policy)
    .bind(at)
    .execute(&mut *tx)
    .await
    .map_err(db)?;
    let row = sqlx::query(
        "SELECT * FROM learning_lesson_audits \
         WHERE lesson_id=? AND draft_sha256=? AND verifier_model=?",
    )
    .bind(&key.lesson_id)
    .bind(&key.draft_sha256)
    .bind(&key.verifier_model)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db)?
    .ok_or_else(|| AppError::Database("the audit row was not written".into()))?;
    let run = run_from_row(&row)?;
    tx.commit().await.map_err(db)?;
    Ok(run)
}

pub(super) async fn append_batch(
    pool: &SqlitePool,
    key: &RunKey,
    batch: &BatchRecord,
) -> Result<()> {
    let written = sqlx::query(
        "UPDATE learning_lesson_audits \
         SET batches_json=json_insert(batches_json, '$[#]', json(?)), \
             input_tokens=input_tokens+?, output_tokens=output_tokens+? \
         WHERE lesson_id=? AND draft_sha256=? AND verifier_model=?",
    )
    .bind(encode(batch)?)
    .bind(signed(batch.input_tokens))
    .bind(signed(batch.output_tokens))
    .bind(&key.lesson_id)
    .bind(&key.draft_sha256)
    .bind(&key.verifier_model)
    .execute(pool)
    .await
    .map_err(db)?;
    if written.rows_affected() == 0 {
        return Err(AppError::NotFound("The lesson's audit is gone".into()));
    }
    Ok(())
}

pub(super) async fn finish_run(
    pool: &SqlitePool,
    key: &RunKey,
    state: RunState,
    at: i64,
) -> Result<()> {
    sqlx::query(
        "UPDATE learning_lesson_audits SET state=?, finished_at=? \
         WHERE lesson_id=? AND draft_sha256=? AND verifier_model=?",
    )
    .bind(state.as_str())
    .bind(at)
    .bind(&key.lesson_id)
    .bind(&key.draft_sha256)
    .bind(&key.verifier_model)
    .execute(pool)
    .await
    .map_err(db)?;
    Ok(())
}

/// The lesson's own reviewed questions in its quiz bank, in the shape the
/// draft holds them.
pub(super) async fn lesson_questions(
    pool: &SqlitePool,
    program_id: &str,
    lesson_id: &str,
) -> Result<Vec<Value>> {
    Ok(sqlx::query("SELECT c.prompt,c.options_json,k.explanation,k.distractor_rationales_json FROM learning_assessment_candidates c JOIN learning_assessment_answer_keys k ON k.candidate_id=c.id WHERE c.program_id=? AND c.lesson_id=? AND c.origin IN ('lesson','migrated') ORDER BY c.rowid")
        .bind(program_id)
        .bind(lesson_id)
        .fetch_all(pool)
        .await
        .map_err(db)?
        .into_iter()
        .map(|row| {
            json!({
                "prompt": row.get::<String, _>("prompt"),
                "options": serde_json::from_str::<Value>(&row.get::<String, _>("options_json")).unwrap_or_default(),
                "explanation": row.get::<String, _>("explanation"),
                "rationales": serde_json::from_str::<Value>(&row.get::<String, _>("distractor_rationales_json")).unwrap_or_default(),
            })
        })
        .collect())
}
