//! File system port for OS-level file operations.
//!
//! This port defines the interface for platform-specific file system operations
//! like opening files with default applications and revealing files in file explorers.
//!
//! # Purpose
//!
//! - Abstracts OS-specific file operations
//! - Provides secure file opening (prevents command injection CWE-78)
//! - Enables testing with mock file system operations
//! - Supports cross-platform file management
//!
//! # Security
//!
//! All implementations MUST:
//! - Validate file paths to prevent directory traversal (CWE-22)
//! - Use secure system APIs (no shell command execution)
//! - Check file existence before operations
//! - Audit file access operations
//!
//! # Infrastructure Implementations
//!
//! - `SystemFileSystemAdapter` - Real OS file system using opener crate
//! - `MockFileSystemAdapter` - Mock for testing
//!
//! # Example Usage
//!
//! ```rust
//! use crate::application::ports::FileSystemPort;
//! use std::path::Path;
//!
//! async fn open_document(
//!     fs: &impl FileSystemPort,
//!     path: &Path,
//! ) -> Result<()> {
//!     // Validate file exists
//!     if !fs.file_exists(path).await? {
//!         return Err(AppError::NotFound("File not found".into()));
//!     }
//!
//!     // Open with default application
//!     fs.open_file(path).await?;
//!     Ok(())
//! }
//! ```

use crate::shared::result::Result;
use async_trait::async_trait;
use std::path::Path;

/// Port for OS-level file system operations.
///
/// Implementations must:
/// - Use secure platform APIs (no shell execution)
/// - Validate all file paths
/// - Prevent directory traversal attacks (CWE-22)
/// - Prevent command injection (CWE-78)
/// - Be thread-safe (`Send + Sync`)
/// - Audit security-relevant operations
#[async_trait]
pub trait FileSystemPort: Send + Sync {
    /// Open a file with the system's default application.
    ///
    /// This operation uses platform-specific secure APIs:
    /// - **Windows**: Uses ShellExecuteW via `opener` crate
    /// - **macOS**: Uses NSWorkspace via `opener` crate
    /// - **Linux**: Uses xdg-open with proper escaping via `opener` crate
    ///
    /// # Arguments
    ///
    /// * `path` - The file path to open. Must be validated by implementation.
    ///
    /// # Returns
    ///
    /// `Ok(())` if file was successfully opened.
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if file does not exist
    /// - `AppError::InvalidInput` if path is invalid or contains traversal attempts
    /// - `AppError::PermissionDenied` if file cannot be accessed
    /// - `AppError::Io` if system open operation fails
    ///
    /// # Security
    ///
    /// Implementations MUST:
    /// 1. Validate path to prevent directory traversal (CWE-22)
    /// 2. Check file exists before opening
    /// 3. Use secure APIs that don't invoke shell (prevents CWE-78)
    /// 4. Audit the operation for security logging
    ///
    /// # Example
    ///
    /// ```rust
    /// // Opens document.pdf with default PDF viewer
    /// fs.open_file(Path::new("/docs/document.pdf")).await?;
    /// ```
    async fn open_file(&self, path: &Path) -> Result<()>;

    /// Reveal a file in the system's file explorer.
    ///
    /// This operation highlights the file in the file manager:
    /// - **Windows**: Opens Explorer with file selected
    /// - **macOS**: Opens Finder with file selected
    /// - **Linux**: Opens file manager (directory only, selection not guaranteed)
    ///
    /// # Arguments
    ///
    /// * `path` - The file path to reveal. Must be validated by implementation.
    ///
    /// # Returns
    ///
    /// `Ok(())` if file was successfully revealed.
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if file does not exist
    /// - `AppError::InvalidInput` if path is invalid or contains traversal attempts
    /// - `AppError::PermissionDenied` if directory cannot be accessed
    /// - `AppError::Io` if system reveal operation fails
    ///
    /// # Security
    ///
    /// Implementations MUST:
    /// 1. Validate path to prevent directory traversal (CWE-22)
    /// 2. Check file exists before revealing
    /// 3. Use secure command construction (OsString, no shell)
    /// 4. Audit the operation
    ///
    /// # Platform Notes
    ///
    /// On Linux, most file managers don't support file selection, so this
    /// opens the containing directory instead.
    ///
    /// # Example
    ///
    /// ```rust
    /// // Opens file manager with document.txt selected
    /// fs.show_in_folder(Path::new("/docs/document.txt")).await?;
    /// ```
    async fn show_in_folder(&self, path: &Path) -> Result<()>;

