//! Every read and write of `jobs`, `job_events` and `job_checkpoints`.
//!
//! Worker transitions are conditional on the job still running, so a committed
//! cancellation is never overwritten by a late result. Functions ending in
//! `_in` take a connection so a feature can commit a job transition together
//! with its own writes.
use super::{Admitted, Backoff, Deferral, JobRecord, JobStatus, NewJob, RecoveryPolicy};
use crate::shared::error::{AppError, Result};
use sqlx::{sqlite::SqliteRow, Row, SqliteConnection, SqlitePool};

const SUSPENDED: &str = "Saved; resumes when the app opens";
const RESUMING: &str = "Resuming saved work";
const STOPPED_BY_RESTART: &str = "The app closed before this job finished.";
const STILL_RUNNING: &str = "Cancel this job before deleting it.";
const ERROR_LIMIT: usize = 2000;

fn db(error: sqlx::Error) -> AppError {
    match error {
        sqlx::Error::PoolTimedOut => AppError::ServiceNotAvailable(
            "The database is temporarily busy. Saved work is retained.".into(),
        ),
        error => AppError::Database(error.to_string()),
    }
}

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

fn parse(row: &SqliteRow) -> Result<JobRecord> {
    let status = JobStatus::parse(row.get::<String, _>("status").as_str())
        .ok_or_else(|| AppError::Database("Invalid job status".into()))?;
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
        error_message: row.get("error_message"),
        retry_of_job_id: row.get("retry_of_job_id"),
        retry_count: count(row, "retry_count"),
        retry_not_before: row.get("retry_not_before"),
        created_at: row.get("created_at"),
        started_at: row.get("started_at"),
        finished_at: row.get("finished_at"),
        heartbeat_at: row.get("heartbeat_at"),
    })
}

/// Appends the job's current progress to its history under `status`.
async fn record_event(
    connection: &mut SqliteConnection,
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
        .execute(connection)
        .await
        .map_err(db)?;
    Ok(())
}

#[derive(Clone)]
pub struct JobStore {
    pool: SqlitePool,
}

