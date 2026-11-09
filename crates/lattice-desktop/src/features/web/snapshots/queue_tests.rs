//! A capture is queued as one unit: its row and its job land together or not at all.
use std::sync::Arc;

use sqlx::SqlitePool;

use super::{SnapshotEvents, WebPageSnapshotService};
use crate::features::web::mocks::MockPageCapture;

struct NoEvents;

impl SnapshotEvents for NoEvents {
    fn emit(&self, _event: &str, _payload: serde_json::Value) {}
}

async fn service() -> (SqlitePool, Arc<WebPageSnapshotService>, tempfile::TempDir) {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO conversations (id, title, space_id, created_at, updated_at) \
         VALUES ('conv-queue', 'Queue', 'space_general', '2026-10-13T00:00:00Z', '2026-10-13T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let sweep_gate = Arc::new(lattice_core::shared::fs::SweepGate::default());
    sweep_gate.open();
    let service = Arc::new(WebPageSnapshotService::new(
        pool.clone(),
        dir.path().join("web-snapshots"),
        Arc::new(MockPageCapture::new()),
        lattice_core::shared::runtime::jobs::JobRuntime::new(pool.clone()),
        Arc::new(NoEvents),
        Arc::new(lattice_core::shared::runtime::background::BackgroundTasks::default()),
        sweep_gate,
    ));
    (pool, service, dir)
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_job_that_cannot_be_saved_leaves_no_snapshot_row() {
    let (pool, service, _dir) = service().await;
    sqlx::query(
        "CREATE TRIGGER refuse_jobs BEFORE INSERT ON jobs \
         BEGIN SELECT RAISE(ABORT, 'injected'); END",
    )
    .execute(&pool)
    .await
    .unwrap();

    let result = service
        .ensure_queued("conv-queue", "https://example.org/page", false)
        .await;

    assert!(result.is_err());
    assert_eq!(count(&pool, "web_page_snapshots").await, 0);
}

#[tokio::test]
async fn a_row_that_cannot_record_its_job_leaves_no_job_behind() {
    let (pool, service, _dir) = service().await;
    sqlx::query(
        "CREATE TRIGGER refuse_job_id BEFORE UPDATE OF job_id ON web_page_snapshots \
         BEGIN SELECT RAISE(ABORT, 'injected'); END",
    )
    .execute(&pool)
    .await
    .unwrap();

    let result = service
        .ensure_queued("conv-queue", "https://example.org/page", false)
        .await;

    assert!(result.is_err());
    assert_eq!(count(&pool, "jobs").await, 0);
    assert_eq!(count(&pool, "web_page_snapshots").await, 0);
}

/// Crash point: the page file is written, then the row cannot turn `ready`.
/// The file goes with the failed attempt; the row says why.
#[tokio::test]
async fn a_row_that_cannot_turn_ready_leaves_a_failed_row_and_no_page_file() {
    let (pool, service, dir) = service().await;
    let failures = service.jobs.register_all(vec![service.job_kind()]).await;
    assert!(failures.is_empty(), "{failures:?}");
    sqlx::query(
        "CREATE TRIGGER refuse_ready BEFORE UPDATE ON web_page_snapshots \
         WHEN NEW.status = 'ready' BEGIN SELECT RAISE(ABORT, 'injected'); END",
    )
    .execute(&pool)
    .await
    .unwrap();

    service
        .ensure_queued("conv-queue", "https://example.org/page", false)
        .await
        .unwrap();

    let mut status = String::new();
    for _ in 0..500 {
        status = sqlx::query_scalar("SELECT status FROM web_page_snapshots")
            .fetch_one(&pool)
            .await
            .unwrap();
        if status != "pending" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(status, "failed");
    let code: Option<String> = sqlx::query_scalar("SELECT error_code FROM web_page_snapshots")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(code.as_deref(), Some("storage"));
    let pages = std::fs::read_dir(dir.path().join("web-snapshots"))
        .map(|entries| entries.flatten().count())
        .unwrap_or(0);
    assert_eq!(pages, 0);
}
