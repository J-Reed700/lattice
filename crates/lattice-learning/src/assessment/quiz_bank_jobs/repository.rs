//! The quiz jobs' statements: queueing a job with its learning record, the pending written answers a grading job
//! waits for, and a lesson's heading.

use super::{QUIZ_BANK, QUIZ_GRADING};
use crate::curriculum_repository::lesson_rank_in;
use crate::persistence::db;
use lattice_core::shared::error::Result;
use lattice_core::shared::runtime::jobs::{JobStore, NewJob};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::SqliteConnection;

fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// A UUID derived from text, so a repeated request finds the same job.
fn derived_uuid(seed: &str) -> String {
    let hash = Sha256::digest(seed.as_bytes());
    let mut bytes = [0u8; 16];
    for (slot, byte) in bytes.iter_mut().zip(hash.iter()) {
        *slot = *byte;
    }
    uuid::Builder::from_random_bytes(bytes)
        .into_uuid()
        .to_string()
}

pub(super) async fn queue_in(
    connection: &mut SqliteConnection,
    kind: &str,
    program_id: &str,
    operation_id: String,
    requested: serde_json::Value,
    total: u32,
    message: &str,
    rank: Option<i64>,
) -> Result<String> {
    let admitted = JobStore::submit_with_rank_in(
        connection,
        &NewJob {
            kind: kind.into(),
            subject_id: Some(program_id.into()),
            operation_id,
            payload_hash: digest(&requested.to_string()),
            requested,
            progress_total: total.max(1),
            message: message.into(),
        },
        rank,
    )
    .await?;
    if admitted.created {
        let revision = program_revision_in(connection, program_id).await?;
        insert_learning_job_in(connection, &admitted.id, program_id, revision.max(0)).await?;
    }
    Ok(admitted.id)
}

/// Queue the grading of a submitted quiz's pending items, or (with a
/// contest) a blind regrade. The caller tells the runtime once it commits.
pub(crate) async fn queue_grading_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    form_id: &str,
    contest_id: Option<&str>,
    seed: &str,
) -> Result<String> {
    let pending = pending_item_count_in(connection, form_id).await?;
    queue_in(
        connection,
        QUIZ_GRADING,
        program_id,
        derived_uuid(&format!("{QUIZ_GRADING}:{seed}")),
        json!({"programId": program_id, "formId": form_id, "contestId": contest_id}),
        if contest_id.is_some() {
            1
        } else {
            pending.max(1) as u32
        },
        if contest_id.is_some() {
            "Waiting to regrade"
        } else {
            "Waiting to grade written answers"
        },
        None,
    )
    .await
}

/// Queue writing a published lesson's quiz bank, in the transaction that
/// publishes the lesson. The caller tells the runtime once it commits.
pub(crate) async fn queue_lesson_bank_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    lesson_id: &str,
) -> Result<String> {
    let rank = lesson_rank_in(connection, lesson_id).await?;
    queue_in(
        connection,
        QUIZ_BANK,
        program_id,
        uuid::Uuid::new_v4().to_string(),
        json!({
            "programId": program_id,
            "scope": {"kind": "lesson", "id": lesson_id},
            "topUp": false,
            "lessonStage": true,
        }),
        1,
        "Waiting to write quiz questions",
        rank,
    )
    .await
}

/// The course's revision number.
pub(super) async fn program_revision_in(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<i64> {
    sqlx::query_scalar("SELECT revision FROM learning_programs WHERE id=?")
        .bind(program_id)
        .fetch_one(&mut *connection)
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
        .fetch_one(&mut *connection)
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
        .await
        .map_err(db)
}
