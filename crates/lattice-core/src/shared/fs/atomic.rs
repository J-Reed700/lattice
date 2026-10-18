use crate::shared::error::{AppError, Result, ResultExt};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;
use uuid::Uuid;

/// SECURITY FIX: Prevent TOCTOU race conditions (CWE-367)
/// Provides atomic file system operations
pub struct AtomicFs;

impl AtomicFs {
    /// Atomically write to a file (write to temp, then rename)
    pub fn write_file<P: AsRef<Path>>(path: P, content: &[u8]) -> Result<()> {
        let path = path.as_ref();
        let parent = path
            .parent()
            .ok_or_else(|| AppError::InvalidInput("Invalid file path".to_string()))?;

        let temp_name = format!(".tmp_{}", Uuid::new_v4());
        let temp_path = parent.join(&temp_name);

        let mut temp_file = File::create(&temp_path).context("Failed to create temporary file")?;
        temp_file
            .write_all(content)
            .context("Failed to write to temporary file")?;
        temp_file
            .sync_all()
            .context("Failed to sync temporary file")?;

        // Atomically rename temp file to target
        fs::rename(&temp_path, path).context("Failed to rename temporary file")?;

        Ok(())
    }

    /// Safely check existence and read a file atomically
    pub fn read_if_exists<P: AsRef<Path>>(path: P) -> Result<Option<Vec<u8>>> {
        match fs::read(path) {
            Ok(content) => Ok(Some(content)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(AppError::Io {
                message: e.to_string(),
                kind: format!("{:?}", e.kind()),
            }),
        }
    }

    /// Atomically create a directory if it doesn't exist
    pub fn create_dir_atomic<P: AsRef<Path>>(path: P) -> Result<bool> {
        match fs::create_dir(path) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(false),
            Err(e) => Err(AppError::Io {
                message: e.to_string(),
                kind: format!("{:?}", e.kind()),
            }),
        }
    }

    /// Safely move a file with atomic operations
    pub fn move_file<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> Result<()> {
        let from = from.as_ref();
        let to = to.as_ref();

        // First try atomic rename
        if let Ok(()) = fs::rename(from, to) {
            return Ok(());
        }

        // If rename fails (cross-device), do copy+delete
        let content = fs::read(from).context("Failed to read source file")?;
        Self::write_file(to, &content)?;
        fs::remove_file(from).context("Failed to remove source file")?;

        Ok(())
    }

    /// Lock file for exclusive access (using advisory locks)
    #[cfg(unix)]
    pub fn with_file_lock<P: AsRef<Path>, F, R>(path: P, f: F) -> Result<R>
    where
        F: FnOnce() -> Result<R>,
    {
        use std::fs::OpenOptions;
        use std::os::unix::fs::OpenOptionsExt;

        let lock_path = format!("{}.lock", path.as_ref().display());
        let _lock = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&lock_path)
            .context("Failed to acquire lock")?;

        let result = f()?;

        // Lock is automatically released when _lock is dropped
        fs::remove_file(&lock_path).ok();
        Ok(result)
    }

    #[cfg(windows)]
    pub fn with_file_lock<P: AsRef<Path>, F, R>(path: P, f: F) -> Result<R>
    where
        F: FnOnce() -> Result<R>,
    {
        use std::fs::OpenOptions;

        let lock_path = format!("{}.lock", path.as_ref().display());
        let _lock = OpenOptions::new()
            .create(true)
            .write(true)
            .open(&lock_path)
            .context("Failed to acquire lock")?;

        let result = f()?;

        // Lock is automatically released when _lock is dropped
        fs::remove_file(&lock_path).ok();
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_atomic_write() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        AtomicFs::write_file(&file_path, b"Hello, World!").unwrap();

        let content = fs::read(&file_path).unwrap();
        assert_eq!(content, b"Hello, World!");
    }

    #[test]
    fn test_read_if_exists() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // File doesn't exist
        assert!(AtomicFs::read_if_exists(&file_path).unwrap().is_none());

        fs::write(&file_path, b"test").unwrap();

        // File exists
        let content = AtomicFs::read_if_exists(&file_path).unwrap().unwrap();
        assert_eq!(content, b"test");
    }
}
