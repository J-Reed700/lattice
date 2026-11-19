//! Statements run on the live database and on backup snapshots.
use lattice_core::shared::persistence::{Database, WriteTx};
use sqlx::SqliteConnection;

type Result<T> = std::result::Result<T, sqlx::Error>;

/// `integrity_check` in a read unit.
pub async fn database_integrity_check(database: &Database) -> Result<String> {
    let mut read = database.begin_read().await?;
    let integrity = integrity_check(&mut read).await?;
    read.commit().await?;
    Ok(integrity)
}

/// `document_count` in a read unit.
pub async fn database_document_count(database: &Database) -> Result<i64> {
    let mut read = database.begin_read().await?;
    let count = document_count(&mut read).await?;
    read.commit().await?;
    Ok(count)
}

/// `newest_migration` in a read unit.
pub async fn database_newest_migration(database: &Database) -> Result<Option<i64>> {
    let mut read = database.begin_read().await?;
    let version = newest_migration(&mut read).await?;
    read.commit().await?;
    Ok(version)
}

/// What a backup's `schema_version` table says.
pub enum SchemaVersion {
    NoTable,
    Table(Option<i64>),
}

/// Which step of reading a backup's schema version failed.
pub enum SchemaVersionError {
    Check(sqlx::Error),
    Read(sqlx::Error),
}

/// Whether the backup has a `schema_version` table and its latest row, on one
/// connection.
pub async fn backup_schema_version(
    database: &Database,
) -> std::result::Result<SchemaVersion, SchemaVersionError> {
    let mut tx = database
        .begin_read()
        .await
        .map_err(|error| SchemaVersionError::Check(error.into()))?;
    if !has_schema_version_table(&mut tx)
        .await
        .map_err(SchemaVersionError::Check)?
    {
        return Ok(SchemaVersion::NoTable);
    }
    let version = latest_schema_version(&mut tx)
        .await
        .map_err(SchemaVersionError::Read)?;
    tx.commit()
        .await
        .map_err(|error| SchemaVersionError::Read(error.into()))?;
    Ok(SchemaVersion::Table(version))
}

pub async fn integrity_check(connection: &mut SqliteConnection) -> Result<String> {
    sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(connection)
        .await
}

pub async fn table_exists(connection: &mut SqliteConnection, table: &str) -> Result<bool> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(table)
            .fetch_one(connection)
            .await?;
    Ok(count > 0)
}

pub async fn column_exists(
    connection: &mut SqliteConnection,
    table: &str,
    column: &str,
) -> Result<bool> {
    // `pragma_table_info` is a table-valued function, so its argument is a
    // string literal rather than a bind parameter.
    let escaped = table.replace('\'', "''");
    let names: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT name FROM pragma_table_info('{escaped}')"
    )))
    .fetch_all(connection)
    .await?;
    Ok(names.iter().any(|name| name == column))
}

pub async fn table_count(connection: &mut SqliteConnection) -> Result<i64> {
    sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'")
        .fetch_one(connection)
        .await
}

pub async fn count_rows(connection: &mut SqliteConnection, table: &str) -> Result<i64> {
    let escaped = table.replace('"', "\"\"");
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT COUNT(*) FROM \"{escaped}\""
    )))
    .fetch_one(connection)
    .await
}

pub async fn clear_table(tx: &mut WriteTx, table: &str) -> Result<()> {
    let escaped = table.replace('"', "\"\"");
    sqlx::query(sqlx::AssertSqlSafe(format!("DELETE FROM \"{escaped}\"")))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// The FTS5 tables with the SQL that created them.
pub async fn fts5_tables(
    connection: &mut SqliteConnection,
) -> Result<Vec<(String, Option<String>)>> {
    sqlx::query_as(
        "SELECT name, sql FROM sqlite_master \
         WHERE type = 'table' AND sql IS NOT NULL AND lower(sql) LIKE '%using fts5%'",
    )
    .fetch_all(connection)
    .await
}

pub async fn rebuild_fts5_index(tx: &mut WriteTx, table: &str) -> Result<()> {
    let escaped = table.replace('"', "\"\"");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO \"{escaped}\"(\"{escaped}\") VALUES('rebuild')"
    )))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Highest applied migration.
pub async fn newest_migration(connection: &mut SqliteConnection) -> Result<Option<i64>> {
    sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_one(connection)
        .await
}

pub async fn newest_memory_schema_version(
    connection: &mut SqliteConnection,
) -> Result<Option<i64>> {
    sqlx::query_scalar::<_, Option<i64>>(
        "SELECT MAX(schema_version) FROM conversation_memory_state",
    )
    .fetch_one(connection)
    .await
}

pub async fn distinct_checksums(connection: &mut SqliteConnection) -> Result<Vec<Option<String>>> {
    sqlx::query_scalar("SELECT DISTINCT checksum FROM documents")
        .fetch_all(connection)
        .await
}

pub async fn document_count(connection: &mut SqliteConnection) -> Result<i64> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
        .fetch_one(connection)
        .await?;
    Ok(count)
}

pub async fn has_schema_version_table(connection: &mut SqliteConnection) -> Result<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='schema_version'",
    )
    .fetch_one(connection)
    .await?;
    Ok(count != 0)
}

pub async fn latest_schema_version(connection: &mut SqliteConnection) -> Result<Option<i64>> {
    sqlx::query_scalar("SELECT version FROM schema_version ORDER BY version DESC LIMIT 1")
        .fetch_optional(connection)
        .await
}
