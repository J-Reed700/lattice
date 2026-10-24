//! File system port for OS-level file operations.

use crate::shared::error::Result;
use async_trait::async_trait;
use std::path::Path;

/// Port for OS-level file system operations.
#[async_trait]
pub trait FileSystemPort: Send + Sync {
    /// Open a file with the system's default application.
    async fn open_file(&self, path: &Path) -> Result<()>;

    /// Reveal a file in the system's file explorer.
    async fn show_in_folder(&self, path: &Path) -> Result<()>;

    /// Check if a file exists.
    async fn file_exists(&self, path: &Path) -> Result<bool>;

    /// Check if a path is a directory.
    async fn is_directory(&self, path: &Path) -> Result<bool>;

    /// Create a directory and all parent directories if they don't exist.
    async fn create_directory_all(&self, path: &Path) -> Result<()>;

    /// List all entries in a directory (non-recursive).
    async fn list_directory(&self, path: &Path) -> Result<Vec<std::path::PathBuf>>;
}
