//! Statements run on the live database and on backup snapshots.
use std::path::Path;

use lattice_core::shared::persistence::driver::DriverPool;
use lattice_core::shared::persistence::Database;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{SqliteConnection, SqlitePool};

type Result<T> = std::result::Result<T, sqlx::Error>;

/// Opens an existing backup file read-only, built from `SqliteConnectOptions`
/// rather than a `sqlite://…?mode=ro` DSN: a canonical Windows path
/// (`\\?\C:\...`) contains a `?`, where sqlx splits a DSN.
pub async fn open_read_only(path: &Path) -> Result<Database> {
    let options = SqliteConnectOptions::new().filename(path).read_only(true);
    SqlitePool::connect_with(options).await.map(Database::from)
}

/// `vacuum_into` on a connection of its own.
pub async fn vacuum_database_into(database: &impl DriverPool, destination: &str) -> Result<()> {
    let mut connection = database.sqlx_pool().acquire().await?;
    vacuum_into(&mut connection, destination).await
}

/// `integrity_check` on a connection of its own.
pub async fn database_integrity_check(database: &impl DriverPool) -> Result<String> {
    let mut connection = database.sqlx_pool().acquire().await?;
    integrity_check(&mut connection).await
}

/// `document_count` on a connection of its own.
pub async fn database_document_count(database: &impl DriverPool) -> Result<i64> {
    let mut connection = database.sqlx_pool().acquire().await?;
    document_count(&mut connection).await
}

/// `newest_migration` on a connection of its own.
pub async fn database_newest_migration(database: &impl DriverPool) -> Result<Option<i64>> {
    let mut connection = database.sqlx_pool().acquire().await?;
    newest_migration(&mut connection).await
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
    database: &impl DriverPool,
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

/// `VACUUM INTO` takes a string literal, not a bind parameter, so the single
/// quotes of `destination` are doubled.
pub async fn vacuum_into(connection: &mut SqliteConnection, destination: &str) -> Result<()> {
    let escaped = destination.replace('\'', "''");
    sqlx::query(&format!("VACUUM INTO '{escaped}'"))
        .execute(connection)
        .await?;
    Ok(())
}

/// Reclaims free pages. SQLite refuses it inside a transaction.
pub async fn vacuum(connection: &mut SqliteConnection) -> Result<()> {
    sqlx::query("VACUUM").execute(connection).await?;
    Ok(())
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
    let names: Vec<String> =
        sqlx::query_scalar(&format!("SELECT name FROM pragma_table_info('{escaped}')"))
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
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM \"{escaped}\""))
        .fetch_one(connection)
        .await
}

pub async fn clear_table(connection: &mut SqliteConnection, table: &str) -> Result<()> {
    let escaped = table.replace('"', "\"\"");
    sqlx::query(&format!("DELETE FROM \"{escaped}\""))
        .execute(connection)
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

pub async fn rebuild_fts5_index(connection: &mut SqliteConnection, table: &str) -> Result<()> {
    let escaped = table.replace('"', "\"\"");
    sqlx::query(&format!(
        "INSERT INTO \"{escaped}\"(\"{escaped}\") VALUES('rebuild')"
    ))
    .execute(connection)
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
