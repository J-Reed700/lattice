use crate::shared::error::{AppError, Result, ResultExt};
use crate::shared::utils::{retry_with_backoff, RetryConfig};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::future::Future;
use std::path::PathBuf;
use std::time::Duration;

pub struct DatabaseConnection {
    pool: SqlitePool,
}

impl DatabaseConnection {
    pub async fn new(db_path: PathBuf) -> Result<Self> {
        // Issue #3: Add connection timeout (P0 - Prevents Hangs)
        // Issue #2: Enable foreign key enforcement (P0 - Data Integrity)
        // TITANIUM SHIELD: Corruption Recovery (Oracle-mandated)

        // Attempt connection, with corruption detection and auto-recovery
        let pool_result = Self::create_pool_with_corruption_recovery(&db_path).await;

        let pool = match pool_result {
            Ok(p) => p,
            Err(e) => {
                // If still failing after recovery attempt, propagate error
                return Err(e);
            }
        };

        // Verify foreign keys are actually enabled
        let fk_enabled: (i32,) = sqlx::query_as("PRAGMA foreign_keys")
            .fetch_one(&pool)
            .await
            .context("Failed to check foreign key status")?;

        if fk_enabled.0 != 1 {
            return Err(AppError::Database(
                "Failed to enable foreign key constraints".to_string(),
            ));
        }

        Ok(Self { pool })
    }

    /// Create SQLite pool with automatic corruption recovery
    ///
    /// Oracle Mandate (Titanium Shield Audit):
    /// "Catch SQLITE_CORRUPT and move bad DB to .bak instead of crash loop"
    ///
    /// Recovery Strategy:
    /// 1. Attempt to connect to database
    /// 2. If connection fails due to corruption:
    ///    - Rename lattice.db to lattice.db.corrupted-{timestamp}.bak
    ///    - Retry connection (creates fresh DB via create_if_missing)
    /// 3. If corruption detected during pragma check:
    ///    - Same recovery procedure
    ///
    /// This prevents the "hundreds of errors" scenario where a corrupted
    /// database causes infinite boot loops.
    async fn create_pool_with_corruption_recovery(db_path: &PathBuf) -> Result<SqlitePool> {
        let connect_options = SqliteConnectOptions::new()
            .filename(db_path)
            .create_if_missing(true)
            .busy_timeout(Duration::from_secs(5)) // SQLite busy timeout
            .pragma("foreign_keys", "ON") // Enable foreign keys
            .pragma("journal_mode", "WAL") // Write-ahead logging for better concurrency
            .pragma("synchronous", "NORMAL") // Balance between safety and speed
            .pragma("cache_size", "-20000") // 20MB cache
            .pragma("temp_store", "MEMORY"); // Use memory for temp tables

        let pool_result = SqlitePoolOptions::new()
            .max_connections(5)
            .min_connections(1)
            .acquire_timeout(Duration::from_secs(5)) // Don't wait forever for connection
            .idle_timeout(Duration::from_secs(600)) // Keep connections alive for 10 minutes
            .max_lifetime(Duration::from_secs(1800)) // Recycle connections after 30 minutes
            .connect_with(connect_options.clone())
            .await;

        match pool_result {
            Ok(pool) => Ok(pool),
            Err(e) => {
                let error_msg = e.to_string();

                // Detect corruption errors
                let is_corrupt = error_msg.contains("corrupt")
                    || error_msg.contains("database disk image is malformed")
                    || error_msg.contains("SQLITE_CORRUPT")
                    || error_msg.contains("file is not a database");

                if is_corrupt && db_path.exists() {
                    tracing::error!("Database corruption detected: {}", error_msg);
                    tracing::warn!("Attempting automatic recovery...");

                    // Generate backup filename with timestamp
                    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
                    let backup_path =
                        db_path.with_extension(format!("corrupted-{}.bak", timestamp));

                    // Move corrupted database to backup
                    match std::fs::rename(db_path, &backup_path) {
                        Ok(_) => {
                            tracing::info!(
                                "Corrupted database moved to: {}",
                                backup_path.display()
                            );
                            tracing::info!("Creating fresh database at: {}", db_path.display());

                            // Retry connection (will create fresh database)
                            SqlitePoolOptions::new()
                                .max_connections(5)
                                .min_connections(1)
                                .acquire_timeout(Duration::from_secs(5))
                                .idle_timeout(Duration::from_secs(600))
                                .max_lifetime(Duration::from_secs(1800))
                                .connect_with(connect_options)
                                .await
                                .context(
                                    "Failed to create fresh database after corruption recovery",
                                )
                        }
                        Err(rename_err) => {
                            tracing::error!("Failed to move corrupted database: {}", rename_err);
                            Err(AppError::Database(format!(
                                "Database is corrupted and automatic recovery failed. \
                                 Please manually delete {} and restart the application. \
                                 Original error: {}",
                                db_path.display(),
                                error_msg
                            )))
                        }
                    }
                } else {
                    // Not a corruption error, propagate original error
                    Err(AppError::Database(error_msg))
                }
            }
        }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn close(self) -> Result<()> {
        self.pool.close().await;
        Ok(())
    }

    // Issue #5: Fix Transaction Isolation (P0 - Race Conditions)
    // Retry on database locked errors (SQLITE_BUSY, SQLITE_LOCKED)
    pub async fn begin_immediate(&self) -> Result<sqlx::Transaction<'_, sqlx::Sqlite>> {
        tracing::info!("→ begin_immediate: Starting transaction acquisition...");
        let pool = self.pool.clone();
        let result = retry_with_backoff(
            RetryConfig::default(),
            || async {
                tracing::info!("→ begin_immediate: Calling pool.begin()...");
                let mut tx = pool.begin().await?;
                tracing::info!(
                    "→ begin_immediate: Swapping to IMMEDIATE mode (ROLLBACK; BEGIN IMMEDIATE)..."
                );
                sqlx::query("ROLLBACK; BEGIN IMMEDIATE")
                    .execute(&mut *tx)
                    .await?;
                tracing::info!("✓ begin_immediate: Swapped to IMMEDIATE mode successfully");
                Ok(tx)
            },
            |e: &AppError| {
                // Retry on database locked errors
                tracing::warn!("begin_immediate: Database locked, will retry: {}", e);
                matches!(e, AppError::Database(msg) if
                    msg.contains("database is locked") ||
                    msg.contains("SQLITE_BUSY") ||
                    msg.contains("SQLITE_LOCKED")
                )
            },
        )
        .await;

        match &result {
            Ok(_) => tracing::info!("✓ begin_immediate: Transaction acquired successfully"),
            Err(e) => tracing::error!("✗ begin_immediate: Failed to acquire transaction: {}", e),
        }

        result
    }

