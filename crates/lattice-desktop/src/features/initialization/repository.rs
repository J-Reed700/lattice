use lattice_core::shared::error::{AppError, Result};
use sqlx::SqlitePool;

pub async fn has_schema_version_table(pool: &SqlitePool) -> Result<bool> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'schema_version'",
    )
    .fetch_one(pool)
    .await
    .map_err(|e| AppError::Database(format!("Failed to inspect database schema: {e}")))?;
    Ok(count > 0)
}

pub async fn schema_version(pool: &SqlitePool) -> Result<i64> {
    sqlx::query_scalar::<_, i64>("SELECT COALESCE(MAX(version), 0) FROM schema_version")
        .fetch_one(pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to query schema version: {}", e)))
}
