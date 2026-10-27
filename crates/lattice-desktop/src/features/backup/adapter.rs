//! Backup Adapter Implementation
//!
//! Implements BackupPort for SQLite database backup and restore.
//!
//! # Features
//! - Create backups using SQLite's backup API
//! - Restore from backup files
//! - List backups in a directory
//! - Include metadata (version, file count, size)
//!
//! # Safety
//! - Uses online backup (database remains available during backup)
//! - Validates backup integrity before restore
//! - Creates timestamped backup files

use async_trait::async_trait;
use chrono::Utc;
use lattice_core::application::ports::backup_port::{BackupInfoData, BackupPort};
use lattice_core::shared::error::{AppError, Result};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::SqlitePool;
#[cfg(test)]
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{error, info, warn};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Best-effort cleanup for a sibling snapshot after success or failure.
struct TemporarySnapshot(PathBuf);

impl Drop for TemporarySnapshot {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Validate that a filename is safe (no directory traversal patterns).
///
/// This function checks for directory traversal patterns including:
/// - Literal `..` sequences
/// - URL-encoded patterns like `%2e%2e`, `%2f`
/// - Absolute paths (starts with `/` or `\`)
fn validate_safe_filename(filename: &str) -> Result<()> {
    if filename.contains("..") {
        return Err(AppError::Security(
            "Filename contains directory traversal (..)".to_string(),
        ));
    }

    let filename_lower = filename.to_lowercase();
    if filename_lower.contains("%2e")
        || filename_lower.contains("%2f")
        || filename_lower.contains("%5c")
    {
        return Err(AppError::Security(
            "Filename contains URL-encoded directory traversal".to_string(),
        ));
    }

    if filename.starts_with('/') || filename.starts_with('\\') {
        return Err(AppError::Security(
            "Filename cannot be an absolute path".to_string(),
        ));
    }

    if filename.contains('\0') {
        return Err(AppError::Security(
            "Filename contains null byte".to_string(),
        ));
    }

    Ok(())
}

/// Validate that a path is safe for use in SQL queries.
///
/// This function provides defense-in-depth protection against SQL injection
/// in VACUUM INTO commands, which cannot use parameterized queries.
///
/// # Security
///
/// Checks for SQL injection patterns:
/// - Semicolons (statement terminators)
/// - Single quotes (string delimiters)
/// - Double quotes (identifier delimiters)
/// - SQL keywords (DROP, DELETE, INSERT, etc.)
/// - Comment sequences (-- and /* */)
/// - Null bytes
fn validate_sql_safe_path(path: &str) -> Result<()> {
    // SQL keywords are valid in paths and are not rejected here.
    // that would block legitimate paths like "Dropbox", "update_v2", etc.
    // Security relies on:
    // 1. Path canonicalization + starts_with() check (confines to backup_root)
    // 2. Single-quote escaping below (replace("'", "''"))
    // We keep basic checks for obvious SQL injection attempts:

    if path.contains(';') {
        return Err(AppError::Security(
            "Path contains SQL statement terminator (;)".to_string(),
        ));
    }

    if path.contains("--") {
        return Err(AppError::Security(
            "Path contains SQL comment sequence (--)".to_string(),
        ));
    }

    if path.contains("/*") || path.contains("*/") {
        return Err(AppError::Security(
            "Path contains SQL comment sequence (/* */)".to_string(),
        ));
    }

    if path.contains('\0') {
        return Err(AppError::Security("Path contains null byte".to_string()));
    }

    Ok(())
}

/// Connection options for opening an existing backup file read-only, built
/// from `SqliteConnectOptions` rather than a `sqlite://…?mode=ro` DSN: a
/// canonical Windows path (`\\?\C:\...`) contains a `?`, where sqlx splits a
/// DSN. The `Path` goes to sqlx as is, with no quoting or escaping involved.
fn read_only_backup_options(path: &Path) -> SqliteConnectOptions {
    SqliteConnectOptions::new().filename(path).read_only(true)
}

/// Put the pre-restore safety copy back as the live database. Returns what to
/// tell the user when that fails, including the path to inspect for recovery.
pub(crate) async fn roll_back_database(safety: &Path, db_path: &Path) -> Option<String> {
    // repository-barrier-allow: rolling back the live database file.
    match tokio::fs::rename(safety, db_path).await {
        Ok(()) => {
            info!("Rolled back to original database");
            None
        }
        Err(e) => {
            error!(
                safety_copy = %safety.display(),
                error = %e,
                "Could not roll back to the original database"
            );
            Some(format!(
                " Your original database could not be put back automatically. Check the safety copy at {} before retrying.",
                safety.display()
            ))
        }
    }
}

/// Backup adapter using SQLite backup API.
pub struct BackupAdapter {
    pool: SqlitePool,
    db_path: PathBuf,
}

impl BackupAdapter {
    /// `VACUUM INTO` the live database at `destination`, replacing an older
    /// safety copy. `VACUUM INTO` refuses an existing file, and a stale copy
    /// from an earlier restore is worth less than a current one.
    async fn write_safety_copy(&self, destination: &Path) -> Result<()> {
        // Build and verify the new copy before touching the last recovery
        // point. A failed VACUUM (disk full, locked/corrupt DB, etc.) must not
        // destroy the previous safety copy.
        let destination_str = destination.to_string_lossy().to_string();
        validate_sql_safe_path(&destination_str)?;
        let file_name = destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("database.db");
        let temp_path =
            destination.with_file_name(format!(".{file_name}.{}.tmp", uuid::Uuid::new_v4()));
        let _temporary_snapshot = TemporarySnapshot(temp_path.clone());
        let temp_str = temp_path.to_string_lossy().to_string();
        validate_sql_safe_path(&temp_str)?;
        let escaped = temp_str.replace('\'', "''");
        if let Err(error) = sqlx::query(&format!("VACUUM INTO '{escaped}'"))
            .execute(&self.pool)
            .await
        {
            error!("Failed to create safety backup: {}", error);
            return Err(AppError::Database(format!(
                "Failed to create safety backup: {error}"
            )));
        }

        self.validate_backup(&temp_path).await?;

        // The temporary lives beside the destination, so replacement is one
        // same-filesystem rename. If it fails, the original recovery copy is
        // still at its canonical path.
        tokio::fs::rename(&temp_path, destination)
            .await
            .map_err(|error| {
                AppError::FileSystem(format!("Failed to install the new safety backup: {error}"))
            })?;
        Ok(())
    }

