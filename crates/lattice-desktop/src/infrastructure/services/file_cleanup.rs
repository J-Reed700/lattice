use crate::infrastructure::services::validated_path::{PathValidationError, ValidatedFilePath};
use std::path::Path;
use std::time::Duration;
use thiserror::Error;
use tokio::time::sleep;
use tracing::{error, info, warn};

#[derive(Debug, Error)]
pub enum FileCleanupError {
    #[error("Path validation failed: {0}")]
    ValidationFailed(#[from] PathValidationError),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Max retry attempts exceeded after {0} attempts")]
    MaxRetriesExceeded(usize),

    #[error("Internal error: {0}")]
    InternalError(String),
}

pub struct FileCleanupService {
    max_retries: usize,
    initial_delay_ms: u64,
}

impl Default for FileCleanupService {
    fn default() -> Self {
        Self::new()
    }
}

impl FileCleanupService {
    pub fn new() -> Self {
        Self {
            max_retries: 3,
            initial_delay_ms: 100,
        }
    }

    pub fn with_retry_config(max_retries: usize, initial_delay_ms: u64) -> Self {
        Self {
            max_retries,
            initial_delay_ms,
        }
    }

    pub async fn delete_file<P: AsRef<Path>>(&self, path: P) -> Result<(), FileCleanupError> {
        let path = path.as_ref();

        let validated = ValidatedFilePath::validate_and_canonicalize(path)?;

        if !validated.exists() {
            info!(
                path = %validated.display(),
                "File does not exist, skipping deletion"
            );
            return Ok(());
        }

        if !validated.is_file() {
            warn!(
                path = %validated.display(),
                "Path is not a file, refusing to delete"
            );
            return Err(FileCleanupError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Path is not a file",
            )));
        }

        let mut attempts = 0;
        let mut last_error = None;

        while attempts < self.max_retries {
            attempts += 1;

            match tokio::fs::remove_file(validated.path()).await {
                Ok(_) => {
                    if attempts == 1 {
                        info!(
                            path = %validated.display(),
                            "File deleted successfully"
                        );
                    } else {
                        info!(
                            path = %validated.display(),
                            attempts = attempts,
                            "File deleted successfully after retry"
                        );
                    }
                    return Ok(());
                }
                Err(e) => {
                    last_error = Some(e);

                    if attempts < self.max_retries {
                        let delay = self.calculate_backoff_delay(attempts);
                        warn!(
                            path = %validated.display(),
                            attempt = attempts,
                            max_retries = self.max_retries,
                            delay_ms = delay.as_millis(),
                            error = %last_error.as_ref()
                                .map(|e| e.to_string())
                                .unwrap_or_else(|| "Unknown error".to_string()),
                            "File deletion failed, retrying after delay"
                        );
                        sleep(delay).await;
                    }
                }
            }
        }

        let error = last_error.ok_or_else(|| {
            FileCleanupError::InternalError(
                "Retry loop completed without error (logic bug)".to_string(),
            )
        })?;
        error!(
            path = %validated.display(),
            attempts = attempts,
            error = %error,
            "File deletion failed after all retry attempts"
        );

        Err(FileCleanupError::IoError(error))
    }

    pub async fn delete_file_best_effort<P: AsRef<Path>>(&self, path: P) {
        let path = path.as_ref();

        match self.delete_file(path).await {
            Ok(_) => {}
            Err(e) => {
                warn!(
                    path = %path.display(),
                    error = %e,
                    "Best-effort file deletion failed, continuing"
                );
            }
        }
    }

    fn calculate_backoff_delay(&self, attempt: usize) -> Duration {
        let multiplier = 2_u64.pow((attempt - 1) as u32);
        Duration::from_millis(self.initial_delay_ms * multiplier)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_delete_existing_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "test content").unwrap();

        let service = FileCleanupService::new();
        let result = service.delete_file(&file_path).await;

        if let Err(e) = &result {
            eprintln!("Delete failed: {:?}", e);
        }
        assert!(result.is_ok(), "Failed to delete file: {:?}", result.err());
        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_file_succeeds() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("nonexistent.txt");

        let service = FileCleanupService::new();
        let result = service.delete_file(&file_path).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_refuses_to_delete_directory() {
        let temp_dir = TempDir::new().unwrap();

        let service = FileCleanupService::new();
        let result = service.delete_file(temp_dir.path()).await;

        assert!(result.is_err());
        match result {
            Err(FileCleanupError::IoError(e)) => {
                assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput);
            }
            _ => panic!("Expected IoError"),
        }
    }

    #[tokio::test]
    async fn test_rejects_path_traversal() {
        let service = FileCleanupService::new();
        let result = service.delete_file("../../etc/passwd").await;

        assert!(result.is_err());
        assert!(matches!(result, Err(FileCleanupError::ValidationFailed(_))));
    }

    #[tokio::test]
    async fn test_best_effort_deletion_does_not_panic() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "test").unwrap();

        let service = FileCleanupService::new();
        service.delete_file_best_effort(&file_path).await;

        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn test_best_effort_handles_invalid_path() {
        let service = FileCleanupService::new();
        service.delete_file_best_effort("../../../etc/passwd").await;
    }

    #[tokio::test]
    async fn test_backoff_calculation() {
        let service = FileCleanupService::new();

        assert_eq!(
            service.calculate_backoff_delay(1),
            Duration::from_millis(100)
        );
        assert_eq!(
            service.calculate_backoff_delay(2),
            Duration::from_millis(200)
        );
        assert_eq!(
            service.calculate_backoff_delay(3),
            Duration::from_millis(400)
        );
    }

    #[tokio::test]
    async fn test_custom_retry_config() {
        let service = FileCleanupService::with_retry_config(5, 50);

        assert_eq!(service.max_retries, 5);
        assert_eq!(service.initial_delay_ms, 50);
        assert_eq!(
            service.calculate_backoff_delay(1),
            Duration::from_millis(50)
        );
        assert_eq!(
            service.calculate_backoff_delay(2),
            Duration::from_millis(100)
        );
    }

    #[tokio::test]
    async fn test_symlink_detection() {
        let temp_dir = TempDir::new().unwrap();
        let real_file = temp_dir.path().join("real.txt");
        let link_file = temp_dir.path().join("link.txt");

        fs::write(&real_file, "test").unwrap();

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&real_file, &link_file).unwrap();

            let service = FileCleanupService::new();
            let result = service.delete_file(&link_file).await;

            assert!(result.is_err());
            assert!(matches!(
                result,
                Err(FileCleanupError::ValidationFailed(
                    PathValidationError::SymlinkAttack { .. }
                ))
            ));

            assert!(real_file.exists());
        }

        #[cfg(windows)]
        {
            use std::os::windows::fs::symlink_file;
            if symlink_file(&real_file, &link_file).is_ok() {
                let service = FileCleanupService::new();
                let result = service.delete_file(&link_file).await;

                assert!(result.is_err());
                assert!(matches!(
                    result,
                    Err(FileCleanupError::ValidationFailed(
                        PathValidationError::SymlinkAttack { .. }
                    ))
                ));

                assert!(real_file.exists());
            }
        }
    }
}
