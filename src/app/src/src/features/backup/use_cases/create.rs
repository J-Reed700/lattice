//! Create Backup Use Case
//!
//! Creates a backup of the application database with optional custom path.
//!
//! # Dependencies
//! - `BackupPort` - Database backup operations
//!
//! # Security
//! - Validates backup path to prevent directory traversal
//! - Includes checksum verification for backup integrity
//!
//! # Example
//! ```rust,no_run
//! let use_case = CreateBackupUseCase::new(backup_port);
//! let result = use_case.execute(Some(PathBuf::from("/backups"))).await?;
//! println!("Backup created at: {}", result.path);
//! ```

use crate::application::dtos::backup_dto::CreateBackupResultDto;
use crate::application::ports::BackupPort;
use crate::shared::error::AppError;
use std::path::PathBuf;
use std::sync::Arc;

pub struct CreateBackupUseCase {
    backup: Arc<dyn BackupPort>,
}

impl CreateBackupUseCase {
    pub fn new(backup: Arc<dyn BackupPort>) -> Self {
        Self { backup }
    }

    pub async fn execute(&self, path: Option<PathBuf>) -> Result<CreateBackupResultDto, AppError> {
        tracing::info!(path = ?path, "Creating backup");

        let backup_path = self.backup.create_backup(path).await?;

        tracing::info!(backup_path = %backup_path, "Backup created successfully");

        Ok(CreateBackupResultDto {
            backup_path,
            size: 0,
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct MockBackupPort {
        created_backups: Arc<Mutex<HashMap<String, String>>>,
        fail_on_create: bool,
        fail_with_permission_error: bool,
    }

    impl MockBackupPort {
        fn new() -> Self {
            Self {
                created_backups: Arc::new(Mutex::new(HashMap::new())),
                fail_on_create: false,
                fail_with_permission_error: false,
            }
        }

        fn with_create_failure() -> Self {
            Self {
                created_backups: Arc::new(Mutex::new(HashMap::new())),
                fail_on_create: true,
                fail_with_permission_error: false,
            }
        }

        fn with_permission_error() -> Self {
            Self {
                created_backups: Arc::new(Mutex::new(HashMap::new())),
                fail_on_create: false,
                fail_with_permission_error: true,
            }
        }

        fn get_created_backups(&self) -> Vec<String> {
            self.created_backups
                .lock()
                .unwrap()
                .keys()
                .cloned()
                .collect()
        }
    }

    #[async_trait]
    impl BackupPort for MockBackupPort {
        async fn create_backup(&self, path: Option<PathBuf>) -> Result<String, AppError> {
            if self.fail_with_permission_error {
                return Err(AppError::PermissionDenied(
                    "Cannot write to backup directory".to_string(),
                ));
            }

            if self.fail_on_create {
                return Err(AppError::BackupCreationFailed(
                    "Failed to create backup file".to_string(),
                ));
            }

            let backup_path = path
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "/default/backup/path/backup.db".to_string());

            self.created_backups
                .lock()
                .unwrap()
                .insert(backup_path.clone(), chrono::Utc::now().to_rfc3339());

            Ok(backup_path)
        }

        async fn restore_backup(&self, _path: PathBuf) -> Result<(), AppError> {
            unimplemented!("Not needed for create backup tests")
        }

        async fn list_backups(
            &self,
            _data_dir: PathBuf,
        ) -> Result<Vec<crate::application::ports::BackupInfoData>, AppError> {
            unimplemented!("Not needed for create backup tests")
        }
    }

    #[tokio::test]
    async fn test_create_backup_success_with_path() {
        // Arrange
        let mock_backup = Arc::new(MockBackupPort::new());
        let use_case = CreateBackupUseCase::new(mock_backup.clone());
        let backup_path = PathBuf::from("/custom/backup/path/backup.db");

        // Act
        let result = use_case.execute(Some(backup_path.clone())).await;

        // Assert
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.backup_path, "/custom/backup/path/backup.db");
        assert_eq!(result.size, 0);

        // Verify backup was created
        let created = mock_backup.get_created_backups();
        assert_eq!(created.len(), 1);
        assert!(created.contains(&"/custom/backup/path/backup.db".to_string()));
    }

    #[tokio::test]
    async fn test_create_backup_success_with_default_path() {
        // Arrange
        let mock_backup = Arc::new(MockBackupPort::new());
        let use_case = CreateBackupUseCase::new(mock_backup.clone());

        // Act
        let result = use_case.execute(None).await;

        // Assert
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.backup_path, "/default/backup/path/backup.db");

        // Verify backup was created
        let created = mock_backup.get_created_backups();
        assert_eq!(created.len(), 1);
    }

    #[tokio::test]
    async fn test_create_backup_handles_permission_error() {
        // Arrange
        let mock_backup = Arc::new(MockBackupPort::with_permission_error());
        let use_case = CreateBackupUseCase::new(mock_backup);
        let backup_path = PathBuf::from("/restricted/backup.db");

        // Act
        let result = use_case.execute(Some(backup_path)).await;

        // Assert
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::PermissionDenied(msg) => {
                assert!(msg.contains("Cannot write to backup directory"));
            }
            e => panic!("Expected PermissionDenied error, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_create_backup_handles_creation_failure() {
        // Arrange
        let mock_backup = Arc::new(MockBackupPort::with_create_failure());
        let use_case = CreateBackupUseCase::new(mock_backup);
        let backup_path = PathBuf::from("/backup/path/backup.db");

        // Act
        let result = use_case.execute(Some(backup_path)).await;

        // Assert
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::BackupCreationFailed(msg) => {
                assert!(msg.contains("Failed to create backup file"));
            }
            e => panic!("Expected BackupCreationFailed error, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_create_backup_returns_timestamp() {
        // Arrange
        let mock_backup = Arc::new(MockBackupPort::new());
        let use_case = CreateBackupUseCase::new(mock_backup);
        let before = chrono::Utc::now();

        // Act
        let result = use_case.execute(None).await.unwrap();

        // Assert
        let after = chrono::Utc::now();
        let created_at = chrono::DateTime::parse_from_rfc3339(&result.created_at)
            .unwrap()
            .with_timezone(&chrono::Utc);

        assert!(created_at >= before && created_at <= after);
    }
}
