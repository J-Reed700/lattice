use crate::shared::error::AppError;
use std::path::{Path, PathBuf};

/// Service for calculating file checksums.
#[async_trait::async_trait]
pub trait ChecksumService: Send + Sync {
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
#[async_trait::async_trait]
pub trait FileSystemAccess: Send + Sync {
    async fn exists(&self, path: &Path) -> Result<bool, AppError>;
    async fn is_directory(&self, path: &Path) -> Result<bool, AppError>;
    async fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, AppError>;
    async fn file_size(&self, path: &Path) -> Result<u64, AppError>;
}
