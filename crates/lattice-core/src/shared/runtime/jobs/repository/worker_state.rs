use super::super::{Backoff, Deferral, JobRecord, JobStatus};
use super::{
    count, decode, encode, not_found, now, parse_status, record_event, JobStore, ERROR_LIMIT,
};
use crate::shared::error::{AppError, Result};
use crate::shared::persistence::WriteTx;
use sqlx::Row;

impl JobStore {
    /// Moves a due pending job to running. At most one caller wins.
    pub async fn claim(&self, id: &str) -> Result<Option<JobRecord>> {
        let mut tx = self.db.begin_write().await?;
        let stamp = now();
        let claimed = sqlx::query("UPDATE jobs SET status='running',started_at=?,finished_at=NULL,error_code=NULL,cause_code=NULL,error_message=NULL,retry_not_before=NULL,progress_message='Starting',heartbeat_at=? WHERE id=? AND status='pending' AND (retry_not_before IS NULL OR retry_not_before<=?)")
            .bind(stamp)
            .bind(stamp)
            .bind(id)
            .bind(stamp)
            .execute(&mut *tx)
            .await?
            .rows_affected()
            == 1;
        if claimed {
            record_event(&mut tx, id, JobStatus::Running, "Starting", stamp).await?;
        }
        let job = Self::find_summary_in(&mut tx, id)
            .await?
            .ok_or_else(not_found)?;
        tx.commit().await?;
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
        let mut tx = self.db.begin_write().await?;
        let row = sqlx::query(
            "SELECT status,progress_current,progress_total,progress_message FROM jobs WHERE id=?",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(not_found)?;
        let status = parse_status(row.get::<String, _>("status").as_str())?;
        if status != JobStatus::Running {
            tx.commit().await?;
            return Ok(false);
        }
        let progress_current = count(&row, "progress_current");
        let progress_total = count(&row, "progress_total");
        let progress_message: String = row.get("progress_message");
        if activity.is_some() && current < progress_current {
            // A heartbeat can wait for the write lock while the worker records
            // a later step. Discard that older snapshot without stopping the
            // worker or moving its durable progress backwards.
            tx.commit().await?;
            return Ok(true);
        }
        if current < progress_current || current > progress_total {
            return Err(AppError::InvalidInput(
                "Job progress must advance monotonically within its bound.".into(),
            ));
        }
        let stamp = now();
        sqlx::query("UPDATE jobs SET progress_current=?,progress_message=?,heartbeat_at=?,activity_json=coalesce(?,activity_json),saved_work=saved_work+(?>progress_current) WHERE id=? AND status='running'")
            .bind(i64::from(current))
            .bind(message)
            .bind(stamp)
            .bind(activity.map(encode).transpose()?)
            .bind(i64::from(current))
            .bind(id)
            .execute(&mut *tx)
            .await?;
        if current != progress_current || message != progress_message {
            record_event(&mut tx, id, JobStatus::Running, message, stamp).await?;
        }
        tx.commit().await?;
        Ok(true)
    }

    pub async fn heartbeat(&self, id: &str) -> Result<bool> {
        let mut tx = self.db.begin_write().await?;
        let beat = sqlx::query("UPDATE jobs SET heartbeat_at=? WHERE id=? AND status='running'")
            .bind(now())
            .bind(id)
            .execute(&mut *tx)
            .await?
            .rows_affected()
            == 1;
        tx.commit().await?;
        Ok(beat)
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
        let mut tx = self.db.begin_write().await?;
        let job = Self::find_in(&mut tx, id).await?.ok_or_else(not_found)?;
        if job.status != JobStatus::Running {
            tx.commit().await?;
            return Ok(false);
        }
        let staged = update(job.staged_result)?;
        sqlx::query("UPDATE jobs SET staged_result_json=?1,saved_work=saved_work+(staged_result_json IS NOT ?1) WHERE id=?2 AND status='running'")
            .bind(encode(&staged)?)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn checkpoint(&self, id: &str, key: &str) -> Result<Option<serde_json::Value>> {
        let mut read = self.db.begin_read().await?;
        let saved: Option<String> =
            sqlx::query_scalar("SELECT data_json FROM job_checkpoints WHERE job_id=? AND key=?")
                .bind(id)
                .bind(key)
                .fetch_optional(&mut *read)
                .await?;
        read.commit().await?;
        saved.as_deref().map(decode).transpose()
    }

    /// Checkpoints whose key starts with `prefix`, most recently saved first.
    pub async fn checkpoints_with_prefix(
        &self,
        id: &str,
        prefix: &str,
    ) -> Result<Vec<serde_json::Value>> {
        let mut read = self.db.begin_read().await?;
        let saved: Vec<String> = sqlx::query_scalar("SELECT data_json FROM job_checkpoints WHERE job_id=? AND substr(key,1,length(?))=? ORDER BY updated_at DESC,key")
            .bind(id)
            .bind(prefix)
            .bind(prefix)
            .fetch_all(&mut *read)
            .await?;
        read.commit().await?;
        saved.iter().map(|value| decode(value)).collect()
    }

    pub async fn put_checkpoint(
        &self,
        id: &str,
        key: &str,
        data: &serde_json::Value,
    ) -> Result<bool> {
        let mut tx = self.db.begin_write().await?;
        let saved = Self::put_checkpoint_in(&mut tx, id, key, data).await?;
        tx.commit().await?;
        Ok(saved)
    }

    /// Saves one checkpoint while the job runs; returns false otherwise.
    pub async fn put_checkpoint_in(
        tx: &mut WriteTx,
        id: &str,
        key: &str,
        data: &serde_json::Value,
    ) -> Result<bool> {
        let data = encode(data)?;
        sqlx::query("UPDATE jobs SET saved_work=saved_work+1 WHERE id=?1 AND status='running' AND NOT EXISTS (SELECT 1 FROM job_checkpoints WHERE job_id=?1 AND key=?2 AND data_json=?3)")
            .bind(id)
            .bind(key)
            .bind(&data)
            .execute(&mut **tx)
            .await?;
        Ok(sqlx::query("INSERT INTO job_checkpoints(job_id,key,data_json,updated_at) SELECT id,?,?,? FROM jobs WHERE id=? AND status='running' ON CONFLICT(job_id,key) DO UPDATE SET data_json=excluded.data_json,updated_at=excluded.updated_at")
            .bind(key)
            .bind(data)
            .bind(now())
            .bind(id)
            .execute(&mut **tx)
            .await?
            .rows_affected()
            == 1)
    }

    pub async fn complete(&self, id: &str, result_ref: &str, message: &str) -> Result<bool> {
        let mut tx = self.db.begin_write().await?;
        let completed = Self::complete_in(&mut tx, id, result_ref, message).await?;
        tx.commit().await?;
        Ok(completed)
    }

    /// Completes a running job, naming what it produced.
    pub async fn complete_in(
        tx: &mut WriteTx,
        id: &str,
        result_ref: &str,
        message: &str,
    ) -> Result<bool> {
        let stamp = now();
        let completed = sqlx::query("UPDATE jobs SET status='completed',progress_current=progress_total,progress_message=?,result_ref=?,finished_at=?,heartbeat_at=?,error_code=NULL,cause_code=NULL,error_message=NULL,retry_not_before=NULL WHERE id=? AND status='running'")
            .bind(message)
            .bind(result_ref)
            .bind(stamp)
            .bind(stamp)
            .bind(id)
            .execute(&mut **tx)
            .await?
            .rows_affected()
            == 1;
        if completed {
            record_event(tx, id, JobStatus::Completed, message, stamp).await?;
        }
        Ok(completed)
    }

    pub async fn fail(&self, id: &str, code: &str, message: &str) -> Result<bool> {
        self.fail_because(id, code, message, None).await
    }

    /// [`Self::fail`], also recording the wire code of the error behind it.
    pub async fn fail_because(
        &self,
        id: &str,
        code: &str,
        message: &str,
        cause_code: Option<&str>,
    ) -> Result<bool> {
        let mut tx = self.db.begin_write().await?;
        let failed = Self::fail_in(&mut tx, id, code, message, cause_code).await?;
        tx.commit().await?;
        Ok(failed)
    }

    pub(super) async fn fail_in(
        tx: &mut WriteTx,
        id: &str,
        code: &str,
        message: &str,
        cause_code: Option<&str>,
    ) -> Result<bool> {
        let stamp = now();
        let failed = sqlx::query("UPDATE jobs SET status='failed',finished_at=?,heartbeat_at=?,error_code=?,cause_code=?,error_message=?,progress_message='Failed',result_ref=NULL WHERE id=? AND status='running'")
            .bind(stamp)
            .bind(stamp)
            .bind(code)
            .bind(cause_code)
            .bind(message.chars().take(ERROR_LIMIT).collect::<String>())
            .bind(id)
            .execute(&mut **tx)
            .await?
            .rows_affected()
            == 1;
        if failed {
            record_event(tx, id, JobStatus::Failed, "Failed", stamp).await?;
        }
        Ok(failed)
    }

    /// Returns a running job to pending after a temporary outage. It is due
    /// again after `backoff`, which grows with each consecutive deferral.
    pub async fn defer(&self, id: &str, deferral: &Deferral, backoff: Backoff) -> Result<bool> {
        self.defer_because(id, deferral, backoff, None).await
    }

    /// [`Self::defer`], also recording the wire code of the error behind it.
    pub async fn defer_because(
        &self,
        id: &str,
        deferral: &Deferral,
        backoff: Backoff,
        cause_code: Option<&str>,
    ) -> Result<bool> {
        let mut tx = self.db.begin_write().await?;
        let row = sqlx::query("SELECT status,retry_count FROM jobs WHERE id=?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(not_found)?;
        let status = parse_status(row.get::<String, _>("status").as_str())?;
        if status != JobStatus::Running {
            tx.commit().await?;
            return Ok(false);
        }
        let stamp = now();
        let delay = i64::try_from(backoff.delay(count(&row, "retry_count")).as_millis())
            .unwrap_or(i64::MAX);
        sqlx::query("UPDATE jobs SET status='pending',finished_at=NULL,error_code=?,cause_code=?,error_message=?,progress_message=?,heartbeat_at=?,activity_json=coalesce(?,activity_json),retry_not_before=?,retry_count=retry_count+1,unsaved_starts=0 WHERE id=? AND status='running'")
            .bind(&deferral.code)
            .bind(cause_code)
            .bind(deferral.message.chars().take(ERROR_LIMIT).collect::<String>())
            .bind(&deferral.progress_message)
            .bind(stamp)
            .bind(deferral.activity.as_ref().map(encode).transpose()?)
            .bind(stamp.saturating_add(delay))
            .bind(id)
            .execute(&mut *tx)
            .await?;
        record_event(
            &mut tx,
            id,
            JobStatus::Pending,
            &deferral.progress_message,
            stamp,
        )
        .await?;
        tx.commit().await?;
        Ok(true)
    }
}
