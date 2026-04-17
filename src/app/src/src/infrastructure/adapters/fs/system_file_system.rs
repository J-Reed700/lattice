//! System File System Adapter
//!
//! Infrastructure adapter implementing FileSystemPort using Tokio's async file system operations.
//!
//! This adapter provides secure, cross-platform file system operations including:
//! - Opening files with default applications (via `opener` crate)
//! - Revealing files in file managers
//! - Checking file existence and directory status
//! - Creating directories with parent creation
//! - Listing directory contents
//!
//! # Security
//!
//! All methods validate paths to prevent directory traversal attacks (CWE-22) and
//! use secure platform APIs instead of shell execution to prevent command injection (CWE-78).

use crate::application::ports::FileSystemPort;
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use std::path::Path;
use tokio::fs;

/// System file system adapter using Tokio for async operations.
///
/// This adapter implements secure file system operations using:
/// - `tokio::fs` for file system operations
/// - `opener` crate for opening files with default applications
///
/// # Thread Safety
///
/// This adapter is `Send + Sync` and safe to use across async tasks.
#[derive(Debug, Clone)]
pub struct SystemFileSystemAdapter;

impl SystemFileSystemAdapter {
    /// Create a new system file system adapter.
    pub fn new() -> Self {
        Self
    }

    /// Validate path to prevent directory traversal attacks (CWE-22).
    ///
    /// Rejects paths containing:
    /// - `..` (parent directory traversal)
    /// - Null bytes
    fn validate_path(path: &Path) -> Result<()> {
        let path_str = path
            .to_str()
            .ok_or_else(|| AppError::InvalidInput("Path contains invalid UTF-8".to_string()))?;

        // Reject null bytes
        if path_str.contains('\0') {
            return Err(AppError::InvalidInput(
                "Path contains null byte".to_string(),
            ));
        }

        // Reject parent directory traversal attempts
        for component in path.components() {
            if matches!(component, std::path::Component::ParentDir) {
                return Err(AppError::InvalidInput(
                    "Path contains parent directory traversal (..)".to_string(),
                ));
            }
        }

        Ok(())
    }
}

impl Default for SystemFileSystemAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FileSystemPort for SystemFileSystemAdapter {
    async fn open_file(&self, path: &Path) -> Result<()> {
        // Validate path for security
        Self::validate_path(path)?;

        // Check file exists
        if !path.exists() {
            return Err(AppError::NotFound(format!(
                "File not found: {}",
                path.display()
            )));
        }

        // Open with default application using opener crate
        opener::open(path).map_err(|e| AppError::Io {
            message: format!("Failed to open file {}: {}", path.display(), e),
            kind: "OpenerError".to_string(),
        })?;

        Ok(())
    }

    async fn show_in_folder(&self, path: &Path) -> Result<()> {
        // Validate path for security
        Self::validate_path(path)?;

        // Check file exists
        if !path.exists() {
            return Err(AppError::NotFound(format!(
                "File not found: {}",
                path.display()
            )));
        }

        // Reveal in file manager using opener crate
        // Note: This may not work on all platforms (Linux file managers vary)
        #[cfg(target_os = "windows")]
        {
            // Windows: Use explorer.exe /select
            use std::process::Command;
            Command::new("explorer")
                .args(&["/select,", &path.to_string_lossy()])
                .spawn()
                .map_err(|e| AppError::Io {
                    message: format!("Failed to reveal file {}: {}", path.display(), e),
                    kind: "ProcessSpawnError".to_string(),
                })?;
        }

        #[cfg(target_os = "macos")]
        {
            // macOS: Use open -R
            use std::process::Command;
            Command::new("open")
                .args(["-R", &path.to_string_lossy()])
                .spawn()
                .map_err(|e| AppError::Io {
                    message: format!("Failed to reveal file {}: {}", path.display(), e),
                    kind: "ProcessSpawnError".to_string(),
                })?;
        }

        #[cfg(target_os = "linux")]
        {
            // Linux: Open containing directory (selection not guaranteed)
            let parent = path.parent().ok_or_else(|| {
                AppError::InvalidInput("Cannot reveal root directory".to_string())
            })?;
            opener::open(parent).map_err(|e| AppError::Io {
                message: format!("Failed to reveal directory {}: {}", parent.display(), e),
                kind: "OpenerError".to_string(),
            })?;
        }

        Ok(())
    }

