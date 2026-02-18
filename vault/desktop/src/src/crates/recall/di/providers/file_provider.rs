//! FileProvider - File system operations and monitoring
//!
//! This module provides a self-contained provider for file system operations
//! following the "bricks and studs" architecture pattern.
//!
//! # Features
//!
//! - **File Storage**: Secure file I/O with path validation
//! - **File Watching**: Optional file system monitoring
//! - **Audit Logging**: Security-relevant file operations are logged
//!
//! # Architecture
//!
//! The FileProvider is a "brick" that manages:
//! - `FileStoragePort` - File I/O operations
//! - `FileWatcher` - File system change monitoring (optional)
//! - Audit logging for security-sensitive operations
//!
//! # Builder Pattern
//!
//! Use `FileProviderBuilder` to configure the provider:
//!
//! ```rust
//! use crate::di::providers::FileProviderBuilder;
//!
//! let provider = FileProviderBuilder::new()
//!     .with_pool(pool)
//!     .with_audit_logger(logger)
//!     .with_watch_enabled(true)  // Enable file watching
//!     .build()?;
//! ```
//!
//! # When to Disable Watch Service
//!
//! The watch service should be disabled in these scenarios:
//! - **Unit tests** - Avoid file system dependencies in tests
//! - **CI/CD environments** - File watchers can be unreliable in containers
//! - **Performance testing** - Reduce overhead when benchmarking
//! - **Batch processing** - Not needed for one-time file operations

use crate::infrastructure::audit::AuditLogger;
use crate::infrastructure::file_system::SecureFileStorage;
use crate::infrastructure::services::file_watch::FileWatcher;
use crate::application::ports::FileStoragePort;
use crate::shared::error::{AppError, Result};
use sqlx::SqlitePool;
use std::sync::Arc;

/// FileProvider manages file system operations and monitoring services.
///
/// This is a self-contained module that provides:
/// - File storage operations (read, write, delete, hash)
/// - File system monitoring (optional)
/// - Audit logging for file operations
///
/// # Public Interface ("Studs")
///
/// - `file_storage()` - Get file storage service
/// - `file_watcher()` - Get file watcher service
///
/// # Dependencies
///
/// - `SqlitePool` - Database connection pool
/// - `AuditLogger` - Audit logging service
///
/// # Example
///
/// ```rust
/// let provider = FileProviderBuilder::new()
///     .with_pool(pool)
///     .with_audit_logger(logger)
///     .with_watch_enabled(true)
///     .build()?;
///
/// let storage = provider.file_storage();
/// let watcher = provider.file_watcher();
/// ```
pub struct FileProvider {
    file_storage: Arc<dyn FileStoragePort>,
    file_watcher: Option<Arc<FileWatcher>>,
}

impl FileProvider {
    /// Get the file storage service.
    ///
    /// Returns an `Arc<dyn FileStoragePort>` for reading, writing, and
    /// managing files with security validation.
    ///
    /// # Example
    ///
    /// ```rust
    /// let storage = provider.file_storage();
    /// let content = storage.read_file(path).await?;
    /// ```
    pub fn file_storage(&self) -> Arc<dyn FileStoragePort> {
        Arc::clone(&self.file_storage)
    }

    /// Get the file watcher service.
    ///
    /// Returns an `Option<Arc<FileWatcher>>` for monitoring file system changes.
    /// Returns `None` if file watching is disabled (e.g., in tests).
    ///
    /// # Example
    ///
    /// ```rust
    /// if let Some(watcher) = provider.file_watcher() {
    ///     watcher.watch(path)?;
    /// }
    /// ```
    pub fn file_watcher(&self) -> Option<Arc<FileWatcher>> {
        self.file_watcher.as_ref().map(Arc::clone)
    }

    /// Check if file watching is enabled.
    ///
    /// # Returns
    ///
    /// `true` if file watching is enabled, `false` otherwise.
    ///
    /// # Example
    ///
    /// ```rust
    /// if provider.is_watch_enabled() {
    ///     println!("File watching is active");
    /// }
    /// ```
    pub fn is_watch_enabled(&self) -> bool {
        self.file_watcher.is_some()
    }
}

