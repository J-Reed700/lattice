//! Open File Use Case
//!
//! Opens a file with the system's default application.

use crate::application::ports::{FileStoragePort, FileSystemPort};
use crate::application::services::FileType;
use crate::features::file::dto::{OpenFileRequestDto, OpenFileResponseDto};
use crate::infrastructure::security::FileAccessConfig;
use crate::infrastructure::web::WebArticleDetector;
use crate::shared::error::{AppError, Result};
use std::sync::Arc;

/// Use case for opening a file with the system's default application.
///
/// This use case:
/// 1. Validates the file path (security: CWE-22)
/// 2. Checks file existence
/// 3. Verifies it's a file (not a directory)
/// 4. Opens it with the default application (security: CWE-78 prevention)
///
/// # Security
///
/// - Path validation prevents directory traversal (CWE-22)
/// - Uses secure system APIs via FileSystemPort (prevents CWE-78)
/// - Audits file access for security monitoring
pub struct OpenFileUseCase {
    file_system: Arc<dyn FileSystemPort>,
    file_storage: Arc<dyn FileStoragePort>,
    file_access_config: Arc<FileAccessConfig>,
}

impl OpenFileUseCase {
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

    /// Execute the use case.
    ///
    /// # Arguments
    ///
    /// * `request` - Request containing the file path to open
    ///
    /// # Returns
    ///
    /// Success DTO if file was opened successfully.
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if file does not exist
    /// - `AppError::InvalidInput` if path is invalid or is a directory
    /// - `AppError::Io` if file cannot be opened
    pub async fn execute(&self, request: OpenFileRequestDto) -> Result<OpenFileResponseDto> {
        // CRITICAL: Validate path FIRST to prevent directory traversal (CWE-22)
        let validated_path = self
            .file_access_config
            .validate_path(&request.path)
            .map_err(|e| AppError::InvalidInput(format!("Invalid file path: {}", e)))?;

        // Check file exists (using validated path)
        if !self.file_storage.exists(&validated_path).await {
            return Err(AppError::NotFound(format!(
                "File not found: {}",
                validated_path.display()
            )));
        }

        // Check if it's a directory
        if self.file_system.is_directory(&validated_path).await? {
            return Err(AppError::InvalidInput(format!(
                "Cannot open directories: {}",
                validated_path.display()
            )));
        }

        if WebArticleDetector::is_web_article(&validated_path) {
            let html_path = WebArticleDetector::get_html_path(&validated_path)
                .ok_or_else(|| AppError::NotFound("page.html not found".to_string()))?;

            let title = WebArticleDetector::get_title(&validated_path)
                .await
                .unwrap_or_else(|_| "Web Article".to_string());

            return Ok(OpenFileResponseDto::render_internal(
                html_path.to_string_lossy().to_string(),
                title,
            ));
        }

        self.file_system.open_file(&validated_path).await?;

        let file_type = FileType::from_path(&validated_path);

        Ok(OpenFileResponseDto::opened_external(
            validated_path.display().to_string(),
            file_type,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::file_system_port::FileSystemPort;
    use crate::application::ports::FileStoragePort;
    use async_trait::async_trait;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    struct MockFileSystem {
        opened_files: Arc<Mutex<Vec<String>>>,
        directory_paths: Vec<String>,
    }

    impl MockFileSystem {
        fn new() -> Self {
            Self {
                opened_files: Arc::new(Mutex::new(Vec::new())),
                directory_paths: Vec::new(),
            }
        }

        fn with_directory(mut self, path: &str) -> Self {
            self.directory_paths.push(path.to_string());
            self
        }

        fn get_opened_files(&self) -> Vec<String> {
            self.opened_files.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl FileSystemPort for MockFileSystem {
        async fn open_file(&self, path: &Path) -> Result<()> {
            self.opened_files
                .lock()
                .unwrap()
                .push(path.display().to_string());
            Ok(())
        }

        async fn show_in_folder(&self, _path: &Path) -> Result<()> {
            Ok(())
        }

        async fn file_exists(&self, _path: &Path) -> Result<bool> {
            Ok(true)
        }

        async fn is_directory(&self, path: &Path) -> Result<bool> {
            let path_str = path.to_string_lossy().to_string();
            Ok(self
                .directory_paths
                .iter()
                .any(|p| p == &path_str || p == &path.display().to_string()))
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
    impl FileStoragePort for MockFileStorage {
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

        async fn metadata(&self, _path: &Path) -> Result<crate::application::ports::FileMetadata> {
            Ok(crate::application::ports::FileMetadata {
                size: 0,
                modified_at: 0,
                is_file: true,
                is_directory: false,
            })
        }
    }

    #[tokio::test]
    async fn test_open_file_success() {
        use crate::infrastructure::security::FileAccessConfig;
        use std::env;

        let temp_dir = env::temp_dir();
        let test_file = temp_dir.join("test_open_file.txt");
        std::fs::write(&test_file, "test").ok();

        let canonical_path = test_file.canonicalize().unwrap_or(test_file.clone());
        let file_access_config = Arc::new(FileAccessConfig::new(vec![temp_dir.clone()]));
        let file_system = Arc::new(MockFileSystem::new());
        let file_storage =
            Arc::new(MockFileStorage::new().with_file(&canonical_path.to_string_lossy()));
        let use_case = OpenFileUseCase::new(file_system.clone(), file_storage, file_access_config);

        let request = OpenFileRequestDto {
            path: test_file.to_string_lossy().to_string(),
        };

        let result = use_case.execute(request).await;

        std::fs::remove_file(&test_file).ok();

        assert!(result.is_ok());
        assert_eq!(result.unwrap().action, "opened_external");
        assert_eq!(file_system.get_opened_files().len(), 1);
    }

    #[tokio::test]
    async fn test_open_file_not_found() {
        use crate::infrastructure::security::FileAccessConfig;
        use std::env;

        let temp_dir = env::temp_dir();
        let file_access_config = Arc::new(FileAccessConfig::new(vec![temp_dir.clone()]));
        let file_system = Arc::new(MockFileSystem::new());
        let file_storage = Arc::new(MockFileStorage::new()); // No files
        let use_case = OpenFileUseCase::new(file_system, file_storage, file_access_config);

        let nonexistent = temp_dir.join("nonexistent_test_file_xyz.txt");
        let request = OpenFileRequestDto {
            path: nonexistent.to_string_lossy().to_string(),
        };

        let result = use_case.execute(request).await;

        assert!(result.is_err());
        assert!(matches!(result, Err(crate::error::AppError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_open_directory_fails() {
        use crate::infrastructure::security::FileAccessConfig;
        use std::env;

        let temp_dir = env::temp_dir();
        let test_dir = temp_dir.join("test_open_dir");
        std::fs::create_dir_all(&test_dir).ok();

        let canonical_path = test_dir.canonicalize().unwrap_or(test_dir.clone());
        let file_access_config = Arc::new(FileAccessConfig::new(vec![temp_dir.clone()]));
        let file_system =
            Arc::new(MockFileSystem::new().with_directory(&canonical_path.to_string_lossy()));
        let file_storage =
            Arc::new(MockFileStorage::new().with_file(&canonical_path.to_string_lossy()));
        let use_case = OpenFileUseCase::new(file_system, file_storage, file_access_config);

        let request = OpenFileRequestDto {
            path: test_dir.to_string_lossy().to_string(),
        };

        let result = use_case.execute(request).await;

        std::fs::remove_dir(&test_dir).ok();

        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(crate::error::AppError::InvalidInput(_))
        ));
    }
}
