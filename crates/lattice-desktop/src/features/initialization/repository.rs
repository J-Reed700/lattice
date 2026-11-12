use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;

pub async fn has_schema_version_table(db: &Database) -> Result<bool> {
    let mut read = db.begin_read().await?;
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'schema_version'",
    )
    .fetch_one(&mut *read)
    .await
    .map_err(|e| AppError::Database(format!("Failed to inspect database schema: {e}")))?;
    read.commit().await?;
    Ok(count > 0)
}

pub async fn schema_version(db: &Database) -> Result<i64> {
    let mut read = db.begin_read().await?;
    let version =
        sqlx::query_scalar::<_, i64>("SELECT COALESCE(MAX(version), 0) FROM schema_version")
            .fetch_one(&mut *read)
            .await
            .map_err(|e| AppError::Database(format!("Failed to query schema version: {}", e)))?;
    read.commit().await?;
    Ok(version)
}
