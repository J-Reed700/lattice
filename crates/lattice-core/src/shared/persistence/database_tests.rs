use super::Database;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Connection, SqliteConnection};

async fn open() -> Database {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    Database::new(pool)
}

async fn write_a_note(db: &Database) {
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
}

async fn notes(connection: &mut SqliteConnection) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM notes")
        .fetch_one(connection)
        .await
        .unwrap()
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
    write_a_note(&db).await;
    let mut read = db.begin_read().await.unwrap();
    assert_eq!(notes(&mut read).await, 1);
}

#[tokio::test]
async fn a_copy_goes_to_a_path_with_sql_in_its_name() {
    let dir = tempfile::tempdir().unwrap();
    let live = SqliteConnectOptions::new()
        .filename(dir.path().join("live.db"))
        .create_if_missing(true);
    let db = Database::new(
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(live)
            .await
            .unwrap(),
    );
    write_a_note(&db).await;
    let copy = dir.path().join("it's; -- a copy.db");
    db.vacuum_into(copy.to_str().unwrap()).await.unwrap();
    db.close().await;
    let mut copied = SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(&copy))
        .await
        .unwrap();
    assert_eq!(notes(&mut copied).await, 1);
    copied.close().await.unwrap();
}

#[tokio::test]
async fn the_open_connections_are_the_pools() {
    let db = open().await;
    assert!(db.is_reachable().await);
    assert_eq!(db.open_connections(), 1);
}