    // Issue #7: Add VACUUM Strategy (P0 - Disk Bloat)
    // Retry on transient database errors
    pub async fn vacuum_if_needed(&self, threshold_bytes: i64) -> Result<bool> {
        let pool = self.pool.clone();
        retry_with_backoff(
            RetryConfig::conservative(), // Less aggressive for maintenance tasks
            || async {
                // Check database size
                let size_result: (i64, i64) = sqlx::query_as(
                    "SELECT page_count, page_size FROM pragma_page_count(), pragma_page_size()",
                )
                .fetch_one(&pool)
                .await?;

                let db_size = size_result.0 * size_result.1;

                // Check free pages ratio
                let freelist: (i64,) =
                    sqlx::query_as("SELECT freelist_count FROM pragma_freelist_count()")
                        .fetch_one(&pool)
                        .await?;

                let free_ratio = freelist.0 as f64 / size_result.0 as f64;

                // Vacuum if database is over threshold and more than 20% fragmented
                if db_size > threshold_bytes && free_ratio > 0.2 {
                    tracing::info!(
                        "Vacuuming database: size={} bytes, fragmentation={:.1}%",
                        db_size,
                        free_ratio * 100.0
                    );
                    sqlx::query("VACUUM").execute(&pool).await?;
                    return Ok(true);
                }

                Ok(false)
            },
            |e: &AppError| {
                // Retry on database locked errors
                matches!(e, AppError::Database(msg) if
                    msg.contains("database is locked") ||
                    msg.contains("SQLITE_BUSY")
                )
            },
        )
        .await
    }

    // Helper for running maintenance tasks
    // Retry on transient database errors
    pub async fn run_maintenance(&self) -> Result<()> {
        let pool = self.pool.clone();
        retry_with_backoff(
            RetryConfig::conservative(),
            || async {
                // Run ANALYZE to update query planner statistics
                sqlx::query("ANALYZE").execute(&pool).await?;
                Ok(())
            },
            |e: &AppError| {
                matches!(e, AppError::Database(msg) if
                    msg.contains("database is locked") ||
                    msg.contains("SQLITE_BUSY")
                )
            },
        )
        .await?;

        // Vacuum if database is over 100MB
        self.vacuum_if_needed(100_000_000).await?;

        Ok(())
    }
}

// P1 Issue #3: Add query timeout wrapper to prevent queries from hanging indefinitely

// Default timeout durations for different operation types
use crate::shared::constants::{
    DB_QUERY_TIMEOUT_HEAVY, DB_QUERY_TIMEOUT_NORMAL, DB_QUERY_TIMEOUT_QUICK,
};

pub async fn query_with_timeout<T, F, Fut>(operation: F) -> Result<T>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, sqlx::Error>>,
{
    query_with_timeout_duration(operation, DB_QUERY_TIMEOUT_NORMAL).await
}

pub async fn query_with_quick_timeout<T, F, Fut>(operation: F) -> Result<T>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, sqlx::Error>>,
{
    query_with_timeout_duration(operation, DB_QUERY_TIMEOUT_QUICK).await
}

pub async fn query_with_heavy_timeout<T, F, Fut>(operation: F) -> Result<T>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, sqlx::Error>>,
{
    query_with_timeout_duration(operation, DB_QUERY_TIMEOUT_HEAVY).await
}

pub async fn query_with_timeout_duration<T, F, Fut>(operation: F, timeout: Duration) -> Result<T>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, sqlx::Error>>,
{
    let result = tokio::time::timeout(timeout, operation())
        .await
        .map_err(|_| {
            AppError::Database(format!(
                "Database query timed out after {} seconds",
                timeout.as_secs()
            ))
        })?;

    result.map_err(|e| AppError::Database(format!("Database query failed: {}", e)))
}
