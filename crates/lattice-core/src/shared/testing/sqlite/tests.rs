use super::*;

async fn freshly_migrated() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    pool
}

async fn schema(pool: &SqlitePool) -> Vec<(String, String, Option<String>)> {
    sqlx::query_as("SELECT type, name, sql FROM sqlite_master ORDER BY type, name")
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn applied(pool: &SqlitePool) -> Vec<(i64, Vec<u8>, bool)> {
    sqlx::query_as("SELECT version, checksum, success FROM _sqlx_migrations ORDER BY version")
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn rows_per_table(pool: &SqlitePool) -> Vec<(String, i64)> {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name <> '_sqlx_migrations' ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let mut rows = Vec::new();
    for table in tables {
        let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM \"{table}\""
        )))
        .fetch_one(pool)
        .await
        .unwrap();
        rows.push((table, count));
    }
    rows
}

#[tokio::test]
async fn a_migrated_database_has_the_schema_and_rows_the_migrations_make() {
    let fresh = freshly_migrated().await;
    let copied = migrated_in_memory().await.unwrap();

    assert_eq!(schema(&copied).await, schema(&fresh).await);
    assert_eq!(applied(&copied).await, applied(&fresh).await);
    assert_eq!(rows_per_table(&copied).await, rows_per_table(&fresh).await);
    let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&copied)
        .await
        .unwrap();
    assert_eq!(foreign_keys, 1);
}

#[tokio::test]
async fn each_migrated_database_is_its_own() {
    let first = migrated_in_memory().await.unwrap();
    let second = migrated_in_memory().await.unwrap();

    sqlx::query("CREATE TABLE only_in_the_first (x)")
        .execute(&first)
        .await
        .unwrap();

    let in_second: i64 =
        sqlx::query_scalar("SELECT count(*) FROM sqlite_master WHERE name = 'only_in_the_first'")
            .fetch_one(&second)
            .await
            .unwrap();
    assert_eq!(in_second, 0);
}

#[tokio::test]
async fn a_written_database_opens_as_production_opens_it_with_nothing_left_to_migrate() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("lattice.db");
    write_migrated(&path).await.unwrap();

    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(file_options(&path))
        .await
        .unwrap();

    let journal: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(journal, "wal");
    assert_eq!(
        applied(&pool).await,
        applied(&freshly_migrated().await).await
    );
    MIGRATOR.run(&pool).await.unwrap();
    pool.close().await;
}
