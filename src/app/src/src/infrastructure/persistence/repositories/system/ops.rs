use crate::error::AppError;
use sqlx::SqliteConnection;
use tracing::{debug, error, info};

pub async fn vacuum(conn: &mut SqliteConnection) -> Result<(), AppError> {
    sqlx::query("VACUUM").execute(conn).await.map_err(|e| {
        error!(error = %e, "Failed to vacuum database");
        AppError::Database(format!("Failed to vacuum: {}", e))
    })?;

    info!("Database vacuumed successfully");
    Ok(())
}

pub async fn analyze(conn: &mut SqliteConnection) -> Result<(), AppError> {
    sqlx::query("ANALYZE").execute(conn).await.map_err(|e| {
        error!(error = %e, "Failed to analyze database");
        AppError::Database(format!("Failed to analyze: {}", e))
    })?;

    info!("Database analyzed successfully");
    Ok(())
}

pub async fn get_database_size(conn: &mut SqliteConnection) -> Result<i64, AppError> {
    let result: (i64,) = sqlx::query_as(
        "SELECT page_count * page_size as size FROM pragma_page_count(), pragma_page_size()",
    )
    .fetch_one(conn)
    .await
    .map_err(|e| {
        error!(error = %e, "Failed to get database size");
        AppError::Database(format!("Failed to get database size: {}", e))
    })?;

    debug!(size = result.0, "Retrieved database size");
    Ok(result.0)
}

pub async fn integrity_check(conn: &mut SqliteConnection) -> Result<bool, AppError> {
    let result: (String,) = sqlx::query_as("PRAGMA integrity_check")
        .fetch_one(conn)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to check database integrity");
            AppError::Database(format!("Failed to check integrity: {}", e))
        })?;

    let is_ok = result.0 == "ok";

    if is_ok {
        debug!("Database integrity check passed");
    } else {
        error!(result = ?result.0, "Database integrity check failed");
    }

    Ok(is_ok)
}
