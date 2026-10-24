//! File storage port for reading, writing, and querying filesystem files.

use crate::shared::error::Result;
use async_trait::async_trait;
use std::path::Path;

/// Port for file storage operations.
#[async_trait]
pub trait FileStoragePort: Send + Sync {
    /// Read the complete contents of a file as a UTF-8 string.
    async fn read_file(&self, path: &Path) -> Result<String>;

    /// Read file contents as raw bytes.
    async fn read_file_bytes(&self, path: &Path) -> Result<Vec<u8>>;

    /// Write UTF-8 string contents to a file.
    async fn write_file(&self, path: &Path, content: &str) -> Result<()>;

    /// Write raw bytes to a file.
    async fn write_file_bytes(&self, path: &Path, content: &[u8]) -> Result<()>;

    /// Delete a file from storage.
    async fn delete_file(&self, path: &Path) -> Result<()>;

    /// Compute a cryptographic hash of file contents.
    async fn compute_hash(&self, path: &Path) -> Result<String>;

    /// Check if a file exists.
    async fn exists(&self, path: &Path) -> bool;

    /// Get file metadata (size, modification time, etc.).
    async fn metadata(&self, path: &Path) -> Result<FileMetadata>;
}

/// File metadata information.
#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub size: u64,
    pub modified_at: i64,
    pub is_file: bool,
    pub is_directory: bool,
}
