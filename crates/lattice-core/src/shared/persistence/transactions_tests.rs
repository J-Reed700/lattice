use super::{PersistenceErrorKind, Transactions};
use crate::shared::testing::sqlite::file_options;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::time::{Duration, Instant};

const HELD: Duration = Duration::from_millis(300);

async fn open() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(file_options(dir.path().join("t.db")))
        .await
        .unwrap();
    sqlx::query("CREATE TABLE notes(id INTEGER PRIMARY KEY, body TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO notes(body) VALUES('first')")
        .execute(&pool)
        .await
        .unwrap();
    (dir, pool)
}

/// Another connection takes the write lock now and commits after `HELD`.
async fn another_writer(pool: &SqlitePool) -> tokio::task::JoinHandle<()> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    sqlx::query("INSERT INTO notes(body) VALUES('other')")
        .execute(&mut *tx)
        .await
        .unwrap();
    tokio::spawn(async move {
        tokio::time::sleep(HELD).await;
        tx.commit().await.unwrap();
    })
}

async fn notes(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM notes")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_write_transaction_that_reads_first_waits_for_the_writer_ahead_of_it() {
    let (_dir, pool) = open().await;
    let other = another_writer(&pool).await;
    let started = Instant::now();
    let mut tx = pool.begin_write().await.unwrap();
    let seen: i64 = sqlx::query_scalar("SELECT count(*) FROM notes")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO notes(body) VALUES('mine')")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    other.await.unwrap();
    assert_eq!(seen, 2, "it read after the other writer committed");
    assert!(started.elapsed() >= HELD / 2, "it waited for the lock");
    assert_eq!(notes(&pool).await, 3);
}

#[tokio::test]
async fn a_writer_meanwhile_waits_for_a_write_transaction_instead_of_failing() {
    let (_dir, pool) = open().await;
    let mut tx = pool.begin_write().await.unwrap();
    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM notes")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let other = {
        let pool = pool.clone();
        tokio::spawn(async move {
            sqlx::query("INSERT INTO notes(body) VALUES('other')")
                .execute(&pool)
                .await
                .map(|_| ())
        })
    };
    tokio::time::sleep(HELD).await;
    sqlx::query("INSERT INTO notes(body) VALUES('mine')")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    other.await.unwrap().unwrap();
    assert_eq!(notes(&pool).await, 3);
}

#[tokio::test]
async fn a_read_transaction_that_writes_fails_at_once_with_database_is_locked() {
    let (_dir, pool) = open().await;
    let other = another_writer(&pool).await;
    let mut tx = pool.begin_read().await.unwrap();
    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM notes")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let started = Instant::now();
    let error = sqlx::query("INSERT INTO notes(body) VALUES('mine')")
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("database is locked"), "{error}");
    assert!(
        started.elapsed() < HELD / 2,
        "the busy timeout did not wait"
    );
    drop(tx);
    other.await.unwrap();
}

#[tokio::test]
async fn beginning_on_a_closed_pool_reports_it_closed() {
    let (_dir, pool) = open().await;
    pool.close().await;
    let write = pool.begin_write().await.err().unwrap();
    let read = pool.begin_read().await.err().unwrap();
    assert_eq!(write.kind(), PersistenceErrorKind::Closed);
    assert_eq!(read.kind(), PersistenceErrorKind::Closed);
}
