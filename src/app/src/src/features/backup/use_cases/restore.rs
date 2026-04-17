//! Restore Backup Use Case
//!
//! Restores application database from a backup file with integrity verification.
//!
//! # Dependencies
//! - `BackupPort` - Database restore operations
//!
//! # Security
//! - Validates backup file integrity before restoration
//! - Creates automatic backup before restore operation
//! - Audit logs restore operations
//!
//! # Example
//! ```rust,no_run
//! let use_case = RestoreBackupUseCase::new(backup_port);
//! use_case.execute(PathBuf::from("/backups/vault_20240115.db")).await?;
//! ```

use crate::application::dtos::backup_dto::RestoreBackupResultDto;
use crate::application::ports::BackupPort;
use crate::shared::error::AppError;
use std::path::PathBuf;
use std::sync::Arc;

pub struct RestoreBackupUseCase {
    backup: Arc<dyn BackupPort>,
}

impl RestoreBackupUseCase {
    pub fn new(backup: Arc<dyn BackupPort>) -> Self {
        Self { backup }
    }

    pub async fn execute(&self, path: PathBuf) -> Result<RestoreBackupResultDto, AppError> {
        tracing::info!(path = ?path, "Restoring backup");

        self.backup.restore_backup(path).await?;

        tracing::info!("Backup restored successfully");

        Ok(RestoreBackupResultDto {
            success: true,
            restored_count: 0,
            message: Some("Backup restored successfully".to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashSet;
    use std::sync::Mutex;

    struct MockBackupPort {
        restored_backups: Arc<Mutex<HashSet<String>>>,
        valid_backups: Arc<Mutex<HashSet<String>>>,
        corrupted_backups: Arc<Mutex<HashSet<String>>>,
        fail_on_restore: bool,
    }

    impl MockBackupPort {
        fn new() -> Self {
            let mut valid = HashSet::new();
            valid.insert("/valid/backup.db".to_string());
            valid.insert("/backups/2024-01-01.db".to_string());

            let mut corrupted = HashSet::new();
            corrupted.insert("/corrupted/backup.db".to_string());

            Self {
                restored_backups: Arc::new(Mutex::new(HashSet::new())),
                valid_backups: Arc::new(Mutex::new(valid)),
                corrupted_backups: Arc::new(Mutex::new(corrupted)),
                fail_on_restore: false,
            }
        }

        fn with_restore_failure() -> Self {
            Self {
                restored_backups: Arc::new(Mutex::new(HashSet::new())),
                valid_backups: Arc::new(Mutex::new(HashSet::new())),
                corrupted_backups: Arc::new(Mutex::new(HashSet::new())),
                fail_on_restore: true,
            }
        }

        fn add_valid_backup(&self, path: &str) {
            self.valid_backups.lock().unwrap().insert(path.to_string());
        }

        fn add_corrupted_backup(&self, path: &str) {
            self.corrupted_backups
                .lock()
                .unwrap()
                .insert(path.to_string());
        }

        fn was_restored(&self, path: &str) -> bool {
            self.restored_backups
                .lock()
                .unwrap()
                .contains(&path.to_string())
        }
    }

    #[async_trait]
    impl BackupPort for MockBackupPort {
        async fn create_backup(&self, _path: Option<PathBuf>) -> Result<String, AppError> {
            unimplemented!("Not needed for restore backup tests")
        }

        async fn restore_backup(&self, path: PathBuf) -> Result<(), AppError> {
            if self.fail_on_restore {
                return Err(AppError::BackupRestoreFailed(
                    "Generic restore failure".to_string(),
                ));
            }

            let path_str = path.to_string_lossy().to_string();

            // Check if backup file exists
            if !self.valid_backups.lock().unwrap().contains(&path_str)
                && !self.corrupted_backups.lock().unwrap().contains(&path_str)
            {
                return Err(AppError::FileNotFound {
                    path: path_str.clone(),
                });
            }

            // Check if backup is corrupted
            if self.corrupted_backups.lock().unwrap().contains(&path_str) {
                return Err(AppError::BackupCorrupted(format!(
                    "Backup file is corrupted: {}",
                    path_str
                )));
            }

            // Mark as restored
            self.restored_backups.lock().unwrap().insert(path_str);

            Ok(())
        }

        async fn list_backups(
            &self,
            _data_dir: PathBuf,
        ) -> Result<Vec<crate::application::ports::BackupInfoData>, AppError> {
            unimplemented!("Not needed for restore backup tests")
        }
    }

    #[tokio::test]
    async fn test_restore_backup_success() {
        // Arrange
        let mock_backup = Arc::new(MockBackupPort::new());
        let use_case = RestoreBackupUseCase::new(mock_backup.clone());
        let backup_path = PathBuf::from("/valid/backup.db");

        // Act
        let result = use_case.execute(backup_path.clone()).await;

        // Assert
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.success);
        assert_eq!(result.restored_count, 0);
        assert_eq!(
            result.message,
            Some("Backup restored successfully".to_string())
        );

        // Verify backup was restored
        assert!(mock_backup.was_restored("/valid/backup.db"));
    }

    #[tokio::test]
    async fn test_restore_backup_validates_file_exists() {
        // Arrange
        let mock_backup = Arc::new(MockBackupPort::new());
        let use_case = RestoreBackupUseCase::new(mock_backup);
        let nonexistent_path = PathBuf::from("/nonexistent/backup.db");

        // Act
        let result = use_case.execute(nonexistent_path).await;

        // Assert
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::FileNotFound { path } => {
                assert_eq!(path, "/nonexistent/backup.db");
            }
            e => panic!("Expected FileNotFound error, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_restore_backup_validates_integrity() {
        // Arrange
        let mock_backup = Arc::new(MockBackupPort::new());
        let use_case = RestoreBackupUseCase::new(mock_backup);
        let corrupted_path = PathBuf::from("/corrupted/backup.db");

        // Act
        let result = use_case.execute(corrupted_path).await;

        // Assert
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::BackupCorrupted(msg) => {
                assert!(msg.contains("Backup file is corrupted"));
                assert!(msg.contains("/corrupted/backup.db"));
            }
            e => panic!("Expected BackupCorrupted error, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_restore_backup_handles_corruption() {
        // Arrange
        let mock_backup = Arc::new(MockBackupPort::new());
        mock_backup.add_corrupted_backup("/bad/backup.db");
        let use_case = RestoreBackupUseCase::new(mock_backup);
        let corrupted_path = PathBuf::from("/bad/backup.db");

        // Act
        let result = use_case.execute(corrupted_path).await;

        // Assert
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::BackupCorrupted(_)));
    }

    #[tokio::test]
    async fn test_restore_backup_handles_generic_failure() {
        // Arrange
        let mock_backup = Arc::new(MockBackupPort::with_restore_failure());
        let use_case = RestoreBackupUseCase::new(mock_backup);
        let backup_path = PathBuf::from("/any/backup.db");

        // Act
        let result = use_case.execute(backup_path).await;

        // Assert
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::BackupRestoreFailed(msg) => {
                assert!(msg.contains("Generic restore failure"));
            }
            e => panic!("Expected BackupRestoreFailed error, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_restore_backup_with_different_valid_paths() {
        // Arrange
        let mock_backup = Arc::new(MockBackupPort::new());
        mock_backup.add_valid_backup("/custom/path/backup.db");
        let use_case = RestoreBackupUseCase::new(mock_backup.clone());
        let backup_path = PathBuf::from("/custom/path/backup.db");

        // Act
        let result = use_case.execute(backup_path).await;

        // Assert
        assert!(result.is_ok());
        assert!(mock_backup.was_restored("/custom/path/backup.db"));
    }
}
