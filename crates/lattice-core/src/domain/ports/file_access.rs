//! File Access Port
//!
//! Defines the contract for file system operations needed by the domain layer.
//! This keeps the domain pure and testable by abstracting infrastructure concerns.

use crate::shared::error::AppError;
use std::path::{Path, PathBuf};

/// Service for calculating file checksums.
///
/// This port allows the domain layer to request checksum calculations
/// without depending on specific I/O implementations (tokio, std::fs, etc.).
#[async_trait::async_trait]
pub trait ChecksumService: Send + Sync {
    /// Calculate SHA256 checksum for a file.
    ///
    /// # Arguments
    ///
    /// * `file_path` - Path to the file to checksum
    ///
    /// # Returns
    ///
    /// * `Ok(String)` - Hexadecimal string representation of SHA256 hash
    /// * `Err(AppError)` - If file cannot be read or checksum calculation fails
    async fn calculate_sha256(&self, file_path: &Path) -> Result<String, AppError>;
}

/// Directory entry information
#[derive(Debug, Clone)]
pub struct DirectoryEntry {
    pub path: PathBuf,
    pub is_file: bool,
    pub size: u64,
}

/// Service for file system operations.
///
/// This port allows the domain layer to interact with the file system
/// without depending on specific I/O implementations (tokio, std::fs, etc.).
#[async_trait::async_trait]
pub trait FileSystemAccess: Send + Sync {
    /// Check if a path exists
    async fn exists(&self, path: &Path) -> Result<bool, AppError>;

    /// Check if a path is a directory
    async fn is_directory(&self, path: &Path) -> Result<bool, AppError>;

    /// Read directory entries
    async fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, AppError>;

    /// Get file metadata (size)
    async fn file_size(&self, path: &Path) -> Result<u64, AppError>;
}