impl JobStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    async fn immediate(&self) -> Result<sqlx::Transaction<'static, sqlx::Sqlite>> {
        // Reserve the write lock before reading: upgrading a deferred WAL read
        // transaction can fail at once with SQLITE_BUSY instead of waiting.
        self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)
    }

    pub async fn get(&self, id: &str) -> Result<JobRecord> {
        let mut connection = self.pool.acquire().await.map_err(db)?;
        Self::find_in(&mut connection, id)
            .await?
            .ok_or_else(not_found)
    }

    pub async fn find_in(connection: &mut SqliteConnection, id: &str) -> Result<Option<JobRecord>> {
        sqlx::query("SELECT * FROM jobs WHERE id=?")
            .bind(id)
            .fetch_optional(connection)
            .await
            .map_err(db)?
            .as_ref()
            .map(parse)
            .transpose()
    }

    /// A subject's jobs of one kind, newest first.
    pub async fn list_for_subject(&self, kind: &str, subject_id: &str) -> Result<Vec<JobRecord>> {
        let mut connection = self.pool.acquire().await.map_err(db)?;
        Self::list_for_subject_in(&mut connection, kind, subject_id).await
    }

    pub async fn list_for_subject_in(
        connection: &mut SqliteConnection,
        kind: &str,
        subject_id: &str,
    ) -> Result<Vec<JobRecord>> {
        sqlx::query("SELECT * FROM jobs WHERE kind=? AND subject_id=? ORDER BY created_at DESC,id")
            .bind(kind)
            .bind(subject_id)
            .fetch_all(connection)
            .await
            .map_err(db)?
            .iter()
            .map(parse)
            .collect()
    }

    /// Pending and running jobs of a kind, oldest first.
    pub async fn live(&self, kind: &str) -> Result<Vec<JobRecord>> {
        sqlx::query("SELECT * FROM jobs WHERE kind=? AND status IN ('pending','running') ORDER BY created_at,id")
            .bind(kind)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .iter()
            .map(parse)
            .collect()
    }

    /// Jobs of the given kinds, newest first, leaving out attempts that a
    /// later retry superseded.
    pub async fn list_latest(
        &self,
        kinds: &[&str],
        limit: u32,
        offset: u32,
    ) -> Result<Vec<JobRecord>> {
        let kinds =
            serde_json::to_string(kinds).map_err(|e| AppError::Serialization(e.to_string()))?;
        sqlx::query("SELECT * FROM jobs WHERE kind IN (SELECT value FROM json_each(?)) AND NOT EXISTS (SELECT 1 FROM jobs retry WHERE retry.retry_of_job_id=jobs.id) ORDER BY created_at DESC,id LIMIT ? OFFSET ?")
            .bind(kinds)
            .bind(i64::from(limit))
            .bind(i64::from(offset))
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .iter()
            .map(parse)
            .collect()
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        let mut tx = self.immediate().await?;
        Self::delete_in(&mut tx, id).await?;
        tx.commit().await.map_err(db)
    }

    /// Deletes a finished job with every earlier attempt it retried, and their
    /// events and checkpoints. A job still pending or running is refused.
    pub async fn delete_in(connection: &mut SqliteConnection, id: &str) -> Result<()> {
        let statuses: Vec<String> = sqlx::query_scalar("WITH RECURSIVE chain(id) AS (SELECT id FROM jobs WHERE id=? UNION SELECT jobs.retry_of_job_id FROM jobs JOIN chain ON jobs.id=chain.id WHERE jobs.retry_of_job_id IS NOT NULL) SELECT status FROM jobs WHERE id IN (SELECT id FROM chain)")
        .bind(id)
        .fetch_all(&mut *connection)
        .await
        .map_err(db)?;
        if statuses.is_empty() {
            return Err(not_found());
        }
        if statuses
            .iter()
            .any(|status| matches!(status.as_str(), "pending" | "running"))
        {
            return Err(AppError::InvalidState(STILL_RUNNING.into()));
        }
        sqlx::query("WITH RECURSIVE chain(id) AS (SELECT id FROM jobs WHERE id=? UNION SELECT jobs.retry_of_job_id FROM jobs JOIN chain ON jobs.id=chain.id WHERE jobs.retry_of_job_id IS NOT NULL) DELETE FROM jobs WHERE id IN (SELECT id FROM chain)")
        .bind(id)
        .execute(connection)
        .await
        .map_err(db)?;
        Ok(())
    }

    /// Pending jobs of a kind whose retry time, if any, has passed. Jobs are
    /// saved before their worker starts, so this also finds work whose process
    /// exited between those steps.
    pub async fn due(&self, kind: &str) -> Result<Vec<JobRecord>> {
        sqlx::query("SELECT * FROM jobs WHERE kind=? AND status='pending' AND (retry_not_before IS NULL OR retry_not_before<=?) ORDER BY created_at,id")
            .bind(kind)
            .bind(now())
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .iter()
            .map(parse)
            .collect()
    }

    pub async fn submit(&self, job: &NewJob) -> Result<Admitted> {
        let mut tx = self.immediate().await?;
        let admitted = Self::submit_in(&mut tx, job).await?;
        tx.commit().await.map_err(db)?;
        Ok(admitted)
    }

    pub async fn submit_in(connection: &mut SqliteConnection, job: &NewJob) -> Result<Admitted> {
        if job.kind.is_empty() || job.operation_id.is_empty() || job.progress_total == 0 {
            return Err(AppError::InvalidInput(
                "A job needs a kind, an operation ID and at least one step.".into(),
            ));
        }
        if let Some(row) = sqlx::query("SELECT id,kind,payload_hash FROM jobs WHERE operation_id=?")
            .bind(&job.operation_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(db)?
        {
            if row.get::<String, _>("kind") != job.kind
                || row.get::<String, _>("payload_hash") != job.payload_hash
            {
                return Err(reused_operation());
            }
            return Ok(Admitted {
                id: row.get("id"),
                created: false,
            });
        }
        let id = uuid::Uuid::new_v4().to_string();
        let stamp = now();
        sqlx::query("INSERT INTO jobs(id,kind,subject_id,operation_id,payload_hash,status,requested_json,progress_current,progress_total,progress_message,created_at,heartbeat_at) VALUES(?,?,?,?,?,'pending',?,0,?,?,?,?)")
            .bind(&id)
            .bind(&job.kind)
            .bind(&job.subject_id)
            .bind(&job.operation_id)
            .bind(&job.payload_hash)
            .bind(encode(&job.requested)?)
            .bind(i64::from(job.progress_total))
            .bind(&job.message)
            .bind(stamp)
            .bind(stamp)
            .execute(&mut *connection)
            .await
            .map_err(db)?;
        record_event(connection, &id, JobStatus::Pending, &job.message, stamp).await?;
        Ok(Admitted { id, created: true })
    }

    pub async fn cancel(&self, id: &str) -> Result<()> {
        let mut tx = self.immediate().await?;
        Self::cancel_in(&mut tx, id).await?;
        tx.commit().await.map_err(db)
    }

    /// Pending, running and interrupted jobs can be cancelled. The status is
    /// final once committed: the worker's later transitions are conditional.
    pub async fn cancel_in(connection: &mut SqliteConnection, id: &str) -> Result<()> {
        let job = Self::find_in(&mut *connection, id)
            .await?
            .ok_or_else(not_found)?;
        if !matches!(
            job.status,
            JobStatus::Pending | JobStatus::Running | JobStatus::Interrupted
        ) {
            return Err(AppError::InvalidState(
                "This job has already finished.".into(),
            ));
        }
        let stamp = now();
        sqlx::query("UPDATE jobs SET status='cancelled',finished_at=?,result_ref=NULL,error_code=NULL,error_message=NULL,progress_message='Cancelled' WHERE id=?")
            .bind(stamp)
            .bind(id)
            .execute(&mut *connection)
            .await
            .map_err(db)?;
        record_event(connection, id, JobStatus::Cancelled, "Cancelled", stamp).await
    }

    pub async fn retry(&self, id: &str, operation_id: &str, message: &str) -> Result<Admitted> {
        let mut tx = self.immediate().await?;
        let admitted = Self::retry_in(&mut tx, id, operation_id, message).await?;
        tx.commit().await.map_err(db)?;
        Ok(admitted)
    }

    /// A new attempt of a failed, interrupted or cancelled job. It inherits the
    /// request, staged result, activity and checkpoints, so saved work is
    /// reused. One job has at most one live attempt: retrying it again returns
    /// that attempt, and only the newest attempt of a chain can be retried.
    pub async fn retry_in(
        connection: &mut SqliteConnection,
        id: &str,
        operation_id: &str,
        message: &str,
    ) -> Result<Admitted> {
        if let Some(row) = sqlx::query("SELECT id,retry_of_job_id FROM jobs WHERE operation_id=?")
            .bind(operation_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(db)?
        {
            if row.get::<Option<String>, _>("retry_of_job_id").as_deref() != Some(id) {
                return Err(reused_operation());
            }
            return Ok(Admitted {
                id: row.get("id"),
                created: false,
            });
        }
        let old = Self::find_in(&mut *connection, id)
            .await?
            .ok_or_else(not_found)?;
        if !matches!(
            old.status,
            JobStatus::Failed | JobStatus::Interrupted | JobStatus::Cancelled
        ) {
            return Err(AppError::InvalidState(
                "Only failed, interrupted, or cancelled jobs can be retried.".into(),
            ));
        }
        if let Some(child) = sqlx::query("SELECT id,status FROM jobs WHERE retry_of_job_id=? ORDER BY created_at DESC,id LIMIT 1")
            .bind(id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(db)?
        {
            if !matches!(
                child.get::<String, _>("status").as_str(),
                "pending" | "running" | "completed"
            ) {
                return Err(AppError::InvalidState(
                    "This job has a newer attempt. Retry the latest attempt to retain its saved work."
                        .into(),
                ));
            }
            return Ok(Admitted {
                id: child.get("id"),
                created: false,
            });
        }
        let retry = uuid::Uuid::new_v4().to_string();
        let stamp = now();
        sqlx::query("INSERT INTO jobs(id,kind,subject_id,operation_id,payload_hash,status,requested_json,progress_current,progress_total,progress_message,activity_json,staged_result_json,retry_of_job_id,created_at,heartbeat_at) SELECT ?,kind,subject_id,?,payload_hash,'pending',requested_json,0,progress_total,?,activity_json,staged_result_json,id,?,? FROM jobs WHERE id=?")
            .bind(&retry)
            .bind(operation_id)
            .bind(message)
            .bind(stamp)
            .bind(stamp)
            .bind(id)
            .execute(&mut *connection)
            .await
            .map_err(db)?;
        sqlx::query("INSERT INTO job_checkpoints(job_id,key,data_json,updated_at) SELECT ?,key,data_json,updated_at FROM job_checkpoints WHERE job_id=?")
            .bind(&retry)
            .bind(id)
            .execute(&mut *connection)
            .await
            .map_err(db)?;
        record_event(connection, &retry, JobStatus::Pending, message, stamp).await?;
        Ok(Admitted {
            id: retry,
            created: true,
        })
    }

    /// Moves a due pending job to running. At most one caller wins.
    pub async fn claim(&self, id: &str) -> Result<Option<JobRecord>> {
        let mut tx = self.immediate().await?;
        let stamp = now();
        let claimed = sqlx::query("UPDATE jobs SET status='running',started_at=?,finished_at=NULL,error_code=NULL,error_message=NULL,retry_not_before=NULL,progress_message='Starting',heartbeat_at=? WHERE id=? AND status='pending' AND (retry_not_before IS NULL OR retry_not_before<=?)")
            .bind(stamp)
            .bind(stamp)
            .bind(id)
            .bind(stamp)
            .execute(&mut *tx)
            .await
            .map_err(db)?
            .rows_affected()
            == 1;
        if claimed {
            record_event(&mut tx, id, JobStatus::Running, "Starting", stamp).await?;
        }
        let job = Self::find_in(&mut tx, id).await?.ok_or_else(not_found)?;
        tx.commit().await.map_err(db)?;
        Ok(claimed.then_some(job))
    }

    /// Saves progress and refreshes the heartbeat. Returns false once the job
    /// is no longer running, so the worker can stop.
    pub async fn progress(
        &self,
        id: &str,
        current: u32,
        message: &str,
        activity: Option<&serde_json::Value>,
    ) -> Result<bool> {
        let mut tx = self.immediate().await?;
        let job = Self::find_in(&mut tx, id).await?.ok_or_else(not_found)?;
        if job.status != JobStatus::Running {
            tx.commit().await.map_err(db)?;
            return Ok(false);
        }
        if activity.is_some() && current < job.progress_current {
            // A heartbeat can wait for the write lock while the worker records
            // a later step. Discard that older snapshot without stopping the
            // worker or moving its durable progress backwards.
            tx.commit().await.map_err(db)?;
            return Ok(true);
        }
        if current < job.progress_current || current > job.progress_total {
            return Err(AppError::InvalidInput(
                "Job progress must advance monotonically within its bound.".into(),
            ));
        }
        let stamp = now();
        sqlx::query("UPDATE jobs SET progress_current=?,progress_message=?,heartbeat_at=?,activity_json=coalesce(?,activity_json) WHERE id=? AND status='running'")
            .bind(i64::from(current))
            .bind(message)
            .bind(stamp)
            .bind(activity.map(encode).transpose()?)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        if current != job.progress_current || message != job.progress_message {
            record_event(&mut tx, id, JobStatus::Running, message, stamp).await?;
        }
        tx.commit().await.map_err(db)?;
        Ok(true)
    }

    pub async fn heartbeat(&self, id: &str) -> Result<bool> {
        Ok(
            sqlx::query("UPDATE jobs SET heartbeat_at=? WHERE id=? AND status='running'")
                .bind(now())
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(db)?
                .rows_affected()
                == 1,
        )
    }

    pub async fn staged(&self, id: &str) -> Result<Option<serde_json::Value>> {
        Ok(self.get(id).await?.staged_result)
    }

    /// Rewrites the staged result atomically while the job runs. Returns false
    /// without calling `update` once the job is no longer running.
    pub async fn update_staged<F>(&self, id: &str, update: F) -> Result<bool>
    where
        F: FnOnce(Option<serde_json::Value>) -> Result<serde_json::Value> + Send,
    {
        let mut tx = self.immediate().await?;
        let job = Self::find_in(&mut tx, id).await?.ok_or_else(not_found)?;
        if job.status != JobStatus::Running {
            tx.commit().await.map_err(db)?;
            return Ok(false);
        }
        let staged = update(job.staged_result)?;
        sqlx::query("UPDATE jobs SET staged_result_json=? WHERE id=? AND status='running'")
            .bind(encode(&staged)?)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(true)
    }

    pub async fn checkpoint(&self, id: &str, key: &str) -> Result<Option<serde_json::Value>> {
        let saved: Option<String> =
            sqlx::query_scalar("SELECT data_json FROM job_checkpoints WHERE job_id=? AND key=?")
                .bind(id)
                .bind(key)
                .fetch_optional(&self.pool)
                .await
                .map_err(db)?;
        saved.as_deref().map(decode).transpose()
    }

    /// Checkpoints whose key starts with `prefix`, most recently saved first.
    pub async fn checkpoints_with_prefix(
        &self,
        id: &str,
        prefix: &str,
    ) -> Result<Vec<serde_json::Value>> {
        let saved: Vec<String> = sqlx::query_scalar("SELECT data_json FROM job_checkpoints WHERE job_id=? AND substr(key,1,length(?))=? ORDER BY updated_at DESC,key")
            .bind(id)
            .bind(prefix)
            .bind(prefix)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        saved.iter().map(|value| decode(value)).collect()
    }

    pub async fn put_checkpoint(
        &self,
        id: &str,
        key: &str,
        data: &serde_json::Value,
    ) -> Result<bool> {
        let mut tx = self.immediate().await?;
        let saved = Self::put_checkpoint_in(&mut tx, id, key, data).await?;
        tx.commit().await.map_err(db)?;
        Ok(saved)
    }

    /// Saves one checkpoint while the job runs; returns false otherwise.
    pub async fn put_checkpoint_in(
        connection: &mut SqliteConnection,
        id: &str,
        key: &str,
        data: &serde_json::Value,
    ) -> Result<bool> {
        Ok(sqlx::query("INSERT INTO job_checkpoints(job_id,key,data_json,updated_at) SELECT id,?,?,? FROM jobs WHERE id=? AND status='running' ON CONFLICT(job_id,key) DO UPDATE SET data_json=excluded.data_json,updated_at=excluded.updated_at")
            .bind(key)
            .bind(encode(data)?)
            .bind(now())
            .bind(id)
            .execute(connection)
            .await
            .map_err(db)?
            .rows_affected()
            == 1)
    }

    pub async fn complete(&self, id: &str, result_ref: &str, message: &str) -> Result<bool> {
        let mut tx = self.immediate().await?;
        let completed = Self::complete_in(&mut tx, id, result_ref, message).await?;
        tx.commit().await.map_err(db)?;
        Ok(completed)
    }

    /// Completes a running job, naming what it produced.
    pub async fn complete_in(
        connection: &mut SqliteConnection,
        id: &str,
        result_ref: &str,
        message: &str,
    ) -> Result<bool> {
        let stamp = now();
        let completed = sqlx::query("UPDATE jobs SET status='completed',progress_current=progress_total,progress_message=?,result_ref=?,finished_at=?,heartbeat_at=?,error_code=NULL,error_message=NULL,retry_not_before=NULL WHERE id=? AND status='running'")
            .bind(message)
            .bind(result_ref)
            .bind(stamp)
            .bind(stamp)
            .bind(id)
            .execute(&mut *connection)
            .await
            .map_err(db)?
            .rows_affected()
            == 1;
        if completed {
            record_event(connection, id, JobStatus::Completed, message, stamp).await?;
        }
        Ok(completed)
    }

    pub async fn fail(&self, id: &str, code: &str, message: &str) -> Result<bool> {
        let mut tx = self.immediate().await?;
        let stamp = now();
        let failed = sqlx::query("UPDATE jobs SET status='failed',finished_at=?,heartbeat_at=?,error_code=?,error_message=?,progress_message='Failed',result_ref=NULL WHERE id=? AND status='running'")
            .bind(stamp)
            .bind(stamp)
            .bind(code)
            .bind(message.chars().take(ERROR_LIMIT).collect::<String>())
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db)?
            .rows_affected()
            == 1;
        if failed {
            record_event(&mut tx, id, JobStatus::Failed, "Failed", stamp).await?;
        }
        tx.commit().await.map_err(db)?;
        Ok(failed)
    }

    /// Returns a running job to pending after a temporary outage. It is due
    /// again after `backoff`, which grows with each consecutive deferral.
    pub async fn defer(&self, id: &str, deferral: &Deferral, backoff: Backoff) -> Result<bool> {
        let mut tx = self.immediate().await?;
        let job = Self::find_in(&mut tx, id).await?.ok_or_else(not_found)?;
        if job.status != JobStatus::Running {
            tx.commit().await.map_err(db)?;
            return Ok(false);
        }
        let stamp = now();
        let delay = i64::try_from(backoff.delay(job.retry_count).as_millis()).unwrap_or(i64::MAX);
        sqlx::query("UPDATE jobs SET status='pending',finished_at=NULL,error_code=?,error_message=?,progress_message=?,heartbeat_at=?,activity_json=coalesce(?,activity_json),retry_not_before=?,retry_count=retry_count+1 WHERE id=? AND status='running'")
            .bind(&deferral.code)
            .bind(deferral.message.chars().take(ERROR_LIMIT).collect::<String>())
            .bind(&deferral.progress_message)
            .bind(stamp)
            .bind(deferral.activity.as_ref().map(encode).transpose()?)
            .bind(stamp.saturating_add(delay))
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        record_event(
            &mut tx,
            id,
            JobStatus::Pending,
            &deferral.progress_message,
            stamp,
        )
        .await?;
        tx.commit().await.map_err(db)?;
        Ok(true)
    }

    /// Returns a running job to pending, recording why, so it runs again at
    /// its next delivery.
    pub(super) async fn requeue(&self, id: &str, message: &str) -> Result<bool> {
        let mut tx = self.immediate().await?;
        let requeued = Self::suspend_in(&mut tx, id, message).await?;
        tx.commit().await.map_err(db)?;
        Ok(requeued)
    }

    /// Returns a running job to pending so it resumes at the next start.
    pub async fn suspend(&self, id: &str) -> Result<bool> {
        let mut tx = self.immediate().await?;
        let suspended = Self::suspend_in(&mut tx, id, SUSPENDED).await?;
        tx.commit().await.map_err(db)?;
        Ok(suspended)
    }

    async fn suspend_in(
        connection: &mut SqliteConnection,
        id: &str,
        message: &str,
    ) -> Result<bool> {
        let stamp = now();
        let suspended = sqlx::query("UPDATE jobs SET status='pending',finished_at=NULL,error_code=NULL,error_message=NULL,progress_message=?,heartbeat_at=? WHERE id=? AND status='running'")
            .bind(message)
            .bind(stamp)
            .bind(id)
            .execute(&mut *connection)
            .await
            .map_err(db)?
            .rows_affected()
            == 1;
        if suspended {
            record_event(connection, id, JobStatus::Pending, message, stamp).await?;
        }
        Ok(suspended)
    }

    /// Ends a running job whose work cannot resume. It can be retried.
    pub async fn interrupt(&self, id: &str) -> Result<bool> {
        let mut tx = self.immediate().await?;
        let interrupted = Self::interrupt_in(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        Ok(interrupted)
    }

    async fn interrupt_in(connection: &mut SqliteConnection, id: &str) -> Result<bool> {
        let stamp = now();
        let interrupted = sqlx::query("UPDATE jobs SET status='interrupted',finished_at=?,heartbeat_at=?,result_ref=NULL,error_code='interrupted',error_message=?,progress_message='Interrupted' WHERE id=? AND status='running'")
            .bind(stamp)
            .bind(stamp)
            .bind(STOPPED_BY_RESTART)
            .bind(id)
            .execute(&mut *connection)
            .await
            .map_err(db)?
            .rows_affected()
            == 1;
        if interrupted {
            record_event(connection, id, JobStatus::Interrupted, "Interrupted", stamp).await?;
        }
        Ok(interrupted)
    }

    /// Settles the jobs of one kind that the last process left running. Call
    /// once per kind at startup, before that kind is dispatched. Never reap a
    /// live worker by age: a model call may take hours.
    pub async fn recover(&self, kind: &str, policy: RecoveryPolicy) -> Result<Vec<String>> {
        let mut tx = self.immediate().await?;
        let running: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM jobs WHERE kind=? AND status='running' ORDER BY created_at,id",
        )
        .bind(kind)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        for id in &running {
            match policy {
                RecoveryPolicy::Requeue => Self::suspend_in(&mut tx, id, RESUMING).await?,
                RecoveryPolicy::Interrupt => Self::interrupt_in(&mut tx, id).await?,
            };
        }
        tx.commit().await.map_err(db)?;
        Ok(running)
    }
}