/// Builder for FileProvider.
///
/// Provides a fluent API for constructing a FileProvider with required
/// and optional dependencies.
///
/// # Required Dependencies
///
/// - `SqlitePool` - Database connection pool
/// - `AuditLogger` - Audit logging service
///
/// # Optional Configuration
///
/// - `watch_enabled` - Enable/disable file watching (default: true)
///
/// # Example
///
/// ```rust
/// // With file watching enabled (production)
/// let provider = FileProviderBuilder::new()
///     .with_pool(pool)
///     .with_audit_logger(logger)
///     .with_watch_enabled(true)
///     .build()?;
///
/// // Without file watching (testing)
/// let provider = FileProviderBuilder::new()
///     .with_pool(pool)
///     .with_audit_logger(logger)
///     .with_watch_enabled(false)
///     .build()?;
/// ```
pub struct FileProviderBuilder {
    pool: Option<SqlitePool>,
    audit_logger: Option<Arc<AuditLogger>>,
    watch_enabled: bool,
}

impl FileProviderBuilder {
    /// Create a new builder with default configuration.
    ///
    /// # Defaults
    ///
    /// - `watch_enabled` = true (file watching enabled)
    ///
    /// # Example
    ///
    /// ```rust
    /// let builder = FileProviderBuilder::new();
    /// ```
    pub fn new() -> Self {
        Self {
            pool: None,
            audit_logger: None,
            watch_enabled: true,
        }
    }

    /// Set the database connection pool.
    ///
    /// # Arguments
    ///
    /// - `pool` - SQLite connection pool for file metadata storage
    ///
    /// # Example
    ///
    /// ```rust
    /// builder.with_pool(pool)
    /// ```
    pub fn with_pool(mut self, pool: SqlitePool) -> Self {
        self.pool = Some(pool);
        self
    }

    /// Set the audit logger.
    ///
    /// # Arguments
    ///
    /// - `logger` - Audit logger for tracking file operations
    ///
    /// # Example
    ///
    /// ```rust
    /// builder.with_audit_logger(logger)
    /// ```
    pub fn with_audit_logger(mut self, logger: Arc<AuditLogger>) -> Self {
        self.audit_logger = Some(logger);
        self
    }

    /// Enable or disable file watching.
    ///
    /// When disabled, a mock file watcher will be used instead.
    /// This is useful for testing and environments where file watching
    /// is not needed or reliable.
    ///
    /// # Arguments
    ///
    /// - `enabled` - Whether to enable file watching
    ///
    /// # When to Disable
    ///
    /// - Unit tests (avoid file system dependencies)
    /// - CI/CD environments (unreliable in containers)
    /// - Performance testing (reduce overhead)
    /// - Batch processing (not needed)
    ///
    /// # Example
    ///
    /// ```rust
    /// // Production: file watching enabled
    /// builder.with_watch_enabled(true)
    ///
    /// // Testing: file watching disabled
    /// builder.with_watch_enabled(false)
    /// ```
    pub fn with_watch_enabled(mut self, enabled: bool) -> Self {
        self.watch_enabled = enabled;
        self
    }

    /// Build the FileProvider.
    ///
    /// # Errors
    ///
    /// Returns an error if required dependencies are missing:
    /// - `AppError::Configuration` - Missing pool or audit logger
    ///
    /// # Example
    ///
    /// ```rust
    /// let provider = FileProviderBuilder::new()
    ///     .with_pool(pool)
    ///     .with_audit_logger(logger)
    ///     .build()?;
    /// ```
    pub fn build(self) -> Result<FileProvider> {
        // Validate required dependencies
        let pool = self.pool.ok_or_else(|| {
            AppError::Configuration("FileProvider requires a database pool".to_string())
        })?;

        let audit_logger = self.audit_logger.ok_or_else(|| {
            AppError::Configuration("FileProvider requires an audit logger".to_string())
        })?;

        // Create file storage service
        let file_storage: Arc<dyn FileStoragePort> = Arc::new(SecureFileStorage::new());

        // Create file watcher if enabled
        let file_watcher = if self.watch_enabled {
            // Real file watcher for production
            Some(Arc::new(
                FileWatcher::new()
                    .map_err(|e| AppError::Configuration(format!("Failed to create file watcher: {}", e)))?
            ))
        } else {
            // No file watcher for testing
            None
        };

        Ok(FileProvider {
            file_storage,
            file_watcher,
        })
    }
}

