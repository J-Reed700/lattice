//! File storage port for the application layer.
//!
//! This port defines the interface for file system operations.
//! Infrastructure implementations handle actual I/O, validation, and security.

use crate::shared::error::Result;
use async_trait::async_trait;
use std::path::Path;

/// Port for file storage operations.
///
/// Implementations must:
/// - Validate all file paths to prevent directory traversal (CWE-22)
/// - Handle large files efficiently (streaming or chunked)
/// - Provide atomic write operations
/// - Be thread-safe (`Send + Sync`)
#[async_trait]
pub trait FileStoragePort: Send + Sync {
    /// Read the complete contents of a file as a UTF-8 string.
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if file does not exist
    /// - `AppError::PermissionDenied` if read permission is denied
    /// - `AppError::InvalidInput` if path contains directory traversal attempts
    /// - `AppError::Io` if file cannot be read or is not valid UTF-8
    /// - `AppError::FileTooLarge` if file exceeds size limits
    ///
    /// # Security
    ///
    /// Implementations MUST validate paths to prevent directory traversal attacks.
    async fn read_file(&self, path: &Path) -> Result<String>;

    /// Read file contents as raw bytes.
    ///
    /// Use this for binary files or when UTF-8 validation is not required.
    ///
    /// # Errors
    ///
    /// Same as `read_file`, except no UTF-8 validation error.
    async fn read_file_bytes(&self, path: &Path) -> Result<Vec<u8>>;

    /// Write contents to a file, creating it if it doesn't exist.
    ///
    /// This operation should be atomic - either the complete write succeeds
    /// or the file is left unchanged.
    ///
    /// # Errors
    ///
    /// - `AppError::PermissionDenied` if write permission is denied
    /// - `AppError::InvalidInput` if path contains directory traversal attempts
    /// - `AppError::Io` if write operation fails
    /// - `AppError::NotFound` if parent directory does not exist
    async fn write_file(&self, path: &Path, content: &str) -> Result<()>;

    /// Write raw bytes to a file.
    ///
    /// # Errors
    ///
    /// Same as `write_file`.
    async fn write_file_bytes(&self, path: &Path, content: &[u8]) -> Result<()>;

    /// Delete a file from storage.
    ///
    /// If the file does not exist, this is a no-op (returns Ok).
    ///
    /// # Errors
    ///
    /// - `AppError::PermissionDenied` if delete permission is denied
    /// - `AppError::InvalidInput` if path contains directory traversal attempts
    /// - `AppError::Io` if deletion fails
    async fn delete_file(&self, path: &Path) -> Result<()>;

    /// Compute a cryptographic hash of file contents.
    ///
    /// Used for change detection and deduplication. Typically SHA-256.
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if file does not exist
    /// - `AppError::PermissionDenied` if read permission is denied
    /// - `AppError::InvalidInput` if path contains directory traversal attempts
    /// - `AppError::Io` if file cannot be read
    async fn compute_hash(&self, path: &Path) -> Result<String>;

    /// Check if a file exists.
    async fn exists(&self, path: &Path) -> bool;

    /// Get file metadata (size, modification time, etc.).
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if file does not exist
    /// - `AppError::PermissionDenied` if metadata cannot be accessed
    /// - `AppError::Io` if metadata retrieval fails
    async fn metadata(&self, path: &Path) -> Result<FileMetadata>;
}

/// File metadata information.
///
/// Returned by `FileStoragePort::metadata()`.
#[derive(Debug, Clone)]
pub struct FileMetadata {
    /// File size in bytes
    pub size: u64,

    /// Last modified timestamp (Unix epoch seconds)
    pub modified_at: i64,

    /// Whether this is a regular file (vs. directory, symlink, etc.)
    pub is_file: bool,

    /// Whether this is a directory
    pub is_directory: bool,
}