    async fn file_exists(&self, path: &Path) -> Result<bool> {
        // Validate path for security
        Self::validate_path(path)?;

        // Use tokio::fs::metadata for async check
        match fs::metadata(path).await {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Err(
                AppError::PermissionDenied(format!("Cannot access path {}: {}", path.display(), e)),
            ),
            Err(e) => Err(AppError::Io {
                message: format!("Error checking path {}: {}", path.display(), e),
                kind: e.kind().to_string(),
            }),
        }
    }

    async fn is_directory(&self, path: &Path) -> Result<bool> {
        // Validate path for security
        Self::validate_path(path)?;

        // Use tokio::fs::metadata for async check
        match fs::metadata(path).await {
            Ok(metadata) => Ok(metadata.is_dir()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(AppError::NotFound(format!(
                "Path not found: {}",
                path.display()
            ))),
            Err(e) => Err(AppError::Io {
                message: format!("Error checking path {}: {}", path.display(), e),
                kind: e.kind().to_string(),
            }),
        }
    }

    async fn create_directory_all(&self, path: &Path) -> Result<()> {
        // Validate path for security
        Self::validate_path(path)?;

        // Create directory and all missing parents
        fs::create_dir_all(path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                AppError::PermissionDenied(format!(
                    "Cannot create directory {}: {}",
                    path.display(),
                    e
                ))
            } else {
                AppError::FileSystem(format!(
                    "Failed to create directory {}: {}",
                    path.display(),
                    e
                ))
            }
        })?;

        Ok(())
    }

    async fn list_directory(&self, path: &Path) -> Result<Vec<std::path::PathBuf>> {
        // Validate path for security
        Self::validate_path(path)?;

        // Check path is a directory
        if !self.is_directory(path).await? {
            return Err(AppError::InvalidInput(format!(
                "Path is not a directory: {}",
                path.display()
            )));
        }

        // Read directory entries
        let mut entries = Vec::new();
        let mut read_dir = fs::read_dir(path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                AppError::PermissionDenied(format!(
                    "Cannot read directory {}: {}",
                    path.display(),
                    e
                ))
            } else {
                AppError::Io {
                    message: format!("Failed to read directory {}: {}", path.display(), e),
                    kind: e.kind().to_string(),
                }
            }
        })?;

        while let Some(entry) = read_dir.next_entry().await.map_err(|e| AppError::Io {
            message: format!("Error reading directory entry in {}: {}", path.display(), e),
            kind: e.kind().to_string(),
        })? {
            entries.push(entry.path());
        }

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_validate_path_rejects_parent_traversal() {
        let path = PathBuf::from("../etc/passwd");
        assert!(SystemFileSystemAdapter::validate_path(&path).is_err());

        let path = PathBuf::from("foo/../../bar");
        assert!(SystemFileSystemAdapter::validate_path(&path).is_err());
    }

    #[test]
    fn test_validate_path_accepts_normal_paths() {
        let path = PathBuf::from("/absolute/path/to/file.txt");
        assert!(SystemFileSystemAdapter::validate_path(&path).is_ok());

        let path = PathBuf::from("relative/path/to/file.txt");
        assert!(SystemFileSystemAdapter::validate_path(&path).is_ok());
    }

    #[tokio::test]
    async fn test_file_exists_returns_false_for_nonexistent() {
        let adapter = SystemFileSystemAdapter::new();
        let path = PathBuf::from("/nonexistent/file/path/12345.txt");

        let result = adapter.file_exists(&path).await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn test_is_directory_rejects_traversal() {
        let adapter = SystemFileSystemAdapter::new();
        let path = PathBuf::from("../etc");

        let result = adapter.is_directory(&path).await;
        assert!(result.is_err());
    }
}