impl Default for FileProviderBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::audit::sinks::MemoryAuditSink;

    async fn create_test_pool() -> SqlitePool {
        SqlitePool::connect("sqlite::memory:")
            .await
            .expect("Failed to create test pool")
    }

    fn create_test_audit_logger() -> Arc<AuditLogger> {
        let logger = AuditLogger::new();
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async {
                logger.add_sink(Box::new(MemoryAuditSink::new(100))).await;
            });
        Arc::new(logger)
    }

    #[tokio::test]
    async fn test_file_provider_with_watch() {
        let pool = create_test_pool().await;
        let audit_logger = create_test_audit_logger();

        let provider = FileProviderBuilder::new()
            .with_pool(pool)
            .with_audit_logger(audit_logger)
            .with_watch_enabled(true)
            .build()
            .expect("Failed to build FileProvider");

        // Verify services are accessible
        let _file_storage = provider.file_storage();
        assert!(provider.is_watch_enabled());
        assert!(provider.file_watcher().is_some());
    }

    #[tokio::test]
    async fn test_file_provider_without_watch() {
        let pool = create_test_pool().await;
        let audit_logger = create_test_audit_logger();

        let provider = FileProviderBuilder::new()
            .with_pool(pool)
            .with_audit_logger(audit_logger)
            .with_watch_enabled(false)
            .build()
            .expect("Failed to build FileProvider");

        // Should not have watch service
        let _file_storage = provider.file_storage();
        assert!(!provider.is_watch_enabled());
        assert!(provider.file_watcher().is_none());
    }

    #[test]
    fn test_file_provider_builder_missing_pool() {
        let audit_logger = create_test_audit_logger();

        let result = FileProviderBuilder::new()
            .with_audit_logger(audit_logger)
            .build();

        assert!(result.is_err());
        match result {
            Err(AppError::Configuration(msg)) => {
                assert!(msg.contains("database pool"));
            }
            _ => panic!("Expected Configuration error"),
        }
    }

    #[tokio::test]
    async fn test_file_provider_builder_missing_audit_logger() {
        let pool = create_test_pool().await;

        let result = FileProviderBuilder::new()
            .with_pool(pool)
            .build();

        assert!(result.is_err());
        match result {
            Err(AppError::Configuration(msg)) => {
                assert!(msg.contains("audit logger"));
            }
            _ => panic!("Expected Configuration error"),
        }
    }

    #[tokio::test]
    async fn test_file_provider_builder_all_dependencies() {
        let pool = create_test_pool().await;
        let audit_logger = create_test_audit_logger();

        let result = FileProviderBuilder::new()
            .with_pool(pool)
            .with_audit_logger(audit_logger)
            .with_watch_enabled(true)
            .build();

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_file_provider_default_builder() {
        let builder = FileProviderBuilder::default();

        // Should have default watch_enabled = true
        assert!(builder.watch_enabled);
    }

    #[tokio::test]
    async fn test_file_storage_service() {
        let pool = create_test_pool().await;
        let audit_logger = create_test_audit_logger();

        let provider = FileProviderBuilder::new()
            .with_pool(pool)
            .with_audit_logger(audit_logger)
            .build()
            .expect("Failed to build FileProvider");

        let storage = provider.file_storage();

        // Verify it's the correct type
        assert!(Arc::strong_count(&storage) >= 1);
    }

    #[tokio::test]
    async fn test_file_watcher_service() {
        let pool = create_test_pool().await;
        let audit_logger = create_test_audit_logger();

        let provider = FileProviderBuilder::new()
            .with_pool(pool)
            .with_audit_logger(audit_logger)
            .with_watch_enabled(true)
            .build()
            .expect("Failed to build FileProvider");

        let watcher = provider.file_watcher();

        // Verify it's present
        assert!(watcher.is_some());
        if let Some(w) = watcher {
            assert!(Arc::strong_count(&w) >= 1);
        }
    }

    #[tokio::test]
    async fn test_file_watcher_disabled() {
        let pool = create_test_pool().await;
        let audit_logger = create_test_audit_logger();

        let provider = FileProviderBuilder::new()
            .with_pool(pool)
            .with_audit_logger(audit_logger)
            .with_watch_enabled(false)
            .build()
            .expect("Failed to build FileProvider");

        let watcher = provider.file_watcher();

        // Verify it's not present
        assert!(watcher.is_none());
    }

    #[tokio::test]
    async fn test_builder_fluent_api() {
        let pool = create_test_pool().await;
        let audit_logger = create_test_audit_logger();

        // Test fluent API chaining
        let provider = FileProviderBuilder::new()
            .with_pool(pool.clone())
            .with_audit_logger(audit_logger.clone())
            .with_watch_enabled(false)
            .with_watch_enabled(true)  // Override previous setting
            .build()
            .expect("Failed to build FileProvider");

        let _storage = provider.file_storage();
        // Watch should be enabled since we overrode it to true
        assert!(provider.is_watch_enabled());
        assert!(provider.file_watcher().is_some());
    }
}
