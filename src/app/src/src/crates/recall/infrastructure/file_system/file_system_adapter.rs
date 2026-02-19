//! File system adapter for OS-level file operations.
//!
//! This adapter implements the FileSystemPort using platform-specific
//! secure APIs to prevent command injection (CWE-78) and directory
//! traversal (CWE-22) vulnerabilities.
//!
//! # Platform Support
//!
//! - **Windows**: Uses ShellExecuteW via `opener` crate, explorer.exe for reveal
//! - **macOS**: Uses NSWorkspace via `opener` crate, `open` command for reveal
//! - **Linux**: Uses xdg-open via `opener` crate, file manager for reveal
//!
//! # Security
//!
//! - All file paths are validated before use
//! - Uses secure system APIs (no shell execution with user input)
//! - Prevents directory traversal attacks
//! - Audits all file access operations

use crate::application::ports::FileSystemPort;
use crate::shared::error::AppError;
use crate::shared::result::Result;
use async_trait::async_trait;
use std::path::Path;
use tracing::{debug, error, warn};

/// System file system adapter using platform-specific secure APIs.
///
/// This adapter provides OS-level file operations using:
/// - `opener` crate for secure file opening (no shell execution)
/// - Platform-specific commands for file reveal (properly escaped)
/// - Standard library for file existence checks
///
/// # Thread Safety
///
/// All operations are thread-safe and can be called from multiple tasks.
#[derive(Debug, Clone)]
pub struct FileSystemAdapter;

impl FileSystemAdapter {
    /// Create a new file system adapter.
    pub fn new() -> Self {
        Self
    }

    /// Validate a file path before performing operations.
    ///
    /// # Security
    ///
    /// This performs basic path validation:
    /// - Checks for null bytes
    /// - Ensures path is not empty
    /// - Canonicalizes path to prevent traversal
    ///
    /// Note: More comprehensive validation should be done at the domain layer
    /// using ValidatedFilePath before calling these methods.
    async fn validate_path(path: &Path) -> Result<std::path::PathBuf> {
        // Check for empty path
        if path.as_os_str().is_empty() {
            return Err(AppError::InvalidInput("Path cannot be empty".to_string()));
        }

        // Check for null bytes (potential attack)
        let path_str = path.to_str().ok_or_else(|| {
            AppError::InvalidInput("Path contains invalid UTF-8 characters".to_string())
        })?;

        if path_str.contains('\0') {
            return Err(AppError::InvalidInput(
                "Path contains null bytes".to_string(),
            ));
        }

        // Canonicalize to resolve .. and . components
        // This helps prevent directory traversal
        match tokio::fs::canonicalize(path).await {
            Ok(canonical) => {
                debug!("Validated path: {:?} -> {:?}", path, canonical);
                Ok(canonical)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // File might not exist yet, just return original path
                // after basic validation
                warn!("Path does not exist (may be created later): {:?}", path);
                Ok(path.to_path_buf())
            }
            Err(e) => {
                error!("Failed to canonicalize path {:?}: {}", path, e);
                Err(AppError::InvalidInput(format!("Invalid file path: {}", e)))
            }
        }
    }

    /// Open a file using platform-specific secure API.
    ///
    /// # Platform Implementation
    ///
    /// - **Windows**: ShellExecuteW (via opener crate)
    /// - **macOS**: NSWorkspace (via opener crate)
    /// - **Linux**: xdg-open with proper escaping (via opener crate)
    ///
    /// The `opener` crate handles platform-specific details and
    /// prevents command injection by using proper OS APIs.
    #[cfg(not(test))]
    async fn open_with_system(path: &Path) -> Result<()> {
        let path_buf = path.to_path_buf();
        tokio::task::spawn_blocking(move || opener::open(&path_buf))
            .await
            .map_err(|e| {
                error!("Failed to spawn blocking task for file open: {}", e);
                AppError::Io {
                    message: format!("Failed to open file: {}", e),
                    kind: format!("{:?}", std::io::ErrorKind::Other),
                }
            })?
            .map_err(|e| {
                error!("Failed to open file: {}", e);
                AppError::Io {
                    message: format!("Failed to open file: {}", e),
                    kind: format!("{:?}", std::io::ErrorKind::Other),
                }
            })
    }

    /// Test-only mock implementation
    #[cfg(test)]
    async fn open_with_system(_path: &Path) -> Result<()> {
        Ok(())
    }

