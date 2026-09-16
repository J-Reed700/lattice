//! Database migration utilities for upgrading existing databases
//!
//! This module provides a simple wrapper around sqlx's migration functionality.
//! Migrations are automatically discovered from the migrations/ directory.

use crate::shared::error::{AppError, Result};
use sqlx::SqlitePool;

/// Run all pending migrations from the migrations/ directory
///
/// This function uses sqlx::migrate!() to automatically discover and run
/// migration files from the migrations/ directory at compile time.
///
/// # Arguments
///
/// * `pool` - SQLite connection pool to run migrations against
///
/// # Returns
///
/// `Ok(())` if all migrations complete successfully, or an error if any fail.
///
/// # Example
///
/// ```rust
/// use sqlx::SqlitePool;
/// use crate::infrastructure::persistence::database::migrate::run_migrations;
///
/// let pool = SqlitePool::connect("sqlite::memory:").await?;
/// run_migrations(&pool).await?;
/// ```
pub async fn run_migrations(pool: &SqlitePool) -> Result<()> {
    tracing::info!("Starting database migrations...");

    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(|error| {
            let error_text = error.to_string();
            tracing::error!("Migration failed: {}", error_text);
            AppError::Database(format!("Migration failed: {}", error_text))
        })?;

    tracing::info!("All migrations completed successfully");
    Ok(())
}
