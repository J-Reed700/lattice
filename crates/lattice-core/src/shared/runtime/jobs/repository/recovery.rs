use super::super::{JobStatus, RecoveryPolicy};
use super::{
    now, record_event, JobStore, Recovered, RESUMING, STOPPED_BY_RESTART, STOPPED_THE_APP,
    STOPPED_THE_APP_MESSAGE, SUSPENDED,
};
use crate::shared::error::{AppError, Result};
use crate::shared::persistence::WriteTx;

impl JobStore {
    /// Returns a running job to pending, recording why, so it runs again at
    /// its next delivery.
    pub(in super::super) async fn requeue(&self, id: &str, message: &str) -> Result<bool> {
        let mut tx = self.db.begin_write().await?;
        let requeued = Self::suspend_in(&mut tx, id, message).await?;
        tx.commit().await?;
        Ok(requeued)
    }

    /// Returns a running job to pending so it resumes at the next start.
    pub async fn suspend(&self, id: &str) -> Result<bool> {
        let mut tx = self.db.begin_write().await?;
        let suspended = Self::suspend_in(&mut tx, id, SUSPENDED).await?;
        tx.commit().await?;
        Ok(suspended)
    }

    async fn suspend_in(tx: &mut WriteTx, id: &str, message: &str) -> Result<bool> {
        let stamp = now();
        let suspended = sqlx::query("UPDATE jobs SET status='pending',finished_at=NULL,error_code=NULL,cause_code=NULL,error_message=NULL,progress_message=?,heartbeat_at=?,unsaved_starts=0 WHERE id=? AND status='running'")
            .bind(message)
            .bind(stamp)
            .bind(id)
            .execute(&mut **tx)
            .await?
            .rows_affected()
            == 1;
        if suspended {
            record_event(tx, id, JobStatus::Pending, message, stamp).await?;
        }
        Ok(suspended)
    }

    /// Ends a running job whose work cannot resume. It can be retried.
    pub async fn interrupt(&self, id: &str) -> Result<bool> {
        let mut tx = self.db.begin_write().await?;
        let interrupted = Self::interrupt_in(&mut tx, id).await?;
        tx.commit().await?;
        Ok(interrupted)
    }

    async fn interrupt_in(tx: &mut WriteTx, id: &str) -> Result<bool> {
        let stamp = now();
        let interrupted = sqlx::query("UPDATE jobs SET status='interrupted',finished_at=?,heartbeat_at=?,result_ref=NULL,error_code='interrupted',cause_code=NULL,error_message=?,progress_message='Interrupted' WHERE id=? AND status='running'")
            .bind(stamp)
            .bind(stamp)
            .bind(STOPPED_BY_RESTART)
            .bind(id)
            .execute(&mut **tx)
            .await?
            .rows_affected()
            == 1;
        if interrupted {
            record_event(tx, id, JobStatus::Interrupted, "Interrupted", stamp).await?;
        }
        Ok(interrupted)
    }

    /// Settles the jobs of one kind that the last process left running. Call
    /// once per kind at startup, before that kind is dispatched. Never reap a
    /// live worker by age: a model call may take hours.
    pub async fn recover(&self, kind: &str, policy: RecoveryPolicy) -> Result<Vec<String>> {
        Ok(self.recover_settling(kind, policy).await?.settled)
    }

    /// Like [`Self::recover`], also naming the jobs it failed.
    pub(in super::super) async fn recover_settling(
        &self,
        kind: &str,
        policy: RecoveryPolicy,
    ) -> Result<Recovered> {
        let mut tx = self.db.begin_write().await?;
        let mut stopped = Vec::new();
        let running: Vec<(String, i64, i64, i64)> = sqlx::query_as(
            "SELECT id,saved_work,recovery_mark,unsaved_starts FROM jobs WHERE kind=? AND status='running' ORDER BY created_at,rowid",
        )
        .bind(kind)
        .fetch_all(&mut *tx)
        .await?;
        for (id, saved_work, mark, unsaved_starts) in &running {
            match policy {
                RecoveryPolicy::Requeue if saved_work == mark && *unsaved_starts > 0 => {
                    let failed =
                        Self::fail_in(&mut tx, id, STOPPED_THE_APP, STOPPED_THE_APP_MESSAGE, None)
                            .await?;
                    if failed {
                        stopped.push(id.clone());
                    }
                    failed
                }
                RecoveryPolicy::Requeue => {
                    let resumed = Self::suspend_in(&mut tx, id, RESUMING).await?;
                    sqlx::query(
                        "UPDATE jobs SET recovery_mark=saved_work,unsaved_starts=1 WHERE id=?",
                    )
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                    resumed
                }
                RecoveryPolicy::Interrupt => Self::interrupt_in(&mut tx, id).await?,
            };
        }
        let settled = running.into_iter().map(|(id, ..)| id).collect();
        tx.commit().await?;
        Ok(Recovered { settled, stopped })
    }

    /// For an orderly exit, which is never a crash: in one statement, every
    /// running job of a kind in `resumable` becomes pending, and every other
    /// running job stays running for the next recovery to settle by its kind's
    /// policy. Each has its unsaved-start count cleared. Returns the jobs
    /// touched.
    pub async fn suspend_all_running(&self, resumable: &[&str]) -> Result<usize> {
        let stamp = now();
        let resumable =
            serde_json::to_string(resumable).map_err(|e| AppError::Serialization(e.to_string()))?;
        let mut tx = self.db.begin_write().await?;
        let suspended = sqlx::query("UPDATE jobs SET status=CASE WHEN kind IN (SELECT value FROM json_each(?1)) THEN 'pending' ELSE status END,progress_message=CASE WHEN kind IN (SELECT value FROM json_each(?1)) THEN ?2 ELSE progress_message END,heartbeat_at=?3,unsaved_starts=0 WHERE status='running'")
            .bind(resumable)
            .bind(SUSPENDED)
            .bind(stamp)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        tx.commit().await?;
        Ok(usize::try_from(suspended).unwrap_or(usize::MAX))
    }
}
