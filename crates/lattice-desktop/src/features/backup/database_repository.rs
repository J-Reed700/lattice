//! Statements run on the live database and on backup snapshots.
use sqlx::{Executor, Sqlite};

type Result<T> = std::result::Result<T, sqlx::Error>;

/// `VACUUM INTO` takes a string literal, not a bind parameter, so the single
/// quotes of `destination` are doubled.
pub async fn vacuum_into(
    executor: impl Executor<'_, Database = Sqlite>,
    destination: &str,
) -> Result<()> {
    let escaped = destination.replace('\'', "''");
    sqlx::query(&format!("VACUUM INTO '{escaped}'"))
        .execute(executor)
        .await?;
    Ok(())
}

/// Reclaims free pages. SQLite refuses it inside a transaction.
pub async fn vacuum(executor: impl Executor<'_, Database = Sqlite>) -> Result<()> {
    sqlx::query("VACUUM").execute(executor).await?;
    Ok(())
}

pub async fn integrity_check(executor: impl Executor<'_, Database = Sqlite>) -> Result<String> {
    sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(executor)
        .await
}

pub async fn table_exists(
    executor: impl Executor<'_, Database = Sqlite>,
    table: &str,
) -> Result<bool> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(table)
            .fetch_one(executor)
            .await?;
    Ok(count > 0)
}

pub async fn column_exists(
    executor: impl Executor<'_, Database = Sqlite>,
    table: &str,
    column: &str,
) -> Result<bool> {
    // `pragma_table_info` is a table-valued function, so its argument is a
    // string literal rather than a bind parameter.
    let escaped = table.replace('\'', "''");
    let names: Vec<String> =
        sqlx::query_scalar(&format!("SELECT name FROM pragma_table_info('{escaped}')"))
            .fetch_all(executor)
            .await?;
    Ok(names.iter().any(|name| name == column))
}

pub async fn table_count(executor: impl Executor<'_, Database = Sqlite>) -> Result<i64> {
    sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'")
        .fetch_one(executor)
        .await
}

pub async fn count_rows(
    executor: impl Executor<'_, Database = Sqlite>,
    table: &str,
) -> Result<i64> {
    let escaped = table.replace('"', "\"\"");
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM \"{escaped}\""))
        .fetch_one(executor)
        .await
}

pub async fn clear_table(
    executor: impl Executor<'_, Database = Sqlite>,
    table: &str,
) -> Result<()> {
    let escaped = table.replace('"', "\"\"");
    sqlx::query(&format!("DELETE FROM \"{escaped}\""))
        .execute(executor)
        .await?;
    Ok(())
}

/// The FTS5 tables with the SQL that created them.
pub async fn fts5_tables(
    executor: impl Executor<'_, Database = Sqlite>,
) -> Result<Vec<(String, Option<String>)>> {
    sqlx::query_as(
        "SELECT name, sql FROM sqlite_master \
         WHERE type = 'table' AND sql IS NOT NULL AND lower(sql) LIKE '%using fts5%'",
    )
    .fetch_all(executor)
    .await
}

pub async fn rebuild_fts5_index(
    executor: impl Executor<'_, Database = Sqlite>,
    table: &str,
) -> Result<()> {
    let escaped = table.replace('"', "\"\"");
    sqlx::query(&format!(
        "INSERT INTO \"{escaped}\"(\"{escaped}\") VALUES('rebuild')"
    ))
    .execute(executor)
    .await?;
    Ok(())
}

/// Highest applied migration.
pub async fn newest_migration(
    executor: impl Executor<'_, Database = Sqlite>,
) -> Result<Option<i64>> {
    sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_one(executor)
        .await
}

pub async fn newest_memory_schema_version(
    executor: impl Executor<'_, Database = Sqlite>,
) -> Result<Option<i64>> {
    sqlx::query_scalar::<_, Option<i64>>(
        "SELECT MAX(schema_version) FROM conversation_memory_state",
    )
    .fetch_one(executor)
    .await
}

pub async fn distinct_checksums(
    executor: impl Executor<'_, Database = Sqlite>,
) -> Result<Vec<Option<String>>> {
    sqlx::query_scalar("SELECT DISTINCT checksum FROM documents")
        .fetch_all(executor)
        .await
}

pub async fn document_count(executor: impl Executor<'_, Database = Sqlite>) -> Result<i64> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
        .fetch_one(executor)
        .await?;
    Ok(count)
}

pub async fn has_schema_version_table(
    executor: impl Executor<'_, Database = Sqlite>,
) -> Result<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='schema_version'",
    )
    .fetch_one(executor)
    .await?;
    Ok(count != 0)
}

pub async fn latest_schema_version(
    executor: impl Executor<'_, Database = Sqlite>,
) -> Result<Option<i64>> {
    sqlx::query_scalar("SELECT version FROM schema_version ORDER BY version DESC LIMIT 1")
        .fetch_optional(executor)
        .await
}
