//! Show In Folder Use Case
//!
//! Reveals a file in the system's file explorer.

use crate::features::file::dto::{FileOperationSuccessDto, ShowInFolderRequestDto};
use crate::infrastructure::security::FileAccessConfig;
use lattice_core::application::ports::{FileStoragePort, FileSystemPort};
use lattice_core::shared::error::{AppError, Result};
use std::sync::Arc;

/// Use case for revealing a file in the system's file explorer.
///
/// This use case:
/// 1. Validates the file path (security: CWE-22)
/// Opens the system file manager with the file selected.
pub struct ShowInFolderUseCase {
    file_system: Arc<dyn FileSystemPort>,
    file_storage: Arc<dyn FileStoragePort>,
    file_access_config: Arc<FileAccessConfig>,
}

impl ShowInFolderUseCase {
    /// Create a new use case instance.
    pub fn new(
        file_system: Arc<dyn FileSystemPort>,
        file_storage: Arc<dyn FileStoragePort>,
        file_access_config: Arc<FileAccessConfig>,
    ) -> Self {
        Self {
            file_system,
            file_storage,
            file_access_config,
        }
    }

    /// Execute the use case to reveal a file in the system file manager.
    pub async fn execute(
        &self,
        request: ShowInFolderRequestDto,
    ) -> Result<FileOperationSuccessDto> {
        // CRITICAL: Validate path FIRST to prevent directory traversal (CWE-22)
        let validated_path = self
            .file_access_config
            .validate_path(&request.path)
            .map_err(|e| AppError::InvalidInput(format!("Invalid file path: {}", e)))?;

        if !self.file_storage.exists(&validated_path).await {
            return Err(AppError::NotFound(format!(
                "File not found: {}",
                validated_path.display()
            )));
        }

        // Show file in folder (secure)
        self.file_system.show_in_folder(&validated_path).await?;

        Ok(FileOperationSuccessDto {
            status: "success".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use lattice_core::application::ports::file_system_port::FileSystemPort;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    struct MockFileSystem {
        shown_files: Arc<Mutex<Vec<String>>>,
    }

    impl MockFileSystem {
        fn new() -> Self {
            Self {
                shown_files: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn get_shown_files(&self) -> Vec<String> {
            self.shown_files.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl FileSystemPort for MockFileSystem {
        async fn open_file(&self, _path: &Path) -> Result<()> {
            Ok(())
        }

        async fn show_in_folder(&self, path: &Path) -> Result<()> {
            self.shown_files
                .lock()
                .unwrap()
                .push(path.display().to_string());
            Ok(())
        }

        async fn file_exists(&self, _path: &Path) -> Result<bool> {
            Ok(true)
        }

        async fn is_directory(&self, _path: &Path) -> Result<bool> {
            Ok(false)
        }

        async fn create_directory_all(&self, _path: &Path) -> Result<()> {
            Ok(())
        }

        async fn list_directory(&self, _path: &Path) -> Result<Vec<std::path::PathBuf>> {
            Ok(Vec::new())
        }
    }

    struct MockFileStorage {
        existing_files: Vec<String>,
    }

    impl MockFileStorage {
        fn new() -> Self {
            Self {
                existing_files: Vec::new(),
            }
        }

        fn with_file(mut self, path: &str) -> Self {
            self.existing_files.push(path.to_string());
            self
        }
    }

    #[async_trait]
    impl lattice_core::application::ports::FileStoragePort for MockFileStorage {
        async fn read_file(&self, _path: &Path) -> Result<String> {
            Ok(String::new())
        }

        async fn read_file_bytes(&self, _path: &Path) -> Result<Vec<u8>> {
            Ok(Vec::new())
        }

        async fn write_file(&self, _path: &Path, _content: &str) -> Result<()> {
            Ok(())
        }

        async fn write_file_bytes(&self, _path: &Path, _content: &[u8]) -> Result<()> {
            Ok(())
        }

        async fn delete_file(&self, _path: &Path) -> Result<()> {
            Ok(())
        }

        async fn compute_hash(&self, _path: &Path) -> Result<String> {
            Ok("mock_hash".to_string())
        }

        async fn exists(&self, path: &Path) -> bool {
            let path_str = path.to_string_lossy().to_string();
            self.existing_files
                .iter()
                .any(|p| p == &path_str || p == &path.display().to_string())
        }

        async fn metadata(
            &self,
            _path: &Path,
        ) -> Result<lattice_core::application::ports::FileMetadata> {
            Ok(lattice_core::application::ports::FileMetadata {
                size: 0,
                modified_at: 0,
                is_file: true,
                is_directory: false,
            })
        }
    }

    #[tokio::test]
    async fn test_show_in_folder_success() {
        use crate::infrastructure::security::FileAccessConfig;
        use std::env;

        let temp_dir = env::temp_dir();
        let test_file = temp_dir.join("test_show_in_folder.txt");
        std::fs::write(&test_file, "test").ok();

        let canonical_path = test_file.canonicalize().unwrap_or(test_file.clone());
        let file_access_config = Arc::new(FileAccessConfig::new(vec![temp_dir.clone()]));
        let file_system = Arc::new(MockFileSystem::new());
        let file_storage =
            Arc::new(MockFileStorage::new().with_file(&canonical_path.to_string_lossy()));
        let use_case =
            ShowInFolderUseCase::new(file_system.clone(), file_storage, file_access_config);

        let request = ShowInFolderRequestDto {
            path: test_file.to_string_lossy().to_string(),
        };

        let result = use_case.execute(request).await;

        std::fs::remove_file(&test_file).ok();

        assert!(result.is_ok());
        assert_eq!(result.unwrap().status, "success");
        assert_eq!(file_system.get_shown_files().len(), 1);
    }

    #[tokio::test]
    async fn test_show_in_folder_file_not_found() {
        use crate::infrastructure::security::FileAccessConfig;
        use std::env;

        let temp_dir = env::temp_dir();
        let file_access_config = Arc::new(FileAccessConfig::new(vec![temp_dir.clone()]));
        let file_system = Arc::new(MockFileSystem::new());
        let file_storage = Arc::new(MockFileStorage::new()); // No files
        let use_case = ShowInFolderUseCase::new(file_system, file_storage, file_access_config);

        let nonexistent = temp_dir.join("nonexistent_show_test.txt");
        let request = ShowInFolderRequestDto {
            path: nonexistent.to_string_lossy().to_string(),
        };

        let result = use_case.execute(request).await;

        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(lattice_core::shared::error::AppError::NotFound(_))
        ));
    }
}
