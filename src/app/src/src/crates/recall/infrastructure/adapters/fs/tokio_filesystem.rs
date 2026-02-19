//! Tokio-based filesystem adapter for domain file access.

use crate::domain::ports::file_access::{DirectoryEntry, FileSystemAccess};
use crate::shared::error::AppError;
use async_trait::async_trait;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct TokioFileSystemAdapter;

impl TokioFileSystemAdapter {
    pub fn new() -> Self {
        Self
    }

    fn io_error(message: String, error: std::io::Error) -> AppError {
        AppError::Io {
            message,
            kind: error.kind().to_string(),
        }
    }
}

#[async_trait]
impl FileSystemAccess for TokioFileSystemAdapter {
    async fn exists(&self, path: &Path) -> Result<bool, AppError> {
        tokio::fs::try_exists(path).await.map_err(|e| {
            Self::io_error(format!("Failed to check existence: {}", path.display()), e)
        })
    }

    async fn is_directory(&self, path: &Path) -> Result<bool, AppError> {
        let metadata = tokio::fs::metadata(path).await.map_err(|e| {
            Self::io_error(format!("Failed to read metadata: {}", path.display()), e)
        })?;
        Ok(metadata.is_dir())
    }

    async fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, AppError> {
        let mut entries = Vec::new();
        let mut dir = tokio::fs::read_dir(path).await.map_err(|e| {
            Self::io_error(format!("Failed to read directory: {}", path.display()), e)
        })?;

        while let Some(entry) = dir.next_entry().await.map_err(|e| {
            Self::io_error(
                format!("Failed to read directory entry: {}", path.display()),
                e,
            )
        })? {
            let entry_path = entry.path();
            let metadata = entry.metadata().await.map_err(|e| {
                Self::io_error(
                    format!("Failed to read metadata: {}", entry_path.display()),
                    e,
                )
            })?;
            entries.push(DirectoryEntry {
                path: entry_path,
                is_file: metadata.is_file(),
                size: metadata.len(),
            });
        }

        Ok(entries)
    }

    async fn file_size(&self, path: &Path) -> Result<u64, AppError> {
        let metadata = tokio::fs::metadata(path).await.map_err(|e| {
            Self::io_error(format!("Failed to read metadata: {}", path.display()), e)
        })?;
        Ok(metadata.len())
    }
}