    pub fn new(pool: SqlitePool, db_path: PathBuf) -> Self {
        Self { pool, db_path }
    }

    /// Generate timestamped backup filename
    fn generate_backup_filename(&self) -> String {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        format!("recall_backup_{}.db", timestamp)
    }

    /// Get metadata about a backup file
    async fn get_backup_info(&self, path: &PathBuf) -> Result<BackupInfoData> {
        let metadata = tokio::fs::metadata(path)
            .await
            .map_err(|e| AppError::FileSystem(format!("Failed to read backup metadata: {}", e)))?;

        let created_at = metadata
            .created()
            .or_else(|_| metadata.modified())
            .map(|time| {
                let datetime: chrono::DateTime<Utc> = time.into();
                datetime.to_rfc3339()
            })
            .unwrap_or_else(|_| Utc::now().to_rfc3339());

        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        // Try to open backup file and get file count
        let file_count = self.get_backup_file_count(path).await.unwrap_or(0);

        let version = self
            .get_backup_version(path)
            .await
            .unwrap_or_else(|| APP_VERSION.to_string());

        Ok(BackupInfoData {
            path: path.to_string_lossy().to_string(),
            name,
            created_at,
            version,
            file_count,
            size: metadata.len(),
        })
    }

    /// Get file count from backup database
    async fn get_backup_file_count(&self, path: &Path) -> Option<usize> {
        // Open backup database read-only
        let pool = SqlitePool::connect_with(read_only_backup_options(path))
            .await
            .ok()?;

        let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
            .fetch_one(&pool)
            .await
            .ok()?;

        pool.close().await;

        Some(result.0 as usize)
    }

    /// Get version from backup metadata
    async fn get_backup_version(&self, path: &Path) -> Option<String> {
        let pool = match SqlitePool::connect_with(read_only_backup_options(path)).await {
            Ok(pool) => pool,
            Err(error) => {
                warn!(
                    "Failed to open backup database for version lookup ({}): {}",
                    path.display(),
                    error
                );
                return None;
            }
        };

        let table_exists: i64 = match sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='schema_version'",
        )
        .fetch_one(&pool)
        .await
        {
            Ok(count) => count,
            Err(error) => {
                warn!(
                    "Failed to check schema_version table in backup ({}): {}",
                    path.display(),
                    error
                );
                pool.close().await;
                return None;
            }
        };

        if table_exists == 0 {
            pool.close().await;
            return None;
        }

