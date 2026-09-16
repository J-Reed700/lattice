//! File storage port for the application layer.
//!
//! This port defines the interface for file system operations.
//! Infrastructure implementations handle actual I/O, validation, and security.
//!
//! # Purpose
//!
//! - Abstracts file system implementation details
//! - Enforces security validation (path traversal prevention)
//! - Enables testing with mock file systems
//! - Supports content hashing for change detection
//!
//! # Infrastructure Implementations
//!
//! - `SecureFileStorageAdapter` - Real file system with path validation
//! - `MockFileStorageAdapter` - In-memory mock file system for testing
//! - `EncryptedFileStorageAdapter` - Encrypted file storage
//!
//! # Example Usage
//!
//! ```rust
//! use crate::application::ports::FileStoragePort;
//! use std::path::Path;
//!
//! async fn read_and_hash(
//!     storage: &impl FileStoragePort,
//!     path: &Path,
//! ) -> Result<(String, String)> {
//!     let content = storage.read_file(path).await?;
//!     let hash = storage.compute_hash(path).await?;
//!     Ok((content, hash))
//! }
//! ```

use crate::shared::result::Result;
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
    /// # Arguments
    ///
    /// * `path` - The file path to read. Must be validated by implementation.
    ///
    /// # Returns
    ///
    /// The file contents as a UTF-8 string.
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
    ///
    /// # Example
    ///
    /// ```rust
    /// let content = storage.read_file(Path::new("/docs/file.txt")).await?;
    /// println!("File contains: {}", content);
    /// ```
    async fn read_file(&self, path: &Path) -> Result<String>;

    /// Read file contents as raw bytes.
    ///
    /// Use this for binary files or when UTF-8 validation is not required.
    ///
    /// # Arguments
    ///
    /// * `path` - The file path to read
    ///
    /// # Returns
    ///
    /// The file contents as a byte vector.
    ///
    /// # Errors
    ///
    /// Same as `read_file`, except no UTF-8 validation error.
    ///
    /// # Example
    ///
    /// ```rust
    /// let bytes = storage.read_file_bytes(Path::new("/docs/image.png")).await?;
    /// ```
    async fn read_file_bytes(&self, path: &Path) -> Result<Vec<u8>>;

    /// Write contents to a file, creating it if it doesn't exist.
    ///
    /// This operation should be atomic - either the complete write succeeds
    /// or the file is left unchanged.
    ///
    /// # Arguments
    ///
    /// * `path` - The file path to write. Parent directories must exist.
    /// * `content` - The UTF-8 string content to write
    ///
    /// # Errors
    ///
    /// - `AppError::PermissionDenied` if write permission is denied
    /// - `AppError::InvalidInput` if path contains directory traversal attempts
    /// - `AppError::Io` if write operation fails
    /// - `AppError::NotFound` if parent directory does not exist
    ///
    /// # Example
    ///
    /// ```rust
    /// storage.write_file(
    ///     Path::new("/docs/output.txt"),
    ///     "File contents",
    /// ).await?;
    /// ```
    async fn write_file(&self, path: &Path, content: &str) -> Result<()>;

    /// Write raw bytes to a file.
    ///
    /// # Arguments
    ///
    /// * `path` - The file path to write
    /// * `content` - The byte content to write
    ///
    /// # Errors
    ///
    /// Same as `write_file`.
    ///
    /// # Example
    ///
    /// ```rust
    /// storage.write_file_bytes(Path::new("/docs/data.bin"), &bytes).await?;
    /// ```
    async fn write_file_bytes(&self, path: &Path, content: &[u8]) -> Result<()>;

    /// Delete a file from storage.
    ///
    /// If the file does not exist, this is a no-op (returns Ok).
    ///
    /// # Arguments
    ///
    /// * `path` - The file path to delete
    ///
    /// # Errors
    ///
    /// - `AppError::PermissionDenied` if delete permission is denied
    /// - `AppError::InvalidInput` if path contains directory traversal attempts
    /// - `AppError::Io` if deletion fails
    ///
    /// # Example
    ///
    /// ```rust
    /// storage.delete_file(Path::new("/docs/old-file.txt")).await?;
    /// ```
    async fn delete_file(&self, path: &Path) -> Result<()>;

    /// Compute a cryptographic hash of file contents.
    ///
    /// Used for change detection and deduplication. Typically SHA-256.
    ///
    /// # Arguments
    ///
    /// * `path` - The file path to hash
    ///
    /// # Returns
    ///
    /// A hex-encoded hash string (e.g., "a3d5f7e9...")
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if file does not exist
    /// - `AppError::PermissionDenied` if read permission is denied
    /// - `AppError::InvalidInput` if path contains directory traversal attempts
    /// - `AppError::Io` if file cannot be read
    ///
    /// # Example
    ///
    /// ```rust
    /// let hash = storage.compute_hash(Path::new("/docs/file.txt")).await?;
    /// println!("SHA-256: {}", hash);
    /// ```
    async fn compute_hash(&self, path: &Path) -> Result<String>;

    /// Check if a file exists.
    ///
    /// # Arguments
    ///
    /// * `path` - The file path to check
    ///
    /// # Returns
    ///
    /// `true` if the file exists and is readable, `false` otherwise.
    ///
    /// # Example
    ///
    /// ```rust
    /// if storage.exists(Path::new("/docs/file.txt")).await {
    ///     println!("File exists");
    /// }
    /// ```
    async fn exists(&self, path: &Path) -> bool;

    /// Get file metadata (size, modification time, etc.).
    ///
    /// # Arguments
    ///
    /// * `path` - The file path to get metadata for
    ///
    /// # Returns
    ///
    /// File metadata including size in bytes and last modified timestamp.
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if file does not exist
    /// - `AppError::PermissionDenied` if metadata cannot be accessed
    /// - `AppError::Io` if metadata retrieval fails
    ///
    /// # Example
    ///
    /// ```rust
    /// let metadata = storage.metadata(Path::new("/docs/file.txt")).await?;
    /// println!("Size: {} bytes", metadata.size);
    /// ```
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
