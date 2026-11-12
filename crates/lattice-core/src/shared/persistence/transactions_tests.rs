use super::{Database, PersistenceError, PersistenceErrorKind};
use crate::shared::testing::sqlite::file_options;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long a unit keeps the write lock while another waits behind it.
const HELD: Duration = Duration::from_millis(300);

/// Long enough that no test here waits it out unless it sets a shorter one.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

struct Opened {
    _dir: tempfile::TempDir,
    pool: SqlitePool,
    db: Database,
}

async fn open() -> Opened {
    open_with(SqlitePoolOptions::new().max_connections(4), BUSY_TIMEOUT).await
}

async fn open_with(pool: SqlitePoolOptions, busy_timeout: Duration) -> Opened {
    let dir = tempfile::tempdir().unwrap();
    let pool = pool
        .connect_with(file_options(dir.path().join("t.db")).busy_timeout(busy_timeout))
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
    let db = Database::new(pool.clone());
    Opened {
        _dir: dir,
        pool,
        db,
    }
}

/// A connection outside the write queue, as another program's would be, takes
/// the write lock now and commits after `HELD`.
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

/// A unit of work on `db` in a task of its own that waits for its turn, adds a
/// note and commits.
fn writer(db: &Database) -> tokio::task::JoinHandle<Result<(), PersistenceError>> {
    let db = db.clone();
    tokio::spawn(async move {
        let mut tx = db.begin_write().await?;
        sqlx::query("INSERT INTO notes(body) VALUES('waited')")
            .execute(&mut *tx)
            .await?;
        tx.commit().await
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
    let opened = open().await;
    let other = another_writer(&opened.pool).await;
    let started = Instant::now();
    let mut tx = opened.db.begin_write().await.unwrap();
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
    assert_eq!(notes(&opened.pool).await, 3);
}

#[tokio::test]
async fn a_writer_meanwhile_waits_for_a_write_transaction_instead_of_failing() {
    let opened = open().await;
    let mut tx = opened.db.begin_write().await.unwrap();
    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM notes")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let other = {
        let pool = opened.pool.clone();
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
    assert_eq!(notes(&opened.pool).await, 3);
}

#[tokio::test]
async fn a_read_transaction_that_writes_fails_at_once_as_busy() {
    let opened = open().await;
    let other = another_writer(&opened.pool).await;
    let mut tx = opened.db.begin_read().await.unwrap();
    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM notes")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let started = Instant::now();
    let error = sqlx::query("INSERT INTO notes(body) VALUES('mine')")
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert!(
        started.elapsed() < HELD / 2,
        "the busy timeout did not wait"
    );
    assert_eq!(
        PersistenceError::from(error).kind(),
        PersistenceErrorKind::Busy
    );
    drop(tx);
    other.await.unwrap();
}

#[tokio::test]
async fn beginning_on_a_closed_database_reports_it_closed() {
    let opened = open().await;
    opened.db.close().await;
    let write = opened.db.begin_write().await.err().unwrap();
    let read = opened.db.begin_read().await.err().unwrap();
    assert_eq!(write.kind(), PersistenceErrorKind::Closed);
    assert_eq!(read.kind(), PersistenceErrorKind::Closed);
}

#[tokio::test]
async fn a_writer_that_waits_longer_than_the_busy_timeout_still_commits() {
    let opened = open_with(SqlitePoolOptions::new().max_connections(4), HELD / 3).await;
    let mut held = opened.db.begin_write().await.unwrap();
    sqlx::query("INSERT INTO notes(body) VALUES('held')")
        .execute(&mut *held)
        .await
        .unwrap();
    let waiting = writer(&opened.db);
    tokio::time::sleep(HELD).await;
    held.commit().await.unwrap();
    waiting.await.unwrap().unwrap();
    assert_eq!(notes(&opened.pool).await, 3);
}

#[tokio::test]
async fn writers_take_turns_in_the_order_they_asked() {
    let opened = open().await;
    let committed = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let run_of_short_writes = {
        let db = opened.db.clone();
        let committed = Arc::clone(&committed);
        let stop = Arc::clone(&stop);
        tokio::spawn(async move {
            while !stop.load(Ordering::SeqCst) {
                let mut tx = db.begin_write().await.unwrap();
                sqlx::query("INSERT INTO notes(body) VALUES('short')")
                    .execute(&mut *tx)
                    .await
                    .unwrap();
                tokio::time::sleep(Duration::from_millis(2)).await;
                tx.commit().await.unwrap();
                committed.fetch_add(1, Ordering::SeqCst);
            }
        })
    };
    tokio::task::yield_now().await;
    let asked = committed.load(Ordering::SeqCst);
    let mine = opened.db.begin_write().await;
    let ahead = committed.load(Ordering::SeqCst) - asked;
    stop.store(true, Ordering::SeqCst);
    assert!(
        ahead <= 1,
        "{ahead} writes went ahead of one that asked before them"
    );
    mine.unwrap().commit().await.unwrap();
    run_of_short_writes.await.unwrap();
}

#[tokio::test]
async fn readers_go_ahead_while_writers_wait_for_their_turn() {
    let opened = open_with(
        SqlitePoolOptions::new()
            .max_connections(2)
            .acquire_timeout(HELD),
        BUSY_TIMEOUT,
    )
    .await;
    let held = opened.db.begin_write().await.unwrap();
    let waiting: Vec<_> = (0..3).map(|_| writer(&opened.db)).collect();
    tokio::task::yield_now().await;
    let mut read = opened.db.begin_read().await.unwrap();
    let seen: i64 = sqlx::query_scalar("SELECT count(*) FROM notes")
        .fetch_one(&mut *read)
        .await
        .unwrap();
    read.commit().await.unwrap();
    assert_eq!(seen, 1);
    drop(held);
    for waiting in waiting {
        waiting.await.unwrap().unwrap();
    }
    assert_eq!(notes(&opened.pool).await, 4);
}

#[tokio::test]
async fn closing_the_database_ends_the_writers_waiting_for_their_turn() {
    let opened = open().await;
    let held = opened.db.begin_write().await.unwrap();
    let waiting = {
        let db = opened.db.clone();
        tokio::spawn(async move { db.begin_write().await.map(drop) })
    };
    tokio::task::yield_now().await;
    let closing = {
        let db = opened.db.clone();
        tokio::spawn(async move { db.close().await })
    };
    let error = waiting.await.unwrap().unwrap_err();
    assert_eq!(error.kind(), PersistenceErrorKind::Closed);
    drop(held);
    closing.await.unwrap();
}

#[tokio::test]
async fn a_vacuum_waits_for_its_turn() {
    let opened = open_with(SqlitePoolOptions::new().max_connections(4), HELD / 3).await;
    let held = opened.db.begin_write().await.unwrap();
    let vacuum = {
        let db = opened.db.clone();
        tokio::spawn(async move { db.vacuum().await })
    };
    tokio::time::sleep(HELD).await;
    held.commit().await.unwrap();
    vacuum.await.unwrap().unwrap();
}