        let version: Option<i64> = match sqlx::query_scalar(
            "SELECT version FROM schema_version ORDER BY version DESC LIMIT 1",
        )
        .fetch_optional(&pool)
        .await
        {
            Ok(value) => value,
            Err(error) => {
                warn!(
                    "Failed to read backup schema version ({}): {}",
                    path.display(),
                    error
                );
                pool.close().await;
                return None;
            }
        };

        if version.is_none() {
            warn!(
                "schema_version table is empty in backup ({}).",
                path.display()
            );
        }

        pool.close().await;

        version.map(|value| value.to_string())
    }

    /// Validate backup file integrity
    async fn validate_backup(&self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Err(AppError::NotFound(format!(
                "Backup file not found: {}",
                path.display()
            )));
        }

        // Try to open the backup database
        let pool = SqlitePool::connect_with(read_only_backup_options(path))
            .await
            .map_err(|e| AppError::Database(format!("Invalid backup file: {}", e)))?;

        let result: (String,) = sqlx::query_as("PRAGMA integrity_check")
            .fetch_one(&pool)
            .await
            .map_err(|e| AppError::Database(format!("Backup integrity check failed: {}", e)))?;

        pool.close().await;

        if result.0 != "ok" {
            return Err(AppError::Database(format!(
                "Backup integrity check failed: {}",
                result.0
            )));
        }

        Ok(())
    }
}

#[async_trait]
impl BackupPort for BackupAdapter {
    async fn create_backup(&self, path: Option<PathBuf>) -> Result<String, AppError> {
        // SECURITY FIX (CWE-22): Establish allowed backup root directory
        let backup_root = self
            .db_path
            .parent()
            .ok_or_else(|| AppError::FileSystem("Invalid database path".to_string()))?
            .join("backups");

        tokio::fs::create_dir_all(&backup_root).await.map_err(|e| {
            AppError::FileSystem(format!("Failed to create backup directory: {}", e))
        })?;

        // Canonicalize the backup root to get absolute path
        let canonical_backup_root = backup_root.canonicalize().map_err(|e| {
            AppError::FileSystem(format!("Failed to canonicalize backup directory: {}", e))
        })?;

        // Determine and validate the backup path
        let backup_path = match path {
            Some(p) => {
                // SECURITY: Early validation of path string for SQL injection patterns
                // This must happen BEFORE any file system operations to catch malicious paths
                let path_str = p.to_string_lossy();
                validate_sql_safe_path(&path_str)?;

                // SECURITY: Validate user-provided path to prevent directory traversal
                // SQL-safe path validation: ensure path is under backup root
                let canonical_path = if p.exists() {
                    p.canonicalize().map_err(|e| {
                        error!("Failed to canonicalize backup path: {}", e);
                        AppError::InvalidInput(format!("Invalid backup path: {}", e))
                    })?
                } else {
                    // For non-existent paths, canonicalize parent and append filename
                    let parent = p
                        .parent()
                        .ok_or_else(|| AppError::InvalidInput("Invalid backup path".to_string()))?;
                    let filename = p.file_name().ok_or_else(|| {
                        AppError::InvalidInput("Invalid backup filename".to_string())
                    })?;

                    // SECURITY: Validate filename for directory traversal patterns
                    let filename_str = filename.to_str().ok_or_else(|| {
                        AppError::InvalidInput("Invalid filename encoding".to_string())
                    })?;
                    validate_safe_filename(filename_str)?;

                    parent
                        .canonicalize()
                        .map_err(|e| {
                            error!("Failed to canonicalize backup parent: {}", e);
                            AppError::InvalidInput(format!("Invalid backup path: {}", e))
                        })?
                        .join(filename)
                };

                // Confinement to the backup root is deliberate and load-bearing:
                // `plugin_create_backup` is a webview-callable Tauri command, so
                // without this a hostile frontend could write a file anywhere the
                // user can write. Do not relax this to "honour the user's chosen
                // path" without first splitting trusted (scheduler, reading
                // validated settings) from untrusted (IPC) callers — see SET-4.
                // The corresponding half of SET-4 is fixed in settings
                // validation, which now rejects an out-of-root `backupPath` at
                // save time so the user sees an error immediately, instead of
                // every scheduled backup failing silently forever.
                if !canonical_path.starts_with(&canonical_backup_root) {
                    error!(
                        "Backup path outside allowed directory: {}",
                        canonical_path.display()
                    );
                    return Err(AppError::PermissionDenied(format!(
                        "Backup path outside allowed directory: {}",
                        canonical_path.display()
                    )));
                }

                // If validated path is a directory, append generated filename
                if canonical_path.is_dir() {
                    canonical_path.join(self.generate_backup_filename())
                } else {
                    canonical_path
                }
            }
            None => canonical_backup_root.join(self.generate_backup_filename()),
        };

        info!("Creating backup at: {}", backup_path.display());

        let backup_path_str = backup_path.to_string_lossy().to_string();

        // SECURITY: Validate path for SQL safety (CWE-89 mitigation)
        validate_sql_safe_path(&backup_path_str)?;

        // SECURITY: Escape single quotes to prevent SQL injection
        // Note: VACUUM INTO doesn't support parameterized queries
        // Defense in depth: Path is validated above AND escaped here
        let escaped_path = backup_path_str.replace("'", "''");

        sqlx::query(&format!("VACUUM INTO '{}'", escaped_path))
            .execute(&self.pool)
            .await
            .map_err(|e| {
                error!("Backup failed: {}", e);
                AppError::Database(format!("Failed to create backup: {}", e))
            })?;

        if !backup_path.exists() {
            return Err(AppError::FileSystem(
                "Backup file was not created".to_string(),
            ));
        }

        info!("Backup created successfully: {}", backup_path.display());

        Ok(backup_path.to_string_lossy().to_string())
    }

