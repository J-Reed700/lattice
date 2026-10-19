//! A completed job that handed work back to the user continues in a new
//! attempt for what is left, with its saved work; a retry still admits only
//! failed, interrupted or cancelled jobs.
use super::*;
use crate::shared::error::{AppError, Result};
use serde_json::json;

async fn pool() -> sqlx::SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::migrate!("../lattice-desktop/migrations")
        .run(&pool)
        .await
        .unwrap();
    pool
}

async fn continued(
    pool: &sqlx::SqlitePool,
    id: &str,
    operation: &str,
    requested: &serde_json::Value,
    total: u32,
) -> Result<Admitted> {
    let mut tx = pool.begin().await.unwrap();
    let admitted =
        JobStore::continue_in(&mut tx, id, operation, "Resume queued", requested, total).await?;
    tx.commit().await.unwrap();
    Ok(admitted)
}

#[tokio::test]
async fn a_completed_job_continues_with_its_saved_work_for_what_is_left() -> Result<()> {
    let pool = pool().await;
    let store = JobStore::new(pool.clone());
    let first = store
        .submit(&NewJob {
            kind: "test.work".into(),
            subject_id: Some("course".into()),
            operation_id: "first".into(),
            payload_hash: "payload".into(),
            requested: json!({"targets": ["a", "b"]}),
            progress_total: 2,
            message: "Queued".into(),
        })
        .await?
        .id;
    let left = json!({"targets": ["b"]});
    assert!(matches!(
        continued(&pool, &first, "too-early", &left, 1).await,
        Err(AppError::InvalidState(_))
    ));
    assert!(store.claim(&first).await?.is_some());
    assert!(
        store
            .put_checkpoint(&first, "step", &json!({"done": 1}))
            .await?
    );
    assert!(
        store
            .update_staged(&first, |_| Ok(json!({"draft": "saved"})))
            .await?
    );
    assert!(store.complete(&first, "held:b", "Waits for you").await?);
    // A completed job is not retried: its runtime semantics stay as they are.
    assert!(matches!(
        store.retry(&first, "retry", "Retry queued").await,
        Err(AppError::InvalidState(_))
    ));

    let next = continued(&pool, &first, "resume-1", &left, 1).await?;
    assert!(next.created);
    let child = store.get(&next.id).await?;
    assert_eq!(child.retry_of_job_id.as_deref(), Some(first.as_str()));
    assert_eq!(child.status, JobStatus::Pending);
    assert_eq!(child.requested, left);
    assert_eq!(child.progress_total, 1);
    assert_eq!(child.progress_current, 0);
    assert_eq!(child.subject_id.as_deref(), Some("course"));
    assert_eq!(child.staged_result, Some(json!({"draft": "saved"})));
    assert_eq!(
        store.checkpoint(&next.id, "step").await?,
        Some(json!({"done": 1}))
    );
    // One live attempt per job: replaying or continuing again returns it.
    let again = continued(&pool, &first, "resume-2", &left, 1).await?;
    assert_eq!(
        (again.id.as_str(), again.created),
        (next.id.as_str(), false)
    );
    Ok(())
}
