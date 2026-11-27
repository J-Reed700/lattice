//! Every read and write of `jobs`, `job_events` and `job_checkpoints`.
//!
//! Worker transitions are conditional on the job still running, so a committed
//! cancellation is never overwritten by a late result. Functions ending in
//! `_in` take a unit of work so a feature can commit a job transition together
//! with its own writes.
mod lifecycle;
mod queries;
mod recovery;
mod worker_state;

use super::{JobRecord, JobStatus};
use crate::shared::error::{AppError, Result};
use crate::shared::persistence::{Database, WriteTx};
use sqlx::{sqlite::SqliteRow, Row};

const SUSPENDED: &str = "Saved; resumes when the app opens";
const RESUMING: &str = "Resuming saved work";
const STOPPED_BY_RESTART: &str = "The app closed before this job finished.";
const STILL_RUNNING: &str = "Cancel this job before deleting it.";
const ERROR_LIMIT: usize = 2000;
/// The error code of a job that stopped the app on two launches with nothing saved between.
pub const STOPPED_THE_APP: &str = "stopped_the_app";
const STOPPED_THE_APP_MESSAGE: &str =
    "Lattice closed unexpectedly twice while this was running, so it was not resumed. Retry it to try again.";

/// All columns of `jobs` except `staged_result_json`, which can hold large
/// staged payloads that status polls, dispatches, and cancellations never read.
const SUMMARY_COLUMNS: &str = "id,kind,subject_id,operation_id,payload_hash,status,requested_json,progress_current,progress_total,progress_message,activity_json,NULL AS staged_result_json,result_ref,error_code,cause_code,error_message,retry_of_job_id,retry_count,retry_not_before,created_at,started_at,finished_at,heartbeat_at,rank,revision";

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn encode(value: &serde_json::Value) -> Result<String> {
    serde_json::to_string(value).map_err(|e| AppError::Serialization(e.to_string()))
}

fn decode(text: &str) -> Result<serde_json::Value> {
    serde_json::from_str(text).map_err(|e| AppError::Serialization(e.to_string()))
}

fn not_found() -> AppError {
    AppError::NotFound("Job not found".into())
}

fn reused_operation() -> AppError {
    AppError::InvalidInput("Operation ID was reused with different job data.".into())
}

fn count(row: &SqliteRow, column: &str) -> u32 {
    u32::try_from(row.get::<i64, _>(column)).unwrap_or(u32::MAX)
}

fn parse_status(raw: &str) -> Result<JobStatus> {
    JobStatus::parse(raw).ok_or_else(|| AppError::Database("Invalid job status".into()))
}

fn parse(row: &SqliteRow) -> Result<JobRecord> {
    let status = parse_status(row.get::<String, _>("status").as_str())?;
    let json = |column: &str| -> Result<Option<serde_json::Value>> {
        row.get::<Option<String>, _>(column)
            .as_deref()
            .map(decode)
            .transpose()
    };
    Ok(JobRecord {
        id: row.get("id"),
        kind: row.get("kind"),
        subject_id: row.get("subject_id"),
        operation_id: row.get("operation_id"),
        payload_hash: row.get("payload_hash"),
        status,
        requested: decode(&row.get::<String, _>("requested_json"))?,
        progress_current: count(row, "progress_current"),
        progress_total: count(row, "progress_total"),
        progress_message: row.get("progress_message"),
        activity: json("activity_json")?,
        staged_result: json("staged_result_json")?,
        result_ref: row.get("result_ref"),
        error_code: row.get("error_code"),
        cause_code: row.get("cause_code"),
        error_message: row.get("error_message"),
        retry_of_job_id: row.get("retry_of_job_id"),
        retry_count: count(row, "retry_count"),
        retry_not_before: row.get("retry_not_before"),
        created_at: row.get("created_at"),
        started_at: row.get("started_at"),
        finished_at: row.get("finished_at"),
        heartbeat_at: row.get("heartbeat_at"),
        rank: row.get("rank"),
        revision: row.get("revision"),
    })
}

/// Appends the job's current progress to its history under `status`.
async fn record_event(
    tx: &mut WriteTx,
    id: &str,
    status: JobStatus,
    message: &str,
    stamp: i64,
) -> Result<()> {
    sqlx::query("INSERT INTO job_events(job_id,ordinal,status,progress_current,message,created_at) SELECT j.id,(SELECT coalesce(max(ordinal),-1)+1 FROM job_events WHERE job_id=j.id),?,j.progress_current,?,? FROM jobs j WHERE j.id=?")
        .bind(status.as_str())
        .bind(message)
        .bind(stamp)
        .bind(id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

#[derive(Clone)]
pub struct JobStore {
    db: Database,
}

impl JobStore {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

/// What a recovery did: every job it settled, and those it failed.
pub(super) struct Recovered {
    pub(super) settled: Vec<String>,
    pub(super) stopped: Vec<String>,
}