    async fn restore_backup(&self, path: PathBuf) -> Result<(), AppError> {
        info!("Starting backup restore from: {}", path.display());

        // 1. Validate backup before restore
        self.validate_backup(&path).await?;

        // 2. Create safety backup of current database. Through the open pool,
        // not a file copy: the database runs in WAL mode, and a copy of the
        // main file alone leaves out every committed transaction not yet
        // checkpointed — the most recent chats and imports.
        let current_backup = self.db_path.with_extension("db.pre_restore");
        if self.db_path.exists() {
            self.write_safety_copy(&current_backup).await?;
            info!("Created safety backup at: {}", current_backup.display());
        }

        // 3. Close all database connections
        info!("Closing database connections...");
        self.pool.close().await;

        // 4. Replace database file atomically
        // Use temp file + rename for atomicity
        let temp_path = self.db_path.with_extension("db.restoring");

        match tokio::fs::copy(&path, &temp_path).await {
            Ok(_) => {
                // Atomic rename
                match tokio::fs::rename(&temp_path, &self.db_path).await {
                    Ok(_) => {
                        info!("Database restored successfully");
                        info!("Old database backed up to: {}", current_backup.display());
                        warn!("Application restart required to reconnect to restored database");
                        Ok(())
                    }
                    Err(e) => {
                        // Rollback: restore from safety backup
                        error!("Failed to rename restored database: {}", e);
                        let mut note = String::new();
                        if current_backup.exists() {
                            note = roll_back_database(&current_backup, &self.db_path)
                                .await
                                .unwrap_or_default();
                        }
                        let _ = tokio::fs::remove_file(&temp_path).await;
                        Err(AppError::FileSystem(format!(
                            "Failed to restore database: {}.{}",
                            e, note
                        )))
                    }
                }
            }
            Err(e) => {
                error!("Failed to copy backup file: {}", e);
                // Rollback: restore from safety backup if needed
                let mut note = String::new();
                if current_backup.exists() && !self.db_path.exists() {
                    note = roll_back_database(&current_backup, &self.db_path)
                        .await
                        .unwrap_or_default();
                }
                Err(AppError::FileSystem(format!(
                    "Failed to copy backup: {}.{}",
                    e, note
                )))
            }
        }
    }

    async fn list_backups(&self, data_dir: PathBuf) -> Result<Vec<BackupInfoData>, AppError> {
        let backup_dir = data_dir.join("backups");

        if !backup_dir.exists() {
            return Ok(Vec::new());
        }

        let mut entries = tokio::fs::read_dir(&backup_dir)
            .await
            .map_err(|e| AppError::FileSystem(format!("Failed to read backup directory: {}", e)))?;

        let mut backups = Vec::new();

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AppError::FileSystem(format!("Failed to read directory entry: {}", e)))?
        {
            let path = entry.path();

            // Only include .db files
            if path.extension().and_then(|s| s.to_str()) == Some("db") {
                if let Ok(info) = self.get_backup_info(&path).await {
                    backups.push(info);
                }
            }
        }

        backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(backups)
    }
}

#[cfg(test)]
#[path = "adapter_tests.rs"]
mod tests;