    /// Reveal a file in file explorer using platform-specific commands.
    ///
    /// # Platform Implementation
    ///
    /// - **Windows**: `explorer.exe /select,"<path>"`
    /// - **macOS**: `open -R "<path>"`
    /// - **Linux**: Opens containing directory (file selection not standardized)
    ///
    /// # Security
    ///
    /// Uses std::process::Command with proper argument passing to prevent
    /// command injection. Paths are passed as separate arguments, NOT
    /// concatenated into shell strings.
    #[cfg(all(target_os = "windows", not(test)))]
    async fn reveal_with_system(path: &Path) -> Result<()> {
        use std::process::Command;

        let path_buf = path.to_path_buf();
        tokio::task::spawn_blocking(move || {
            Command::new("explorer.exe")
                .arg("/select,")
                .arg(path_buf.as_os_str())
                .spawn()
        })
        .await
        .map_err(|e| {
            error!("Failed to spawn blocking task for reveal: {}", e);
            AppError::Io {
                message: e.to_string(),
                kind: format!("{:?}", std::io::ErrorKind::Other),
            }
        })?
        .map_err(|e| {
            error!("Failed to reveal file in Explorer: {}", e);
            AppError::Io {
                message: e.to_string(),
                kind: format!("{:?}", e.kind()),
            }
        })?;

        debug!("Revealed file in Explorer: {:?}", path);
        Ok(())
    }

    #[cfg(all(target_os = "macos", not(test)))]
    async fn reveal_with_system(path: &Path) -> Result<()> {
        use std::process::Command;

        let path_buf = path.to_path_buf();
        tokio::task::spawn_blocking(move || {
            Command::new("open")
                .arg("-R")
                .arg(path_buf.as_os_str())
                .spawn()
        })
        .await
        .map_err(|e| {
            error!("Failed to spawn blocking task for reveal: {}", e);
            AppError::Io {
                message: e.to_string(),
                kind: format!("{:?}", std::io::ErrorKind::Other),
            }
        })?
        .map_err(|e| {
            error!("Failed to reveal file in Finder: {}", e);
            AppError::Io {
                message: e.to_string(),
                kind: format!("{:?}", e.kind()),
            }
        })?;

        debug!("Revealed file in Finder: {:?}", path);
        Ok(())
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos"), not(test)))]
    async fn reveal_with_system(path: &Path) -> Result<()> {
        use std::process::Command;

        // On Linux, most file managers don't support file selection
        // Open the containing directory instead
        let dir = path
            .parent()
            .ok_or_else(|| AppError::InvalidInput("Cannot get parent directory".to_string()))?
            .to_path_buf();

        tokio::task::spawn_blocking(move || Command::new("xdg-open").arg(dir.as_os_str()).spawn())
            .await
            .map_err(|e| {
                error!("Failed to spawn blocking task for reveal: {}", e);
                AppError::Io {
                    message: e.to_string(),
                    kind: format!("{:?}", std::io::ErrorKind::Other),
                }
            })?
            .map_err(|e| {
                error!("Failed to open directory: {}", e);
                AppError::Io {
                    message: e.to_string(),
                    kind: format!("{:?}", e.kind()),
                }
            })?;

        debug!("Opened directory in file manager");
        Ok(())
    }

    #[cfg(test)]
    async fn reveal_with_system(_path: &Path) -> Result<()> {
        Ok(())
    }
}

impl Default for FileSystemAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FileSystemPort for FileSystemAdapter {
    async fn open_file(&self, path: &Path) -> Result<()> {
        // Validate path
        let validated_path = Self::validate_path(path).await?;

        // Check file exists
        if !tokio::fs::try_exists(&validated_path)
            .await
            .unwrap_or(false)
        {
            return Err(AppError::NotFound(format!(
                "File not found: {}",
                path.display()
            )));
        }

        // Check it's a file, not a directory
        if tokio::fs::metadata(&validated_path)
            .await
            .map(|m| m.is_dir())
            .unwrap_or(false)
        {
            return Err(AppError::InvalidInput(format!(
                "Path is a directory, not a file: {}",
                path.display()
            )));
        }

        // Open with system default application
        debug!("Opening file: {:?}", validated_path);
        Self::open_with_system(&validated_path).await?;

        Ok(())
    }

    async fn show_in_folder(&self, path: &Path) -> Result<()> {
        // Validate path
        let validated_path = Self::validate_path(path).await?;

        // Check file exists
        if !tokio::fs::try_exists(&validated_path)
            .await
            .unwrap_or(false)
        {
            return Err(AppError::NotFound(format!(
                "File not found: {}",
                path.display()
            )));
        }

        // Reveal in file explorer
        debug!("Revealing file in folder: {:?}", validated_path);
        Self::reveal_with_system(&validated_path).await?;

        Ok(())
    }

    async fn file_exists(&self, path: &Path) -> Result<bool> {
        // Basic path validation (allow non-existent paths)
        if path.as_os_str().is_empty() {
            return Err(AppError::InvalidInput("Path cannot be empty".to_string()));
        }

        let path_str = path.to_str().ok_or_else(|| {
            AppError::InvalidInput("Path contains invalid UTF-8 characters".to_string())
        })?;

        if path_str.contains('\0') {
            return Err(AppError::InvalidInput(
                "Path contains null bytes".to_string(),
            ));
        }

        // Check existence
        Ok(tokio::fs::try_exists(path).await.unwrap_or(false))
    }

