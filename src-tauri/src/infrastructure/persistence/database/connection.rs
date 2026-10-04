use crate::shared::error::{AppError, Result, ResultExt};
use crate::shared::resilience::{retry_with_backoff, RetryConfig};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct DatabaseConnection {
    pool: SqlitePool,
    /// Where a corrupt database was moved before a fresh one was created, when
    /// that happened on this open. The user otherwise sees only an empty
    /// library; this is what lets the app say why.
    recovered_from: Option<PathBuf>,
}

/// Tracing target of the corruption-recovery event, stable so the log viewer
/// and support can find it.
pub const DB_RECOVERY_TARGET: &str = "lattice::db_recovery";

/// Rename `db_path` to `backup_path`, and its `-wal` and `-shm` with it.
///
/// Left behind, the old WAL would sit next to the fresh database under the
/// name SQLite looks for, and its frames would be replayed into the new file,
/// bringing the corrupt pages back or corrupting it. Kept beside the moved
/// file under matching names, they still open together for a salvage attempt.
fn move_database_aside(db_path: &Path, backup_path: &Path) -> std::io::Result<()> {
    std::fs::rename(db_path, backup_path)?;
    for suffix in ["-wal", "-shm"] {
        let from = sidecar_path(db_path, suffix);
        let to = sidecar_path(backup_path, suffix);
        match std::fs::rename(&from, &to) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// `lattice.db` + `-wal` → `lattice.db-wal`, SQLite's own naming.
fn sidecar_path(db_path: &Path, suffix: &str) -> PathBuf {
    let mut name = db_path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

impl DatabaseConnection {
    pub async fn new(db_path: PathBuf) -> Result<Self> {
        // Connect with corruption detection and automatic recovery.
        let (pool, recovered_from) = Self::create_pool_with_corruption_recovery(&db_path).await?;

        let fk_enabled: (i32,) = sqlx::query_as("PRAGMA foreign_keys")
            .fetch_one(&pool)
            .await
            .context("Failed to check foreign key status")?;

        if fk_enabled.0 != 1 {
            return Err(AppError::Database(
                "Failed to enable foreign key constraints".to_string(),
            ));
        }

        Ok(Self {
            pool,
            recovered_from,
        })
    }

    /// The corrupt database moved aside on this open, if any.
    pub fn recovered_from(&self) -> Option<&Path> {
        self.recovered_from.as_deref()
    }

    /// Create SQLite pool with automatic corruption recovery
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
    async fn create_pool_with_corruption_recovery(
        db_path: &PathBuf,
    ) -> Result<(SqlitePool, Option<PathBuf>)> {
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
            Ok(pool) => Ok((pool, None)),
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

                    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
                    let backup_path =
                        db_path.with_extension(format!("corrupted-{}.bak", timestamp));

                    // Move corrupted database to backup
                    match move_database_aside(db_path, &backup_path) {
                        Ok(()) => {
                            tracing::warn!(
                                target: DB_RECOVERY_TARGET,
                                moved_to = %backup_path.display(),
                                error = %error_msg,
                                "Corrupt database moved aside; starting with an empty one"
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
                                .map(|pool| (pool, Some(backup_path)))
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

    pub async fn begin_immediate(&self) -> Result<sqlx::Transaction<'_, sqlx::Sqlite>> {
        tracing::info!("→ begin_immediate: Starting transaction acquisition...");
        let pool = self.pool.clone();
        let result = retry_with_backoff(
            RetryConfig::default(),
            || async {
                tracing::info!("→ begin_immediate: Calling pool.begin_with()...");
                let tx = pool.begin_with("BEGIN IMMEDIATE").await?;
                tracing::info!("✓ begin_immediate: Transaction acquired successfully");
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
                let size_result: (i64, i64) = sqlx::query_as(
                    "SELECT page_count, page_size FROM pragma_page_count(), pragma_page_size()",
                )
                .fetch_one(&pool)
                .await?;

                let db_size = size_result.0 * size_result.1;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stale_wal_and_shm_move_with_the_corrupt_database() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("lattice.db");
        let backup = dir.path().join("lattice.corrupted-20260925_000000.bak");
        std::fs::write(&db, b"not a database").unwrap();
        std::fs::write(sidecar_path(&db, "-wal"), b"stale frames").unwrap();
        std::fs::write(sidecar_path(&db, "-shm"), b"stale index").unwrap();

        move_database_aside(&db, &backup).unwrap();

        assert!(!db.exists());
        assert!(
            !sidecar_path(&db, "-wal").exists(),
            "stale WAL left beside the new DB"
        );
        assert!(!sidecar_path(&db, "-shm").exists());
        assert_eq!(std::fs::read(&backup).unwrap(), b"not a database");
        assert_eq!(
            std::fs::read(sidecar_path(&backup, "-wal")).unwrap(),
            b"stale frames"
        );
        assert_eq!(
            std::fs::read(sidecar_path(&backup, "-shm")).unwrap(),
            b"stale index"
        );
    }

    #[test]
    fn a_database_without_a_wal_moves_alone() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("lattice.db");
        let backup = dir.path().join("lattice.bak");
        std::fs::write(&db, b"x").unwrap();

        move_database_aside(&db, &backup).unwrap();
        assert!(backup.exists());
    }

    #[tokio::test]
    async fn memory_attributes_are_found_by_space_through_an_index() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        let plan: Vec<(i64, i64, i64, String)> = sqlx::query_as(
            "EXPLAIN QUERY PLAN SELECT item_id FROM conversation_memory_attributes WHERE space_id = ?",
        )
        .bind("space-1")
        .fetch_all(&pool)
        .await
        .unwrap();
        let detail: Vec<&str> = plan.iter().map(|row| row.3.as_str()).collect();
        assert!(
            detail
                .iter()
                .any(|d| d.contains("idx_memory_attributes_space")),
            "{detail:?}"
        );
    }

    #[tokio::test]
    async fn opening_a_corrupt_file_records_where_it_was_moved() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("lattice.db");
        std::fs::write(&db, vec![0x42u8; 8192]).unwrap();
        std::fs::write(sidecar_path(&db, "-wal"), b"stale frames").unwrap();

        let connection = DatabaseConnection::new(db.clone()).await.unwrap();

        let moved = connection
            .recovered_from()
            .expect("recovery recorded")
            .to_path_buf();
        assert!(moved.exists());
        // The move of `-wal`/`-shm` alongside is covered above; SQLite may
        // already have discarded a WAL it could not read when the failed open
        // closed. What matters here is that nothing was replayed into the
        // fresh file.
        let fresh: (i32,) = sqlx::query_as("PRAGMA schema_version")
            .fetch_one(connection.pool())
            .await
            .unwrap();
        assert_eq!(
            fresh.0, 0,
            "the database at the original path is a new, empty one"
        );
        connection.close().await.unwrap();
    }
}
