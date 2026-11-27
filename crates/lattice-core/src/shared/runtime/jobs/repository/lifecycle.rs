use super::super::attempt::Attempt;
use super::super::{Admitted, JobStatus, NewJob};
use super::{
    encode, not_found, now, parse_status, record_event, reused_operation, JobStore, STILL_RUNNING,
};
use crate::shared::error::{AppError, Result};
use crate::shared::persistence::WriteTx;
use sqlx::{Row, SqliteConnection};

impl JobStore {
    pub async fn delete(&self, id: &str) -> Result<()> {
        let mut tx = self.db.begin_write().await?;
        Self::delete_in(&mut tx, id).await?;
        tx.commit().await.map_err(AppError::from)
    }

    /// Deletes a finished job with every earlier attempt it retried, and their
    /// events and checkpoints. A job still pending or running is refused.
    pub async fn delete_in(tx: &mut WriteTx, id: &str) -> Result<()> {
        let statuses: Vec<String> = sqlx::query_scalar("WITH RECURSIVE chain(id) AS (SELECT id FROM jobs WHERE id=? UNION SELECT jobs.retry_of_job_id FROM jobs JOIN chain ON jobs.id=chain.id WHERE jobs.retry_of_job_id IS NOT NULL) SELECT status FROM jobs WHERE id IN (SELECT id FROM chain)")
            .bind(id)
            .fetch_all(&mut **tx)
            .await?;
        if statuses.is_empty() {
            return Err(not_found());
        }
        if statuses
            .iter()
            .any(|raw| matches!(JobStatus::parse(raw), Some(JobStatus::Pending | JobStatus::Running)))
        {
            return Err(AppError::InvalidState(STILL_RUNNING.into()));
        }
        sqlx::query("WITH RECURSIVE chain(id) AS (SELECT id FROM jobs WHERE id=? UNION SELECT jobs.retry_of_job_id FROM jobs JOIN chain ON jobs.id=chain.id WHERE jobs.retry_of_job_id IS NOT NULL) DELETE FROM jobs WHERE id IN (SELECT id FROM chain)")
            .bind(id)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    pub async fn submit(&self, job: &NewJob) -> Result<Admitted> {
        let mut tx = self.db.begin_write().await?;
        let admitted = Self::submit_in(&mut tx, job).await?;
        tx.commit().await?;
        Ok(admitted)
    }

    pub async fn submit_in(tx: &mut WriteTx, job: &NewJob) -> Result<Admitted> {
        Self::submit_with_rank_in(tx, job, None).await
    }

    /// Like [`Self::submit`], for a job of a ranked kind: its model calls are
    /// served in `rank` order, lowest first.
    pub async fn submit_ranked(&self, job: &NewJob, rank: i64) -> Result<Admitted> {
        let mut tx = self.db.begin_write().await?;
        let admitted = Self::submit_with_rank_in(&mut tx, job, Some(rank)).await?;
        tx.commit().await?;
        Ok(admitted)
    }

    pub async fn submit_with_rank_in(
        tx: &mut WriteTx,
        job: &NewJob,
        rank: Option<i64>,
    ) -> Result<Admitted> {
        if job.kind.is_empty() || job.operation_id.is_empty() || job.progress_total == 0 {
            return Err(AppError::InvalidInput(
                "A job needs a kind, an operation ID and at least one step.".into(),
            ));
        }
        if let Some(row) = sqlx::query("SELECT id,kind,payload_hash FROM jobs WHERE operation_id=?")
            .bind(&job.operation_id)
            .fetch_optional(&mut **tx)
            .await?
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
        sqlx::query("INSERT INTO jobs(id,kind,subject_id,operation_id,payload_hash,status,requested_json,progress_current,progress_total,progress_message,created_at,heartbeat_at,rank) VALUES(?,?,?,?,?,'pending',?,0,?,?,?,?,?)")
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
            .bind(rank)
            .execute(&mut **tx)
            .await?;
        record_event(tx, &id, JobStatus::Pending, &job.message, stamp).await?;
        Ok(Admitted { id, created: true })
    }

    /// Updates a job's queue rank inside the caller's transaction.
    pub async fn set_rank_in(tx: &mut WriteTx, id: &str, rank: i64) -> Result<()> {
        let updated = sqlx::query("UPDATE jobs SET rank=? WHERE id=?")
            .bind(rank)
            .bind(id)
            .execute(&mut **tx)
            .await?
            .rows_affected();
        if updated != 1 {
            return Err(not_found());
        }
        Ok(())
    }

    pub async fn cancel(&self, id: &str) -> Result<()> {
        let mut tx = self.db.urgent().begin_write().await?;
        Self::cancel_in(&mut tx, id).await?;
        tx.commit().await.map_err(AppError::from)
    }

    fn is_cancellable(status: JobStatus) -> bool {
        matches!(
            status,
            JobStatus::Pending | JobStatus::Running | JobStatus::Interrupted
        )
    }

    async fn status_in(connection: &mut SqliteConnection, id: &str) -> Result<JobStatus> {
        let raw_status: String = sqlx::query_scalar("SELECT status FROM jobs WHERE id=?")
            .bind(id)
            .fetch_optional(connection)
            .await?
            .ok_or_else(not_found)?;
        parse_status(&raw_status)
    }

    async fn mark_cancelled_in(tx: &mut WriteTx, id: &str) -> Result<()> {
        let stamp = now();
        sqlx::query("UPDATE jobs SET status='cancelled',finished_at=?,result_ref=NULL,error_code=NULL,cause_code=NULL,error_message=NULL,progress_message='Cancelled' WHERE id=?")
            .bind(stamp)
            .bind(id)
            .execute(&mut **tx)
            .await?;
        record_event(tx, id, JobStatus::Cancelled, "Cancelled", stamp).await
    }

    /// Verifies `id` exists and is pending, running, or interrupted.
    pub async fn ensure_cancellable_in(connection: &mut SqliteConnection, id: &str) -> Result<()> {
        if !Self::is_cancellable(Self::status_in(connection, id).await?) {
            return Err(AppError::InvalidState(
                "This job has already finished.".into(),
            ));
        }
        Ok(())
    }

    /// Pending, running and interrupted jobs can be cancelled. The status is
    /// final once committed: the worker's later transitions are conditional.
    pub async fn cancel_in(tx: &mut WriteTx, id: &str) -> Result<()> {
        Self::ensure_cancellable_in(tx, id).await?;
        Self::mark_cancelled_in(tx, id).await
    }

    /// Cancels `id` when still pending, running, or interrupted, returning
    /// `Ok(false)` without failing if it already reached a terminal status.
    pub async fn cancel_if_active_in(tx: &mut WriteTx, id: &str) -> Result<bool> {
        if !Self::is_cancellable(Self::status_in(tx, id).await?) {
            return Ok(false);
        }
        Self::mark_cancelled_in(tx, id).await?;
        Ok(true)
    }

    pub async fn retry(&self, id: &str, operation_id: &str, message: &str) -> Result<Admitted> {
        let mut tx = self.db.begin_write().await?;
        let admitted = Self::retry_in(&mut tx, id, operation_id, message).await?;
        tx.commit().await?;
        Ok(admitted)
    }

    /// A new attempt of a failed, interrupted or cancelled job. It inherits the
    /// request, staged result, activity and checkpoints, so saved work is
    /// reused. One job has at most one live attempt: retrying it again returns
    /// that attempt, and only the newest attempt of a chain can be retried.
    pub async fn retry_in(
        tx: &mut WriteTx,
        id: &str,
        operation_id: &str,
        message: &str,
    ) -> Result<Admitted> {
        Self::attempt_in(tx, id, operation_id, message, Attempt::Retry).await
    }

    /// A new attempt of a completed job whose result handed part of its work
    /// back to the user, for the work still left once the user has acted (a
    /// lesson that waits for review, then is resumed). Like a retry it
    /// inherits the staged result, activity and checkpoints, with at most one
    /// live attempt per job; unlike one it runs `requested`, of `total` steps,
    /// instead of the original request. Whether a completed job may continue
    /// is the caller's rule; failed or stopped jobs are retried instead.
    pub async fn continue_in(
        tx: &mut WriteTx,
        id: &str,
        operation_id: &str,
        message: &str,
        requested: &serde_json::Value,
        total: u32,
    ) -> Result<Admitted> {
        Self::attempt_in(
            tx,
            id,
            operation_id,
            message,
            Attempt::Continue { requested, total },
        )
        .await
    }

    async fn attempt_in(
        tx: &mut WriteTx,
        id: &str,
        operation_id: &str,
        message: &str,
        attempt: Attempt<'_>,
    ) -> Result<Admitted> {
        if let Some(row) = sqlx::query("SELECT id,retry_of_job_id FROM jobs WHERE operation_id=?")
            .bind(operation_id)
            .fetch_optional(&mut **tx)
            .await?
        {
            if row.get::<Option<String>, _>("retry_of_job_id").as_deref() != Some(id) {
                return Err(reused_operation());
            }
            return Ok(Admitted {
                id: row.get("id"),
                created: false,
            });
        }
        let status = Self::status_in(tx, id).await?;
        attempt.admits(status)?;
        if let Some(child) = sqlx::query("SELECT id,status FROM jobs WHERE retry_of_job_id=? ORDER BY created_at DESC,rowid DESC LIMIT 1")
            .bind(id)
            .fetch_optional(&mut **tx)
            .await?
        {
            let child_status = parse_status(child.get::<String, _>("status").as_str())?;
            if !matches!(
                child_status,
                JobStatus::Pending | JobStatus::Running | JobStatus::Completed
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
        let (requested, total) = match attempt {
            Attempt::Retry => (None, None),
            Attempt::Continue { requested, total } => (Some(encode(requested)?), Some(total)),
        };
        sqlx::query("INSERT INTO jobs(id,kind,subject_id,operation_id,payload_hash,status,requested_json,progress_current,progress_total,progress_message,activity_json,staged_result_json,retry_of_job_id,created_at,heartbeat_at,rank) SELECT ?,kind,subject_id,?,payload_hash,'pending',coalesce(?,requested_json),0,coalesce(?,progress_total),?,activity_json,staged_result_json,id,?,?,rank FROM jobs WHERE id=?")
            .bind(&retry)
            .bind(operation_id)
            .bind(requested)
            .bind(total)
            .bind(message)
            .bind(stamp)
            .bind(stamp)
            .bind(id)
            .execute(&mut **tx)
            .await?;
        sqlx::query("INSERT INTO job_checkpoints(job_id,key,data_json,updated_at) SELECT ?,key,data_json,updated_at FROM job_checkpoints WHERE job_id=?")
            .bind(&retry)
            .bind(id)
            .execute(&mut **tx)
            .await?;
        record_event(tx, &retry, JobStatus::Pending, message, stamp).await?;
        Ok(Admitted {
            id: retry,
            created: true,
        })
    }
}
