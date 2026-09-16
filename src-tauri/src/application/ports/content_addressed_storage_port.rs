//! Content-addressed storage port.
//!
//! This port defines the interface for importing files to content-addressed
//! storage and checking for duplicates by hash.
//!
//! # Purpose
//!
//! - Import files to library storage (`~/.lattice/files/{hash}/filename`)
//! - Detect duplicates by content hash (not file path)
//! - Preserve original filenames for UX
//! - Enable library-based indexing (files owned by library, not source location)
//!
//! # Architecture
//!
//! Content-addressed storage solves the problem of files being moved or deleted
//! after indexing. Once a file is imported, it lives in the library permanently
//! at a hash-based location, independent of the original source path.
//!
//! # Infrastructure Implementations
//!
//! - `ContentAddressedStorage` - SHA-256 based storage in `~/.lattice/files/`
//!
//! # Example Usage
//!
//! ```rust,no_run
//! use crate::application::ports::ContentAddressedStoragePort;
//! use std::path::Path;
//!
//! async fn import_and_check(
//!     storage: &impl ContentAddressedStoragePort,
//!     path: &Path,
//! ) -> Result<(PathBuf, String)> {
//!     // Import file to library
//!     let (library_path, hash) = storage.import_file(path).await?;
//!
//!     // Check if already exists (idempotent)
//!     assert!(storage.exists_by_hash(&hash).await?);
//!
//!     Ok((library_path, hash))
//! }
//! ```

use crate::shared::result::Result;
use async_trait::async_trait;
use std::path::{Path, PathBuf};

/// Port for content-addressed file storage.
///
/// Implementations must:
/// - Store files in content-addressed layout (hash-based directories)
/// - Detect duplicates by content hash (not file path)
/// - Preserve original filenames for UX
/// - Be thread-safe (`Send + Sync`)
#[async_trait]
pub trait ContentAddressedStoragePort: Send + Sync {
    /// Import file to content-addressed storage.
    ///
    /// Copies the source file to library storage at `{library_root}/{hash}/filename`.
    /// If a file with the same hash already exists, returns the existing library path
    /// without copying (idempotent operation).
    ///
    /// # Arguments
    ///
    /// * `source_path` - Path to source file to import
    ///
    /// # Returns
    ///
    /// Tuple of:
    /// - `PathBuf` - Library path where file is stored
    /// - `String` - SHA256 hash of file content (hex-encoded)
    ///
    /// # Errors
    ///
    /// - `AppError::FileNotFound` if source file doesn't exist
    /// - `AppError::PermissionDenied` if cannot read source or write to library
    /// - `AppError::FileRead` if cannot read source file
    /// - `AppError::FileStorage` if cannot write to library
    ///
    /// # Behavior
    ///
    /// - **Deduplication**: Files with identical content (same SHA256) are stored only once
    /// - **Idempotent**: Importing same file multiple times returns same library path
    /// - **Preserves filename**: Original filename is preserved in library path
    /// - **Atomic**: File is fully copied before being made available
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// // Import file
    /// let (library_path, hash) = storage.import_file(
    ///     Path::new("/downloads/document.pdf")
    /// ).await?;
    ///
    /// // Library path: ~/.lattice/files/a3d5f7.../document.pdf
    /// // Hash: "a3d5f7e9..." (64 hex chars)
    ///
    /// // Import again - returns same path (no copy)
    /// let (path2, hash2) = storage.import_file(
    ///     Path::new("/downloads/document.pdf")
    /// ).await?;
    /// assert_eq!(path2, library_path);
    /// assert_eq!(hash2, hash);
    /// ```
    async fn import_file(&self, source_path: &Path) -> Result<(PathBuf, String)>;

    /// Check if file exists in library by content hash.
    ///
    /// # Arguments
    ///
    /// * `hash` - SHA256 hash of file content (hex-encoded)
    ///
    /// # Returns
    ///
    /// `true` if a file with this hash exists in library, `false` otherwise
    ///
    /// # Errors
    ///
    /// - `AppError::FileStorage` if library directory cannot be accessed
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// if storage.exists_by_hash(&hash).await? {
    ///     println!("File already in library");
    /// } else {
    ///     println!("File is new");
    /// }
    /// ```
    async fn exists_by_hash(&self, hash: &str) -> Result<bool>;

    /// Get library path for a file with given hash.
    ///
    /// # Arguments
    ///
    /// * `hash` - SHA256 hash of file content (hex-encoded)
    ///
    /// # Returns
    ///
    /// - `Some(PathBuf)` if file exists in library
    /// - `None` if no file with this hash exists
    ///
    /// # Errors
    ///
    /// - `AppError::FileStorage` if library directory cannot be accessed
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// if let Some(path) = storage.get_path_by_hash(&hash).await? {
    ///     println!("File exists at: {}", path.display());
    /// }
    /// ```
    async fn get_path_by_hash(&self, hash: &str) -> Result<Option<PathBuf>>;
}
