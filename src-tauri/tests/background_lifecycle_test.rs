//! Shutdown must join final database writes after cancelling background work.
#![allow(clippy::unwrap_used)]
use lattice::shared::runtime::background::BackgroundTasks;
use sqlx::sqlite::SqlitePoolOptions;
use std::time::Duration;

#[tokio::test]
async fn shutdown_joins_cleanup_before_database_close() {
    let tasks = BackgroundTasks::default();
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::query("CREATE TABLE results (value TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    let cancel = tasks.token();
    let worker_pool = pool.clone();
    let (release, released) = tokio::sync::oneshot::channel();
    let worker = tasks
        .spawn(async move {
            cancel.cancelled().await;
            released.await.unwrap();
            sqlx::query("INSERT INTO results VALUES ('interrupted')")
                .execute(&worker_pool)
                .await
                .unwrap();
        })
        .unwrap();
    tasks.close();
    assert!(tasks.spawn(async {}).is_none());
    assert!(
        tokio::time::timeout(Duration::from_millis(20), tasks.wait())
            .await
            .is_err()
    );
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), tasks.wait())
        .await
        .unwrap();
    worker.await.unwrap();
    let value: String = sqlx::query_scalar("SELECT value FROM results")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(value, "interrupted");
    pool.close().await;
}