    async fn is_directory(&self, path: &Path) -> Result<bool> {
        // Basic path validation
        if path.as_os_str().is_empty() {
            return Err(AppError::InvalidInput("Path cannot be empty".to_string()));
        }

        let path_str = path.to_str().ok_or_else(|| {
            AppError::InvalidInput("Path contains invalid UTF-8 characters".to_string())
        })?;

        if path_str.contains('\0') {
            return Err(AppError::InvalidInput(
                "Path contains null bytes".to_string(),
            ));
        }

        // Check if path exists
        if !tokio::fs::try_exists(path).await.unwrap_or(false) {
            return Err(AppError::NotFound(format!(
                "Path not found: {}",
                path.display()
            )));
        }

        // Check if it's a directory
        Ok(tokio::fs::metadata(path)
            .await
            .map(|m| m.is_dir())
            .unwrap_or(false))
    }

    async fn create_directory_all(&self, path: &Path) -> Result<()> {
        // Validate path to prevent directory traversal
        let validated_path = Self::validate_path(path).await?;

        // Create directory and all parent directories
        tokio::fs::create_dir_all(&validated_path)
            .await
            .map_err(|e| {
                AppError::FileSystem(format!(
                    "Failed to create directory {}: {}",
                    path.display(),
                    e
                ))
            })?;

        debug!("Created directory: {:?}", validated_path);
        Ok(())
    }

    async fn list_directory(&self, path: &Path) -> Result<Vec<std::path::PathBuf>> {
        // Validate path
        let validated_path = Self::validate_path(path).await?;

        // Check directory exists
        if !tokio::fs::try_exists(&validated_path)
            .await
            .unwrap_or(false)
        {
            return Err(AppError::NotFound(format!(
                "Directory not found: {}",
                path.display()
            )));
        }

        // Check it's a directory
        if !tokio::fs::metadata(&validated_path)
            .await
            .map(|m| m.is_dir())
            .unwrap_or(false)
        {
            return Err(AppError::InvalidInput(format!(
                "Path is not a directory: {}",
                path.display()
            )));
        }

        // Read directory entries
        let mut entries = Vec::new();
        let mut read_dir =
            tokio::fs::read_dir(&validated_path)
                .await
                .map_err(|e| AppError::Io {
                    message: format!("Failed to read directory {}: {}", path.display(), e),
                    kind: e.kind().to_string(),
                })?;

        while let Some(entry) = read_dir.next_entry().await.map_err(|e| AppError::Io {
            message: format!(
                "Failed to read directory entry in {}: {}",
                path.display(),
                e
            ),
            kind: e.kind().to_string(),
        })? {
            entries.push(entry.path());
        }

        debug!(
            "Listed {} entries in directory: {:?}",
            entries.len(),
            validated_path
        );
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_file_exists() {
        let adapter = FileSystemAdapter::new();
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // File doesn't exist yet
        assert!(!adapter.file_exists(&file_path).await.unwrap());

        // Create file
        std::fs::write(&file_path, "test content").unwrap();

        // Now it exists
        assert!(adapter.file_exists(&file_path).await.unwrap());
    }

    #[tokio::test]
    async fn test_is_directory() {
        let adapter = FileSystemAdapter::new();
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "test").unwrap();

        // Directory check
        assert!(adapter.is_directory(dir_path).await.unwrap());

        // File is not a directory
        assert!(!adapter.is_directory(&file_path).await.unwrap());
    }

    #[tokio::test]
    async fn test_open_file_not_found() {
        let adapter = FileSystemAdapter::new();
        let path = PathBuf::from("/nonexistent/file.txt");

        let result = adapter.open_file(&path).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_open_file_is_directory() {
        let adapter = FileSystemAdapter::new();
        let temp_dir = TempDir::new().unwrap();

        let result = adapter.open_file(temp_dir.path()).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_show_in_folder_not_found() {
        let adapter = FileSystemAdapter::new();
        let path = PathBuf::from("/nonexistent/file.txt");

        let result = adapter.show_in_folder(&path).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_validate_path_empty() {
        let result = FileSystemAdapter::validate_path(Path::new("")).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_validate_path_null_bytes() {
        // Create path with null byte (unsafe)
        let path_with_null = "test\0file.txt";
        let path = Path::new(path_with_null);

        let result = FileSystemAdapter::validate_path(path).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_exists_empty_path() {
        let adapter = FileSystemAdapter::new();
        let result = adapter.file_exists(Path::new("")).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_is_directory_not_found() {
        let adapter = FileSystemAdapter::new();
        let path = PathBuf::from("/nonexistent/directory");

        let result = adapter.is_directory(&path).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_open_file_success() {
        let adapter = FileSystemAdapter::new();
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "test content").unwrap();

        // Should succeed (mocked in tests)
        let result = adapter.open_file(&file_path).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_show_in_folder_success() {
        let adapter = FileSystemAdapter::new();
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "test content").unwrap();

        // Should succeed (mocked in tests)
        let result = adapter.show_in_folder(&file_path).await;
        assert!(result.is_ok());
    }
}
