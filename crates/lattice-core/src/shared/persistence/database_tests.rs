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

#[tokio::test]
async fn begin_write_succeeds_immediately_even_when_all_reader_connections_are_held() {
    use std::time::Duration;

    let dir = tempfile::tempdir().unwrap();
    let options = SqliteConnectOptions::new()
        .filename(dir.path().join("separated.db"))
        .create_if_missing(true)
        .busy_timeout(Duration::from_secs(2))
        .pragma("journal_mode", "WAL");
    let reader_pool = SqlitePoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_millis(200))
        .connect_with(options.clone())
        .await
        .unwrap();
    let writer_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_millis(200))
        .connect_with(options)
        .await
        .unwrap();
    let db = Database::with_writer_pool(reader_pool, writer_pool);
    write_a_note(&db).await;

    let r1 = db.begin_read().await.unwrap();
    let r2 = db.begin_read().await.unwrap();
    assert!(
        db.begin_read().await.is_err(),
        "reader_pool must be saturated while both ReadTxs are held"
    );

    let mut tx = tokio::time::timeout(Duration::from_millis(200), db.begin_write())
        .await
        .expect("begin_write must not wait on saturated reader_pool")
        .unwrap();
    sqlx::query("INSERT INTO notes(body) VALUES('written-while-readers-busy')")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    drop((r1, r2));
    let mut read = db.begin_read().await.unwrap();
    assert_eq!(notes(&mut read).await, 2);
    drop(read);
    assert_eq!(db.open_connections(), 3);
    db.close().await;
}

#[tokio::test]
async fn vacuum_into_succeeds_when_reader_pool_enforces_query_only() {
    use std::time::Duration;

    let dir = tempfile::tempdir().unwrap();
    let base_options = SqliteConnectOptions::new()
        .filename(dir.path().join("separated.db"))
        .create_if_missing(true)
        .busy_timeout(Duration::from_secs(2))
        .pragma("journal_mode", "WAL");
    let writer_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(base_options.clone())
        .await
        .unwrap();
    let reader_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(base_options.pragma("query_only", "ON"))
        .await
        .unwrap();
    let db = Database::with_writer_pool(reader_pool, writer_pool);
    write_a_note(&db).await;

    let mut read = db.begin_read().await.unwrap();
    assert!(
        sqlx::query("INSERT INTO notes(body) VALUES('rejected')")
            .execute(&mut *read)
            .await
            .is_err(),
        "reader_pool must reject writes under query_only = ON"
    );
    drop(read);

    let copy = dir.path().join("copy.db");
    db.vacuum_into(copy.to_str().unwrap()).await.unwrap();

    // Ensure the pooled reader connection was restored to query_only = ON.
    let mut read = db.begin_read().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i32>("PRAGMA query_only")
            .fetch_one(&mut *read)
            .await
            .unwrap(),
        1
    );
    drop(read);
    db.close().await;

    let mut copied = SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(&copy))
        .await
        .unwrap();
    assert_eq!(notes(&mut copied).await, 1);
    copied.close().await.unwrap();
}

#[tokio::test]
async fn an_urgent_write_waits_for_one_standard_unit_and_not_for_the_queue() {
    let opened = open().await;
    let held = opened.db.begin_write().await.unwrap();
    let order = std::sync::Arc::new(std::sync::Mutex::new(Vec::<&'static str>::new()));

    let mut standard = Vec::new();
    for label in ["std-1", "std-2", "std-3"] {
        let db = opened.db.clone();
        let order = std::sync::Arc::clone(&order);
        standard.push(tokio::spawn(async move {
            let tx = db.begin_write().await.unwrap();
            order.lock().unwrap().push(label);
            tx.commit().await.unwrap();
        }));
        tokio::task::yield_now().await;
    }

    let urgent = {
        let db = opened.db.clone();
        let order = std::sync::Arc::clone(&order);
        tokio::spawn(async move {
            let tx = db.urgent().begin_write().await.unwrap();
            order.lock().unwrap().push("urgent");
            tx.commit().await.unwrap();
        })
    };
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    held.commit().await.unwrap();
    urgent.await.unwrap();
    for handle in standard {
        handle.await.unwrap();
    }

    let recorded = order.lock().unwrap().clone();
    assert_eq!(recorded.first().copied(), Some("urgent"));
    assert_eq!(recorded.len(), 4);
}

#[tokio::test]
async fn standard_writers_keep_their_order() {
    let opened = open().await;
    let held = opened.db.begin_write().await.unwrap();
    let order = std::sync::Arc::new(std::sync::Mutex::new(Vec::<usize>::new()));

    let mut handles = Vec::new();
    for index in 1..=4 {
        let db = opened.db.clone();
        let order = std::sync::Arc::clone(&order);
        handles.push(tokio::spawn(async move {
            let tx = db.begin_write().await.unwrap();
            order.lock().unwrap().push(index);
            tx.commit().await.unwrap();
        }));
        tokio::task::yield_now().await;
    }

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    held.commit().await.unwrap();
    for handle in handles {
        handle.await.unwrap();
    }

    assert_eq!(*order.lock().unwrap(), vec![1, 2, 3, 4]);
}