    /// Check if a file exists.
    ///
    /// # Arguments
    ///
    /// * `path` - The file path to check
    ///
    /// # Returns
    ///
    /// `Ok(true)` if file exists and is accessible, `Ok(false)` otherwise.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if path contains traversal attempts
    /// - `AppError::PermissionDenied` if path cannot be accessed
    ///
    /// # Example
    ///
    /// ```rust
    /// if fs.file_exists(Path::new("/docs/file.txt")).await? {
    ///     println!("File exists");
    /// }
    /// ```
    async fn file_exists(&self, path: &Path) -> Result<bool>;

    /// Check if a path is a directory.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to check
    ///
    /// # Returns
    ///
    /// `Ok(true)` if path is a directory, `Ok(false)` otherwise.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if path contains traversal attempts
    /// - `AppError::NotFound` if path does not exist
    ///
    /// # Example
    ///
    /// ```rust
    /// if fs.is_directory(Path::new("/docs")).await? {
    ///     println!("Path is a directory");
    /// }
    /// ```
    async fn is_directory(&self, path: &Path) -> Result<bool>;

    /// Create a directory and all parent directories if they don't exist.
    ///
    /// This is equivalent to `mkdir -p` on Unix or `mkdir` with `/p` on Windows.
    ///
    /// # Arguments
    ///
    /// * `path` - The directory path to create
    ///
    /// # Returns
    ///
    /// `Ok(())` if directory was created or already exists.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if path is invalid or contains traversal attempts
    /// - `AppError::PermissionDenied` if directory cannot be created due to permissions
    /// - `AppError::FileSystem` if creation fails for other reasons
    ///
    /// # Security
    ///
    /// Implementations MUST:
    /// 1. Validate path to prevent directory traversal (CWE-22)
    /// 2. Use secure filesystem APIs
    /// 3. Audit directory creation operations
    ///
    /// # Example
    ///
    /// ```rust
    /// // Creates /models/llama-7b and any missing parent directories
    /// fs.create_directory_all(Path::new("/models/llama-7b")).await?;
    /// ```
    async fn create_directory_all(&self, path: &Path) -> Result<()>;

    /// List all entries in a directory.
    ///
    /// Returns paths of all files and subdirectories in the given directory.
    /// Does not recurse into subdirectories.
    ///
    /// # Arguments
    ///
    /// * `path` - The directory path to list
    ///
    /// # Returns
    ///
    /// `Ok(Vec<PathBuf>)` containing paths of all entries in the directory.
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if directory does not exist
    /// - `AppError::InvalidInput` if path is not a directory or contains traversal attempts
    /// - `AppError::PermissionDenied` if directory cannot be read
    /// - `AppError::Io` if directory read operation fails
    ///
    /// # Security
    ///
    /// Implementations MUST:
    /// 1. Validate path to prevent directory traversal (CWE-22)
    /// 2. Check path is a directory before reading
    /// 3. Use secure filesystem APIs
    /// 4. Audit directory listing operations
    ///
    /// # Example
    ///
    /// ```rust
    /// // List all entries in /docs directory
    /// let entries = fs.list_directory(Path::new("/docs")).await?;
    /// for entry in entries {
    ///     println!("Found: {}", entry.display());
    /// }
    /// ```
    async fn list_directory(&self, path: &Path) -> Result<Vec<std::path::PathBuf>>;
}
