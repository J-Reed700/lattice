//! Durable redelivery after a temporary service or connection outage.
use super::*;

impl LearningCurriculumRepository {
    pub(in crate::features::learning) async fn defer_job(
        &self,
        id: &str,
        error: &AppError,
    ) -> Result<()> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;
        let row = sqlx::query(
            "SELECT status,kind,retry_count,activity_json FROM learning_generation_jobs WHERE id=?",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        // Cancellation and completed publication always win over late failures.
        if row.get::<String, _>("status") != "running"
            || row.get::<String, _>("kind") != "lesson_preparation"
        {
            tx.commit().await.map_err(db)?;
            return Ok(());
        }
        let stamp = now();
        let retries = row.get::<i64, _>("retry_count");
        // This caps the delay between attempts, never the job's running time or
        // number of attempts. The dispatcher releases the worker while waiting.
        let delay_ms = (30_000_i64 << retries.clamp(0, 4) as u32).min(300_000);
        let mut activity: Option<curriculum::LearningGenerationActivity> = row
            .get::<Option<String>, _>("activity_json")
            .as_deref()
            .map(decode)
            .transpose()?;
        if let Some(activity) = &mut activity {
            activity.model_running = false;
            activity.response_characters = 0;
            activity.model_attempt = 0;
            activity.last_activity_at = stamp;
        }
        let message = match error {
            AppError::Network(_) => "Connection interrupted; saved work will resume automatically",
            AppError::RateLimitExceeded(_) => "The model service is rate limiting requests; preparation will retry automatically",
            _ => "The model service could not finish the request; preparation will retry automatically",
        };
        let detail = "Your draft and completed checks are saved. Preparation retries automatically while Lattice is open, with increasing delays of up to five minutes. Repeated service failures can prevent further progress until the service recovers. You can leave this screen or cancel.";
        sqlx::query("UPDATE learning_generation_jobs SET status='pending',finished_at=NULL,error_code='temporarily_unavailable',error_message=?,progress_message=?,heartbeat_at=?,activity_json=?,retry_not_before=?,retry_count=? WHERE id=? AND status='running'")
            .bind(detail).bind(message).bind(stamp).bind(activity.as_ref().map(encode).transpose()?)
            .bind(stamp.saturating_add(delay_ms)).bind(retries.saturating_add(1)).bind(id)
            .execute(&mut *tx).await.map_err(db)?;
        sqlx::query("INSERT INTO learning_generation_job_events(job_id,ordinal,status,progress_current,message,created_at) SELECT j.id,(SELECT coalesce(max(ordinal),-1)+1 FROM learning_generation_job_events WHERE job_id=j.id),'pending',j.progress_current,?,? FROM learning_generation_jobs j WHERE j.id=?")
            .bind(message).bind(stamp).bind(id).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)
    }
}

#[cfg(test)]
#[test]
fn only_database_pool_admission_timeout_is_a_temporary_service_failure() {
    assert!(matches!(
        db(sqlx::Error::PoolTimedOut),
        AppError::ServiceNotAvailable(_)
    ));
    assert!(matches!(
        db(sqlx::Error::RowNotFound),
        AppError::Database(_)
    ));
    assert!(matches!(
        db(sqlx::Error::Protocol("invalid state".into())),
        AppError::Database(_)
    ));
}
