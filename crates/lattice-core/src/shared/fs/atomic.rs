use crate::shared::error::{AppError, Result, ResultExt};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

/// Atomic file replacement and advisory locking for filesystem resources.
pub struct AtomicFs;

impl AtomicFs {
    /// Atomically write to a file (write to temp, then rename)
    pub fn write_file<P: AsRef<Path>>(path: P, content: &[u8]) -> Result<()> {
        let path = path.as_ref();
        let parent = path
            .parent()
            .ok_or_else(|| AppError::InvalidInput("Invalid file path".to_string()))?;

        let mut temp_file = tempfile::Builder::new()
            .prefix(".tmp_")
            .tempfile_in(parent)
            .context("Failed to create temporary file")?;
        temp_file
            .write_all(content)
            .context("Failed to write to temporary file")?;
        temp_file
            .as_file()
            .sync_all()
            .context("Failed to sync temporary file")?;

        // Atomically rename temp file to target
        temp_file
            .persist(path)
            .map_err(|error| error.error)
            .context("Failed to rename temporary file")?;

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

    /// Run an operation while holding an exclusive advisory lock.
    /// The persistent sidecar must not be unlinked: waiters lock that same file.
    pub fn with_file_lock<P: AsRef<Path>, F, R>(path: P, f: F) -> Result<R>
    where
        F: FnOnce() -> Result<R>,
    {
        let mut lock_path = path.as_ref().as_os_str().to_os_string();
        lock_path.push(".lock");
        let mut options = OpenOptions::new();
        options.create(true).truncate(false).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options
            .open(lock_path)
            .context("Failed to open lock file")?;
        lock.lock().context("Failed to acquire lock")?;
        // Dropping the handle releases the lock, including on error or unwind.
        f()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn failed_replacement_removes_temporary_file() {
        let temp = TempDir::new().unwrap();
        let destination = temp.path().join("directory");
        fs::create_dir(&destination).unwrap();

        assert!(AtomicFs::write_file(&destination, b"cannot replace a directory").is_err());
        assert!(destination.is_dir());
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }

    #[test]
    fn replacement_overwrites_existing_content() {
        let temp = TempDir::new().unwrap();
        let destination = temp.path().join("note.md");
        fs::write(&destination, b"old text").unwrap();
        AtomicFs::write_file(&destination, b"new text").unwrap();
        assert_eq!(fs::read(destination).unwrap(), b"new text");
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }

    #[test]
    fn file_lock_excludes_other_handles_and_releases_after_error() {
        let temp = TempDir::new().unwrap();
        let resource = temp.path().join("note.md");
        let lock_path = temp.path().join("note.md.lock");
        let result: Result<()> = AtomicFs::with_file_lock(&resource, || {
            let contender = OpenOptions::new().write(true).open(&lock_path).unwrap();
            assert!(matches!(
                contender.try_lock(),
                Err(std::fs::TryLockError::WouldBlock)
            ));
            Err(AppError::InvalidInput("operation failed".into()))
        });
        assert!(result.is_err());
        assert!(lock_path.is_file());
        let contender = OpenOptions::new().write(true).open(lock_path).unwrap();
        contender.try_lock().unwrap();
    }

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
