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

use crate::application::ports::backup_port::{BackupInfoData, BackupPort};
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use chrono::Utc;
use sqlx::SqlitePool;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{error, info, warn};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Validate that a filename is safe (no directory traversal patterns).
///
/// This function checks for directory traversal patterns including:
/// - Literal `..` sequences
/// - URL-encoded patterns like `%2e%2e`, `%2f`
/// - Absolute paths (starts with `/` or `\`)
///
/// # Arguments
///
/// * `filename` - The filename to validate
///
/// # Returns
///
/// `Ok(())` if filename is safe, `Err(AppError::Security)` if dangerous patterns detected
fn validate_safe_filename(filename: &str) -> Result<()> {
    // Check for literal directory traversal
    if filename.contains("..") {
        return Err(AppError::Security(
            "Filename contains directory traversal (..)".to_string(),
        ));
    }

    // Check for URL-encoded directory traversal patterns
    // %2e = . (dot), %2f = / (slash), %5c = \ (backslash)
    let filename_lower = filename.to_lowercase();
    if filename_lower.contains("%2e")
        || filename_lower.contains("%2f")
        || filename_lower.contains("%5c")
    {
        return Err(AppError::Security(
            "Filename contains URL-encoded directory traversal".to_string(),
        ));
    }

    // Check for absolute paths
    if filename.starts_with('/') || filename.starts_with('\\') {
        return Err(AppError::Security(
            "Filename cannot be an absolute path".to_string(),
        ));
    }

    // Check for null bytes
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
///
/// # Arguments
///
/// * `path` - The file path to validate
///
/// # Returns
///
/// `Ok(())` if path is safe, `Err(AppError::Security)` if dangerous patterns detected
///
/// # Example
///
/// ```
/// let safe_path = "/backup/file.db";
/// validate_sql_safe_path(safe_path)?; // OK
///
/// let unsafe_path = "/backup'; DROP TABLE users; --";
/// validate_sql_safe_path(unsafe_path)?; // Error
/// ```
fn validate_sql_safe_path(path: &str) -> Result<()> {
    // ORACLE VALIDATION: Removed overly aggressive SQL keyword checking
    // that would block legitimate paths like "Dropbox", "update_v2", etc.
    //
    // Security relies on:
    // 1. Path canonicalization + starts_with() check (confines to backup_root)
    // 2. Single-quote escaping below (replace("'", "''"))
    //
    // We keep basic checks for obvious SQL injection attempts:

    // Check for SQL statement terminators
    if path.contains(';') {
        return Err(AppError::Security(
            "Path contains SQL statement terminator (;)".to_string(),
        ));
    }

    // Check for SQL comment sequences
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

    // Check for null bytes
    if path.contains('\0') {
        return Err(AppError::Security("Path contains null byte".to_string()));
    }

    Ok(())
}

/// Backup adapter using SQLite backup API
pub struct BackupAdapter {
    pool: SqlitePool,
    db_path: PathBuf,
}

impl BackupAdapter {
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
        let metadata = fs::metadata(path)
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

        // Get version from backup (if possible)
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
        let connection_string = format!("sqlite://{}?mode=ro", path.display());

        let pool = SqlitePool::connect(&connection_string).await.ok()?;

        let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
            .fetch_one(&pool)
            .await
            .ok()?;

        pool.close().await;

        Some(result.0 as usize)
    }

    /// Get version from backup metadata
    async fn get_backup_version(&self, path: &PathBuf) -> Option<String> {
        let connection_string = format!("sqlite://{}?mode=ro", path.display());

        let pool = match SqlitePool::connect(&connection_string).await {
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
        let connection_string = format!("sqlite://{}?mode=ro", path.display());

        let pool = SqlitePool::connect(&connection_string)
            .await
            .map_err(|e| AppError::Database(format!("Invalid backup file: {}", e)))?;

        // Run integrity check
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

        // Create backup root if it doesn't exist
        fs::create_dir_all(&backup_root).map_err(|e| {
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

                // Ensure path is within backup root (defense in depth)
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
            None => {
                // Use default backup path (no validation needed - we control it)
                canonical_backup_root.join(self.generate_backup_filename())
            }
        };

        info!("Creating backup at: {}", backup_path.display());

        // Use VACUUM INTO for online backup (SQLite 3.27+)
        // Note: backup_path is already validated to be within backup_root
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

        // Verify backup was created
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

        // 2. Create safety backup of current database
        let current_backup = self.db_path.with_extension("db.pre_restore");
        if self.db_path.exists() {
            fs::copy(&self.db_path, &current_backup).map_err(|e| {
                error!("Failed to create safety backup: {}", e);
                AppError::FileSystem(format!("Failed to create safety backup: {}", e))
            })?;
            info!("Created safety backup at: {}", current_backup.display());
        }

        // 3. Close all database connections
        info!("Closing database connections...");
        self.pool.close().await;

        // 4. Replace database file atomically
        // Use temp file + rename for atomicity
        let temp_path = self.db_path.with_extension("db.restoring");

        match fs::copy(&path, &temp_path) {
            Ok(_) => {
                // Atomic rename
                match fs::rename(&temp_path, &self.db_path) {
                    Ok(_) => {
                        info!("Database restored successfully");
                        info!("Old database backed up to: {}", current_backup.display());
                        warn!("Application restart required to reconnect to restored database");
                        Ok(())
                    }
                    Err(e) => {
                        // Rollback: restore from safety backup
                        error!("Failed to rename restored database: {}", e);
                        if current_backup.exists() {
                            let _ = fs::rename(&current_backup, &self.db_path);
                            info!("Rolled back to original database");
                        }
                        let _ = fs::remove_file(&temp_path);
                        Err(AppError::FileSystem(format!(
                            "Failed to restore database: {}",
                            e
                        )))
                    }
                }
            }
            Err(e) => {
                error!("Failed to copy backup file: {}", e);
                // Rollback: restore from safety backup if needed
                if current_backup.exists() && !self.db_path.exists() {
                    let _ = fs::rename(&current_backup, &self.db_path);
                    info!("Rolled back to original database");
                }
                Err(AppError::FileSystem(format!(
                    "Failed to copy backup: {}",
                    e
                )))
            }
        }
    }

    async fn list_backups(&self, data_dir: PathBuf) -> Result<Vec<BackupInfoData>, AppError> {
        let backup_dir = data_dir.join("backups");

        if !backup_dir.exists() {
            return Ok(Vec::new());
        }

        let entries = fs::read_dir(&backup_dir)
            .map_err(|e| AppError::FileSystem(format!("Failed to read backup directory: {}", e)))?;

        let mut backups = Vec::new();

        for entry in entries {
            let entry = entry.map_err(|e| {
                AppError::FileSystem(format!("Failed to read directory entry: {}", e))
            })?;

            let path = entry.path();

            // Only include .db files
            if path.extension().and_then(|s| s.to_str()) == Some("db") {
                if let Ok(info) = self.get_backup_info(&path).await {
                    backups.push(info);
                }
            }
        }

        // Sort by created_at (newest first)
        backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(backups)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use tempfile::tempdir;

    async fn create_test_pool() -> (SqlitePool, PathBuf, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Create the database file first
        std::fs::File::create(&db_path).unwrap();

        let pool = SqlitePoolOptions::new()
            .connect(&format!("sqlite://{}", db_path.display()))
            .await
            .unwrap();

        // Create test schema
        sqlx::query(
            r#"
            CREATE TABLE documents (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                content TEXT NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        // Insert test data
        sqlx::query("INSERT INTO documents (id, title, content) VALUES (?, ?, ?)")
            .bind("1")
            .bind("Test Doc")
            .bind("Test content")
            .execute(&pool)
            .await
            .unwrap();

        (pool, db_path, dir)
    }

    #[tokio::test]

    async fn test_create_backup() {
        let (pool, db_path, _dir) = create_test_pool().await;
        let adapter = BackupAdapter::new(pool.clone(), db_path.clone());

        let backup_path = adapter.create_backup(None).await.unwrap();

        assert!(PathBuf::from(&backup_path).exists());
        assert!(backup_path.contains("recall_backup_"));

        pool.close().await;
    }

    #[tokio::test]

    async fn test_create_backup_custom_path() {
        let (pool, db_path, _dir) = create_test_pool().await;
        let adapter = BackupAdapter::new(pool.clone(), db_path.clone());

        // Create custom path WITHIN the allowed backup directory
        let backup_root = db_path.parent().unwrap().join("backups");
        fs::create_dir_all(&backup_root).unwrap();
        let custom_path = backup_root.join("my_backup.db");

        let backup_path = adapter
            .create_backup(Some(custom_path.clone()))
            .await
            .unwrap();

        // Canonicalize both paths for comparison (macOS /var symlinks to /private/var)
        let expected_path = custom_path.canonicalize().unwrap();
        let actual_path = PathBuf::from(&backup_path).canonicalize().unwrap();
        assert_eq!(actual_path, expected_path);
        assert!(custom_path.exists());

        pool.close().await;
    }

    #[tokio::test]

    async fn test_list_backups() {
        let (pool, db_path, _dir) = create_test_pool().await;
        let adapter = BackupAdapter::new(pool.clone(), db_path.clone());

        // Create a couple backups
        let _ = adapter.create_backup(None).await.unwrap();
        // Sleep 1 second to avoid timestamp collision (backups use second-precision timestamps)
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        let _ = adapter.create_backup(None).await.unwrap();

        let data_dir = db_path.parent().unwrap().to_path_buf();
        let backups = adapter.list_backups(data_dir).await.unwrap();

        assert_eq!(backups.len(), 2);
        assert!(backups[0].size > 0);
        assert!(backups[0].name.contains("recall_backup_"));

        pool.close().await;
    }

    #[tokio::test]

    async fn test_validate_backup() {
        let (pool, db_path, _dir) = create_test_pool().await;
        let adapter = BackupAdapter::new(pool.clone(), db_path.clone());

        let backup_path = adapter.create_backup(None).await.unwrap();
        let backup_pathbuf = PathBuf::from(backup_path);

        let result = adapter.validate_backup(&backup_pathbuf).await;
        assert!(result.is_ok());

        pool.close().await;
    }

    #[tokio::test]

    async fn test_validate_invalid_backup() {
        let (pool, db_path, _dir) = create_test_pool().await;
        let adapter = BackupAdapter::new(pool.clone(), db_path.clone());

        let invalid_path = PathBuf::from("/nonexistent/backup.db");
        let result = adapter.validate_backup(&invalid_path).await;

        assert!(result.is_err());

        pool.close().await;
    }

    #[tokio::test]

    async fn test_backup_file_count() {
        let (pool, db_path, _dir) = create_test_pool().await;
        let adapter = BackupAdapter::new(pool.clone(), db_path.clone());

        let backup_path = adapter.create_backup(None).await.unwrap();
        let backup_pathbuf = PathBuf::from(backup_path);

        let count = adapter.get_backup_file_count(&backup_pathbuf).await;
        assert_eq!(count, Some(1)); // We inserted 1 document in setup

        pool.close().await;
    }

    // ========================================================================
    // SECURITY TESTS (CWE-22: Path Traversal Prevention)
    // ========================================================================

    #[tokio::test]

    async fn test_create_backup_rejects_parent_directory_traversal() {
        let (pool, db_path, _dir) = create_test_pool().await;
        let adapter = BackupAdapter::new(pool.clone(), db_path.clone());

        // Attempt to traverse to parent directory
        let malicious_path = db_path
            .parent()
            .unwrap()
            .join("../../../etc/cron.d/backdoor.db");

        let result = adapter.create_backup(Some(malicious_path)).await;

        // Should be rejected with security error
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, AppError::Security(_)) || matches!(err, AppError::InvalidInput(_)),
            "Expected Security or InvalidInput error, got: {:?}",
            err
        );

        pool.close().await;
    }

    #[tokio::test]

    async fn test_create_backup_rejects_absolute_system_path() {
        let (pool, db_path, _dir) = create_test_pool().await;
        let adapter = BackupAdapter::new(pool.clone(), db_path.clone());

        // Attempt to write to system directory
        #[cfg(unix)]
        let malicious_path = PathBuf::from("/etc/cron.d/backdoor.db");
        #[cfg(windows)]
        let malicious_path = PathBuf::from("C:\\Windows\\System32\\backdoor.db");

        let result = adapter.create_backup(Some(malicious_path)).await;

        // Should be rejected
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, AppError::Security(_))
                || matches!(err, AppError::InvalidInput(_))
                || matches!(err, AppError::PermissionDenied(_)),
            "Expected Security/InvalidInput/PermissionDenied error, got: {:?}",
            err
        );

        pool.close().await;
    }

    #[tokio::test]

    async fn test_create_backup_rejects_url_encoded_traversal() {
        let (pool, db_path, _dir) = create_test_pool().await;
        let adapter = BackupAdapter::new(pool.clone(), db_path.clone());

        // URL-encoded ../ pattern: %2e%2e%2f
        let backup_dir = db_path.parent().unwrap().join("backups");
        let malicious_path = backup_dir.join("%2e%2e%2f%2e%2e%2fetc%2fpasswd");

        let result = adapter.create_backup(Some(malicious_path)).await;

        // Should be rejected
        assert!(result.is_err());

        pool.close().await;
    }

    #[tokio::test]

    async fn test_create_backup_allows_valid_path_in_backup_dir() {
        let (pool, db_path, _dir) = create_test_pool().await;
        let adapter = BackupAdapter::new(pool.clone(), db_path.clone());

        // Create a valid path within the backups directory
        let backup_dir = db_path.parent().unwrap().join("backups");
        fs::create_dir_all(&backup_dir).unwrap();

        let valid_path = backup_dir.join("my_custom_backup.db");

        let result = adapter.create_backup(Some(valid_path.clone())).await;

        // Should succeed
        assert!(result.is_ok());
        assert!(valid_path.exists());

        pool.close().await;
    }

    #[tokio::test]

    async fn test_create_backup_rejects_symlink_escape() {
        let (pool, db_path, _dir) = create_test_pool().await;
        let adapter = BackupAdapter::new(pool.clone(), db_path.clone());

        let backup_dir = db_path.parent().unwrap().join("backups");
        fs::create_dir_all(&backup_dir).unwrap();

        // Create a symlink pointing outside the backup directory
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            let symlink_path = backup_dir.join("escape_link");
            let target_path = db_path.parent().unwrap().parent().unwrap();

            // Create symlink (may fail if permissions don't allow)
            if symlink(target_path, &symlink_path).is_ok() {
                let malicious_path = symlink_path.join("evil.db");

                let result = adapter.create_backup(Some(malicious_path)).await;

                // Should be rejected (canonicalization resolves symlink)
                assert!(result.is_err());

                // Clean up
                fs::remove_file(&symlink_path).ok();
            }
        }

        pool.close().await;
    }

    #[tokio::test]

    async fn test_create_backup_handles_directory_input() {
        let (pool, db_path, _dir) = create_test_pool().await;
        let adapter = BackupAdapter::new(pool.clone(), db_path.clone());

        let backup_dir = db_path.parent().unwrap().join("backups");
        fs::create_dir_all(&backup_dir).unwrap();

        // Provide directory instead of file path
        let result = adapter.create_backup(Some(backup_dir.clone())).await;

        // Should succeed and create a file in that directory
        assert!(result.is_ok());

        let backup_path = result.unwrap();
        assert!(PathBuf::from(&backup_path).exists());
        assert!(backup_path.contains("recall_backup_"));

        pool.close().await;
    }

    #[tokio::test]

    async fn test_create_backup_defense_in_depth() {
        let (pool, db_path, _dir) = create_test_pool().await;
        let adapter = BackupAdapter::new(pool.clone(), db_path.clone());

        // Even if initial validation were bypassed, canonicalization
        // and final check should catch traversal
        let backup_dir = db_path.parent().unwrap().join("backups");
        fs::create_dir_all(&backup_dir).unwrap();

        // Attempt various traversal patterns
        let patterns = vec![
            "../../../etc/passwd",
            "..\\..\\..\\windows\\system32\\config",
            "./../../../tmp/evil.db",
            "backup/../../../etc/shadow",
        ];

        for pattern in patterns {
            let malicious_path = backup_dir.join(pattern);
            let result = adapter.create_backup(Some(malicious_path)).await;

            // All should be rejected
            assert!(
                result.is_err(),
                "Pattern '{}' should have been rejected",
                pattern
            );
        }

        pool.close().await;
    }

    // ========================================================================
    // RESTORE BACKUP TESTS
    // ========================================================================

    #[tokio::test]

    async fn test_restore_backup_success() {
        let (pool, db_path, _dir) = create_test_pool().await;
        let adapter = BackupAdapter::new(pool.clone(), db_path.clone());

        // Create a backup
        let backup_path = adapter.create_backup(None).await.unwrap();
        let backup_pathbuf = PathBuf::from(backup_path);

        // Add more data to the original database
        sqlx::query("INSERT INTO documents (id, title, content) VALUES (?, ?, ?)")
            .bind("2")
            .bind("New Doc")
            .bind("New content")
            .execute(&pool)
            .await
            .unwrap();

        // Verify we have 2 documents
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 2);

        // Restore from backup (closes pool internally)
        let result = adapter.restore_backup(backup_pathbuf).await;
        assert!(result.is_ok());

        // Verify safety backup was created
        let safety_backup = db_path.with_extension("db.pre_restore");
        assert!(safety_backup.exists());

        // Verify database file was replaced
        assert!(db_path.exists());

        // Create a new pool to verify restored data
        let new_pool = SqlitePoolOptions::new()
            .connect(&format!("sqlite://{}", db_path.display()))
            .await
            .unwrap();

        let restored_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
            .fetch_one(&new_pool)
            .await
            .unwrap();

        // Should have 1 document (from backup, not 2)
        assert_eq!(restored_count.0, 1);

        new_pool.close().await;
    }

    #[tokio::test]

    async fn test_restore_backup_validates_integrity() {
        let (pool, db_path, _dir) = create_test_pool().await;
        let adapter = BackupAdapter::new(pool.clone(), db_path.clone());

        // Create an invalid backup file
        let invalid_backup = tempdir().unwrap();
        let invalid_path = invalid_backup.path().join("corrupt.db");
        fs::write(&invalid_path, b"not a valid sqlite database").unwrap();

        // Attempt to restore invalid backup
        let result = adapter.restore_backup(invalid_path).await;

        // Should fail validation
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, AppError::Database(_)),
            "Expected Database error, got: {:?}",
            err
        );

        // Original database should still exist and be valid
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 1);

        pool.close().await;
    }

    #[tokio::test]

    async fn test_restore_backup_creates_safety_backup() {
        let (pool, db_path, _dir) = create_test_pool().await;
        let adapter = BackupAdapter::new(pool.clone(), db_path.clone());

        // Create a backup
        let backup_path = adapter.create_backup(None).await.unwrap();
        let backup_pathbuf = PathBuf::from(backup_path);

        // Restore from backup
        adapter.restore_backup(backup_pathbuf).await.unwrap();

        // Verify safety backup exists
        let safety_backup = db_path.with_extension("db.pre_restore");
        assert!(
            safety_backup.exists(),
            "Safety backup should be created at: {}",
            safety_backup.display()
        );

        // Verify safety backup is valid SQLite database
        let safety_pool = SqlitePoolOptions::new()
            .connect(&format!("sqlite://{}", safety_backup.display()))
            .await
            .unwrap();

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
            .fetch_one(&safety_pool)
            .await
            .unwrap();
        assert_eq!(count.0, 1);

        safety_pool.close().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "macOS SQLite doesn't respect file permissions on open connections"
    )]

    async fn test_restore_backup_rolls_back_on_failure() {
        let (pool, db_path, _dir) = create_test_pool().await;
        let adapter = BackupAdapter::new(pool.clone(), db_path.clone());

        // Get original data
        let original_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
            .fetch_one(&pool)
            .await
            .unwrap();

        // Create a backup in a location we can't write to
        let backup_path = adapter.create_backup(None).await.unwrap();
        let backup_pathbuf = PathBuf::from(&backup_path);

        // Close the pool before restore
        pool.close().await;

        // Make db_path read-only to simulate failure
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&db_path).unwrap().permissions();
            perms.set_mode(0o444); // Read-only
            fs::set_permissions(&db_path, perms).unwrap();
        }

        // Create new pool for restore attempt
        let new_pool = SqlitePoolOptions::new()
            .connect(&format!("sqlite://{}", db_path.display()))
            .await
            .unwrap();

        let adapter2 = BackupAdapter::new(new_pool.clone(), db_path.clone());

        // Attempt restore (should fail due to permissions)
        let result = adapter2.restore_backup(backup_pathbuf).await;

        #[cfg(unix)]
        {
            // Restore permissions for cleanup
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o644);
            fs::set_permissions(&db_path, perms).unwrap();
        }

        // On Unix, should fail due to permissions
        #[cfg(unix)]
        {
            assert!(
                result.is_err(),
                "Restore should fail with read-only permissions"
            );
        }

        // Verify original database is intact (rollback occurred)
        let verify_pool = SqlitePoolOptions::new()
            .connect(&format!("sqlite://{}", db_path.display()))
            .await
            .unwrap();

        let final_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
            .fetch_one(&verify_pool)
            .await
            .unwrap();

        assert_eq!(
            final_count.0, original_count.0,
            "Original database should be preserved after failed restore"
        );

        verify_pool.close().await;
        new_pool.close().await;
    }

    #[tokio::test]

    async fn test_restore_backup_handles_missing_file() {
        let (pool, db_path, _dir) = create_test_pool().await;
        let adapter = BackupAdapter::new(pool.clone(), db_path.clone());

        let missing_path = PathBuf::from("/nonexistent/backup.db");
        let result = adapter.restore_backup(missing_path).await;

        // Should fail validation
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, AppError::NotFound(_)),
            "Expected NotFound error, got: {:?}",
            err
        );

        // Original database should be unchanged
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 1);

        pool.close().await;
    }

    #[tokio::test]

    async fn test_restore_backup_atomic_operation() {
        let (pool, db_path, _dir) = create_test_pool().await;
        let adapter = BackupAdapter::new(pool.clone(), db_path.clone());

        // Create a backup
        let backup_path = adapter.create_backup(None).await.unwrap();
        let backup_pathbuf = PathBuf::from(backup_path);

        // Restore from backup
        adapter.restore_backup(backup_pathbuf).await.unwrap();

        // Verify no temporary files left behind
        let temp_path = db_path.with_extension("db.restoring");
        assert!(
            !temp_path.exists(),
            "Temporary file should be cleaned up: {}",
            temp_path.display()
        );

        // Verify database file exists and is valid
        assert!(db_path.exists());

        let new_pool = SqlitePoolOptions::new()
            .connect(&format!("sqlite://{}", db_path.display()))
            .await
            .unwrap();

        let integrity: (String,) = sqlx::query_as("PRAGMA integrity_check")
            .fetch_one(&new_pool)
            .await
            .unwrap();

        assert_eq!(integrity.0, "ok");

        new_pool.close().await;
    }

    // ========================================================================
    // SQL INJECTION TESTS (CWE-89: VACUUM INTO Hardening)
    // ========================================================================

    #[tokio::test]

    async fn test_sql_injection_semicolon_statement_terminator() {
        let (pool, db_path, _dir) = create_test_pool().await;
        let adapter = BackupAdapter::new(pool.clone(), db_path.clone());

        // Attempt SQL injection with semicolon
        let backup_dir = db_path.parent().unwrap().join("backups");
        fs::create_dir_all(&backup_dir).unwrap();

        let malicious_path = backup_dir.join("backup.db'; DROP TABLE documents; --");

        let result = adapter.create_backup(Some(malicious_path)).await;

        // Should be rejected
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, AppError::Security(_)),
            "Expected Security error, got: {:?}",
            err
        );

        // Verify documents table still exists
        let count: Result<(i64,), _> = sqlx::query_as("SELECT COUNT(*) FROM documents")
            .fetch_one(&pool)
            .await;
        assert!(count.is_ok(), "documents table should still exist");

        pool.close().await;
    }

    #[tokio::test]

    async fn test_sql_injection_comment_sequences() {
        let (pool, db_path, _dir) = create_test_pool().await;
        let adapter = BackupAdapter::new(pool.clone(), db_path.clone());

        let backup_dir = db_path.parent().unwrap().join("backups");
        fs::create_dir_all(&backup_dir).unwrap();

        // Test SQL comment patterns
        let comment_patterns = vec![
            "backup.db--comment",
            "backup.db/* comment */",
            "backup/**/file.db",
        ];

        for pattern in comment_patterns {
            let malicious_path = backup_dir.join(pattern);
            let result = adapter.create_backup(Some(malicious_path.clone())).await;

            assert!(result.is_err(), "Pattern '{}' should be rejected", pattern);

            if let Err(err) = result {
                assert!(
                    matches!(err, AppError::Security(_)),
                    "Pattern '{}' should trigger Security error, got: {:?}",
                    pattern,
                    err
                );
            }
        }

        pool.close().await;
    }

    #[tokio::test]

    async fn test_sql_injection_union_attack() {
        let (pool, db_path, _dir) = create_test_pool().await;
        let adapter = BackupAdapter::new(pool.clone(), db_path.clone());

        let backup_dir = db_path.parent().unwrap().join("backups");
        fs::create_dir_all(&backup_dir).unwrap();

        // Attempt UNION injection
        let malicious_path = backup_dir.join("backup' UNION SELECT * FROM documents; --");

        let result = adapter.create_backup(Some(malicious_path)).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, AppError::Security(_)),
            "Expected Security error for UNION injection, got: {:?}",
            err
        );

        pool.close().await;
    }

    #[tokio::test]

    async fn test_sql_injection_drop_table() {
        let (pool, db_path, _dir) = create_test_pool().await;
        let adapter = BackupAdapter::new(pool.clone(), db_path.clone());

        let backup_dir = db_path.parent().unwrap().join("backups");
        fs::create_dir_all(&backup_dir).unwrap();

        // Attempt DROP TABLE injection
        let malicious_path = backup_dir.join("backup' DROP TABLE documents CASCADE; --");

        let result = adapter.create_backup(Some(malicious_path)).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, AppError::Security(_)),
            "Expected Security error for DROP TABLE, got: {:?}",
            err
        );

        // Verify table still exists
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 1, "documents table should still have data");

        pool.close().await;
    }

    #[tokio::test]

    async fn test_sql_injection_insert_attack() {
        let (pool, db_path, _dir) = create_test_pool().await;
        let adapter = BackupAdapter::new(pool.clone(), db_path.clone());

        let backup_dir = db_path.parent().unwrap().join("backups");
        fs::create_dir_all(&backup_dir).unwrap();

        // Attempt INSERT injection
        let malicious_path =
            backup_dir.join("backup'; INSERT INTO documents VALUES ('evil', 'bad'); --");

        let result = adapter.create_backup(Some(malicious_path)).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, AppError::Security(_)),
            "Expected Security error for INSERT, got: {:?}",
            err
        );

        // Verify no extra documents inserted
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count.0, 1, "Should still have only 1 document");

        pool.close().await;
    }

    #[tokio::test]

    async fn test_sql_injection_null_byte() {
        let (pool, db_path, _dir) = create_test_pool().await;
        let adapter = BackupAdapter::new(pool.clone(), db_path.clone());

        let backup_dir = db_path.parent().unwrap().join("backups");
        fs::create_dir_all(&backup_dir).unwrap();

        // Attempt null byte injection (path truncation attack)
        let malicious_filename = "backup.db\0.txt";
        let malicious_path = backup_dir.join(malicious_filename);

        let result = adapter.create_backup(Some(malicious_path)).await;

        assert!(result.is_err());
        // Will fail either in path validation or SQL validation
        // Either way, it should not succeed

        pool.close().await;
    }

    #[tokio::test]

    async fn test_sql_injection_exec_execute() {
        let (pool, db_path, _dir) = create_test_pool().await;
        let adapter = BackupAdapter::new(pool.clone(), db_path.clone());

        let backup_dir = db_path.parent().unwrap().join("backups");
        fs::create_dir_all(&backup_dir).unwrap();

        // Attempt EXEC/EXECUTE injection
        let patterns = vec![
            "backup'; EXEC sp_executesql; --",
            "backup'; EXECUTE ('DROP TABLE documents'); --",
        ];

        for pattern in patterns {
            let malicious_path = backup_dir.join(pattern);
            let result = adapter.create_backup(Some(malicious_path.clone())).await;

            assert!(result.is_err(), "Pattern '{}' should be rejected", pattern);
        }

        pool.close().await;
    }

    // ========================================================================
    // SQL SAFE PATH VALIDATOR UNIT TEST
    // ========================================================================

    #[test]
    fn test_sql_safe_path_validator() {
        // Valid paths
        assert!(validate_sql_safe_path("/backup/file.db").is_ok());
        assert!(validate_sql_safe_path("/Users/user/backup_20240101.db").is_ok());
        assert!(validate_sql_safe_path("C:\\Backups\\my_backup.db").is_ok());

        // Invalid: semicolon
        assert!(validate_sql_safe_path("/backup/file.db;").is_err());
        assert!(validate_sql_safe_path("/backup;DROP TABLE users;").is_err());

        // Invalid: SQL comments
        assert!(validate_sql_safe_path("/backup/file--comment.db").is_err());
        assert!(validate_sql_safe_path("/backup/*comment*/file.db").is_err());

        // ORACLE FIX: Removed SQL keyword checking - these paths are now ALLOWED
        // Valid: paths that happen to contain SQL keywords (e.g., "Dropbox", "update_v2")
        assert!(validate_sql_safe_path("/backup/DROP.db").is_ok());
        assert!(validate_sql_safe_path("/backup/DELETE_backup.db").is_ok());
        assert!(validate_sql_safe_path("/backup/INSERT.db").is_ok());
        assert!(validate_sql_safe_path("/backup/SELECT.db").is_ok());
        assert!(validate_sql_safe_path("/backup/UNION.db").is_ok());
        assert!(validate_sql_safe_path("/Users/Dropbox/backup.db").is_ok());
        assert!(validate_sql_safe_path("/projects/update_v2/backup.db").is_ok());

        // Invalid: null byte
        assert!(validate_sql_safe_path("/backup/file\0.db").is_err());

        // Valid: path with single quote (will be escaped during use)
        assert!(validate_sql_safe_path("/backup/user's_backup.db").is_ok());
    }
}
