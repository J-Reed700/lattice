use super::{Database, Transactions};
use sqlx::sqlite::SqlitePoolOptions;

async fn open() -> Database {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    pool.into()
}

#[tokio::test]
async fn the_database_is_reachable_until_it_is_closed() {
    let db = open().await;
    assert!(db.is_reachable().await);
    db.close().await;
    assert!(!db.is_reachable().await);
}

#[tokio::test]
async fn a_unit_of_work_begun_on_the_database_persists_when_committed() {
    let db = open().await;
    let mut tx = db.begin_write().await.unwrap();
    sqlx::query("CREATE TABLE notes(id INTEGER PRIMARY KEY, body TEXT NOT NULL)")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO notes(body) VALUES('kept')")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let mut read = db.begin_read().await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM notes")
        .fetch_one(&mut *read)
        .await
        .unwrap();
    assert_eq!(count, 1);
}
