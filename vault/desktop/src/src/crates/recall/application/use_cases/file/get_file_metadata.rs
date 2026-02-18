//! Get File Metadata Use Case
//!
//! Retrieves metadata for a file.

use crate::application::dtos::file_dto::{FileMetadataDto, GetFileMetadataRequestDto};
use crate::application::ports::FileStoragePort;
use crate::infrastructure::security::FileAccessConfig;
use crate::shared::error::{AppError, Result};
use chrono::{DateTime, Utc};
use std::sync::Arc;

/// Use case for retrieving file metadata.
///
/// This use case:
/// 1. Validates the file path (security: CWE-22)
/// 2. Checks file existence
/// 3. Retrieves file metadata (size, modified time, permissions)
/// 4. Converts to DTO
pub struct GetFileMetadataUseCase {
    file_storage: Arc<dyn FileStoragePort>,
    file_access_config: Arc<FileAccessConfig>,
}

impl GetFileMetadataUseCase {
    /// Create a new use case instance.
    pub fn new(
        file_storage: Arc<dyn FileStoragePort>,
        file_access_config: Arc<FileAccessConfig>,
    ) -> Self {
        Self {
            file_storage,
            file_access_config,
        }
    }

    /// Execute the use case.
    ///
    /// # Arguments
    ///
    /// * `request` - Request containing the file path
    ///
    /// # Returns
    ///
    /// File metadata DTO.
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if file does not exist
    /// - `AppError::InvalidInput` if path is invalid
    /// - `AppError::Io` if metadata cannot be read
    pub async fn execute(&self, request: GetFileMetadataRequestDto) -> Result<FileMetadataDto> {
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

        // Get metadata from storage port
        let metadata = self.file_storage.metadata(&validated_path).await?;

        // Get file name
        let file_name = validated_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        // Detect MIME type
        let mime_type = Self::detect_mime_type(&validated_path);

        // Convert timestamp to ISO 8601
        let modified_at = DateTime::<Utc>::from_timestamp(metadata.modified_at, 0)
            .unwrap_or_else(Utc::now)
            .to_rfc3339();

        // Determine permissions (simplified - actual permissions may vary by OS)
        let is_readable = metadata.is_file;
        let is_writable = metadata.is_file; // Simplified - real check would use file permissions

        Ok(FileMetadataDto {
            file_name,
            mime_type,
            size_bytes: metadata.size as i64,
            modified_at,
            is_readable,
            is_writable,
            path: validated_path.to_string_lossy().to_string(),
        })
    }

    /// Detect MIME type from file extension.
    fn detect_mime_type(path: &std::path::Path) -> String {
        match path.extension().and_then(|e| e.to_str()) {
            Some("txt") => "text/plain",
            Some("pdf") => "application/pdf",
            Some("docx") => {
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            }
            Some("doc") => "application/msword",
            Some("md") => "text/markdown",
            Some("html") | Some("htm") => "text/html",
            Some("json") => "application/json",
            Some("xml") => "application/xml",
            Some("jpg") | Some("jpeg") => "image/jpeg",
            Some("png") => "image/png",
            Some("gif") => "image/gif",
            Some("mp4") => "video/mp4",
            Some("mp3") => "audio/mpeg",
            _ => "application/octet-stream",
        }
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::path::Path;

    struct MockFileStorage {
        existing_files: Vec<(String, u64, i64)>, // (path, size, modified_at)
    }

    impl MockFileStorage {
        fn new() -> Self {
            Self {
                existing_files: Vec::new(),
            }
        }

        fn with_file(mut self, path: &str, size: u64, modified_at: i64) -> Self {
            self.existing_files
                .push((path.to_string(), size, modified_at));
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
                .any(|(p, _, _)| p == &path_str || p == &path.display().to_string())
        }

        async fn metadata(&self, path: &Path) -> Result<crate::application::ports::FileMetadata> {
            let path_str = path.to_string_lossy().to_string();
            self.existing_files
                .iter()
                .find(|(p, _, _)| p == &path_str || p == &path.display().to_string())
                .map(
                    |(_, size, modified_at)| crate::application::ports::FileMetadata {
                        size: *size,
                        modified_at: *modified_at,
                        is_file: true,
                        is_directory: false,
                    },
                )
                .ok_or_else(|| {
                    crate::error::AppError::NotFound(format!("File not found: {}", path.display()))
                })
        }
    }

    #[tokio::test]
    async fn test_get_file_metadata_success() {
        use crate::infrastructure::security::FileAccessConfig;
        use std::env;

        let temp_dir = env::temp_dir();
        let test_file = temp_dir.join("document_metadata_test.pdf");
        std::fs::write(&test_file, "test pdf content").ok();

        let canonical_path = test_file.canonicalize().unwrap_or(test_file.clone());
        let file_access_config = Arc::new(FileAccessConfig::new(vec![temp_dir.clone()]));
        let file_storage = Arc::new(
            MockFileStorage::new().with_file(&canonical_path.to_string_lossy(), 1024, 1609459200), // 2021-01-01
        );
        let use_case = GetFileMetadataUseCase::new(file_storage, file_access_config);

        let request = GetFileMetadataRequestDto {
            path: test_file.to_string_lossy().to_string(),
        };

        let result = use_case.execute(request).await;

        std::fs::remove_file(&test_file).ok();

        assert!(result.is_ok());
        let metadata = result.unwrap();
        assert_eq!(metadata.file_name, "document_metadata_test.pdf");
        assert_eq!(metadata.mime_type, "application/pdf");
        assert_eq!(metadata.size_bytes, 1024);
        assert!(metadata.is_readable);
        assert!(metadata.is_writable);
    }

    #[tokio::test]
    async fn test_get_file_metadata_file_not_found() {
        use crate::infrastructure::security::FileAccessConfig;
        use std::env;

        let temp_dir = env::temp_dir();
        let file_access_config = Arc::new(FileAccessConfig::new(vec![temp_dir.clone()]));
        let file_storage = Arc::new(MockFileStorage::new()); // No files
        let use_case = GetFileMetadataUseCase::new(file_storage, file_access_config);

        let nonexistent = temp_dir.join("nonexistent_metadata_test.txt");
        let request = GetFileMetadataRequestDto {
            path: nonexistent.to_string_lossy().to_string(),
        };

        let result = use_case.execute(request).await;

        assert!(result.is_err());
        assert!(matches!(result, Err(crate::error::AppError::NotFound(_))));
    }

    #[test]
    fn test_mime_type_detection() {
        use std::path::Path;

        assert_eq!(
            GetFileMetadataUseCase::detect_mime_type(Path::new("test.txt")),
            "text/plain"
        );
        assert_eq!(
            GetFileMetadataUseCase::detect_mime_type(Path::new("test.pdf")),
            "application/pdf"
        );
        assert_eq!(
            GetFileMetadataUseCase::detect_mime_type(Path::new("test.unknown")),
            "application/octet-stream"
        );
    }
}
