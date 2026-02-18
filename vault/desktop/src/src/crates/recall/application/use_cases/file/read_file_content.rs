//! Read File Content Use Case
//!
//! Reads the content of a text file with security limits.

use crate::application::dtos::file_dto::{FileContentDto, ReadFileContentRequestDto};
use crate::application::ports::FileStoragePort;
use crate::infrastructure::indexing::extraction::ContentExtractor;
use crate::infrastructure::security::FileAccessConfig;
use crate::shared::error::{AppError, Result};
use std::path::Path;
use std::sync::Arc;

/// Maximum file size for reading (10 MB)
///
/// This limit prevents:
/// - Memory exhaustion attacks (CWE-400)
/// - Denial of service via large file reads (CWE-770)
const MAX_FILE_SIZE_BYTES: u64 = 10 * 1024 * 1024; // 10 MB

/// Use case for reading file content as text.
///
/// This use case:
/// 1. Validates the file path (security: CWE-22)
/// 2. Checks file existence
/// 3. Validates file size (security: prevents DoS)
/// 4. Reads file content as UTF-8 text
/// 5. Returns content with metadata
///
/// # Security
///
/// - Path validation prevents directory traversal (CWE-22)
/// - File size limit prevents resource exhaustion (CWE-770, CWE-400)
/// - UTF-8 validation ensures safe text processing
pub struct ReadFileContentUseCase {
    file_storage: Arc<dyn FileStoragePort>,
    file_access_config: Arc<FileAccessConfig>,
}

impl ReadFileContentUseCase {
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
    /// File content DTO with text content and metadata.
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if file does not exist
    /// - `AppError::InvalidInput` if path is invalid or is a directory
    /// - `AppError::FileTooLarge` if file exceeds size limit
    /// - `AppError::Io` if file cannot be read or is not valid UTF-8
    pub async fn execute(&self, request: ReadFileContentRequestDto) -> Result<FileContentDto> {
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

        // Get metadata to check size
        let metadata = self.file_storage.metadata(&validated_path).await?;

        // Verify it's a file, not a directory
        if metadata.is_directory {
            return Err(AppError::InvalidInput(format!(
                "Cannot read directory as file: {}",
                validated_path.display()
            )));
        }

        // Check file size limit (security: prevent DoS)
        if metadata.size > MAX_FILE_SIZE_BYTES {
            return Err(AppError::FileTooLarge {
                path: validated_path.display().to_string(),
                size_bytes: metadata.size,
                max_size_bytes: MAX_FILE_SIZE_BYTES,
            });
        }

        // Read file content (using validated path). Fall back to structured extraction
        // for supported document formats when raw UTF-8 read fails.
        let content = match self.file_storage.read_file(&validated_path).await {
            Ok(content) => content,
            Err(read_error) => {
                if let Some(extracted) = self.try_extract_preview_content(&validated_path).await {
                    extracted
                } else {
                    return Err(read_error);
                }
            }
        };

        Ok(FileContentDto {
            path: validated_path.to_string_lossy().to_string(),
            content,
            size_bytes: metadata.size as i64,
            encoding: "UTF-8".to_string(),
        })
    }

    async fn try_extract_preview_content(&self, path: &Path) -> Option<String> {
        let extractor = ContentExtractor::with_max_size(MAX_FILE_SIZE_BYTES);

        if !extractor.is_supported(path) {
            return None;
        }

        match extractor.extract_from_file(path).await {
            Ok(extracted) => Some(extracted.text),
            Err(err) => {
                tracing::debug!(
                    path = %path.display(),
                    error = %err,
                    "Structured extraction fallback failed for preview content"
                );
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::path::Path;

    struct MockFileStorage {
        files: Vec<(String, String, u64)>, // (path, content, size)
        force_read_error: bool,
    }

    impl MockFileStorage {
        fn new() -> Self {
            Self {
                files: Vec::new(),
                force_read_error: false,
            }
        }

        fn with_file(mut self, path: &str, content: &str) -> Self {
            let size = content.len() as u64;
            self.files
                .push((path.to_string(), content.to_string(), size));
            self
        }

        fn with_large_file(mut self, path: &str) -> Self {
            // Create a file larger than MAX_FILE_SIZE_BYTES
            let size = MAX_FILE_SIZE_BYTES + 1;
            self.files.push((path.to_string(), String::new(), size));
            self
        }

        fn with_directory(mut self, path: &str) -> Self {
            let dir_path = if path.ends_with('/') {
                path.to_string()
            } else {
                format!("{}/", path)
            };
            self.files.push((dir_path, String::new(), 0));
            self
        }

        fn with_read_error(mut self) -> Self {
            self.force_read_error = true;
            self
        }
    }

    #[async_trait]
    impl FileStoragePort for MockFileStorage {
        async fn read_file(&self, path: &Path) -> Result<String> {
            if self.force_read_error {
                return Err(AppError::FileRead {
                    path: path.to_string_lossy().to_string(),
                    reason: "forced read failure".to_string(),
                });
            }

            let path_str = path.to_string_lossy().to_string();
            self.files
                .iter()
                .find(|(p, _, _)| p == &path_str || p == &path.display().to_string())
                .map(|(_, content, _)| content.clone())
                .ok_or_else(|| AppError::NotFound(format!("File not found: {}", path.display())))
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
            let path_with_slash = format!("{}/", path_str);
            self.files.iter().any(|(p, _, _)| {
                p == &path_str || p == &path.display().to_string() || p == &path_with_slash
            })
        }

        async fn metadata(&self, path: &Path) -> Result<crate::application::ports::FileMetadata> {
            let path_str = path.to_string_lossy().to_string();
            let path_with_slash = format!("{}/", path_str);
            self.files
                .iter()
                .find(|(p, _, _)| {
                    p == &path_str || p == &path.display().to_string() || p == &path_with_slash
                })
                .map(|(p, _, size)| {
                    // Check if it's a directory (marked with trailing slash)
                    let is_directory = p.ends_with('/');

                    crate::application::ports::FileMetadata {
                        size: *size,
                        modified_at: 0,
                        is_file: !is_directory,
                        is_directory,
                    }
                })
                .ok_or_else(|| AppError::NotFound(format!("File not found: {}", path.display())))
        }
    }

    #[tokio::test]
    async fn test_read_file_content_success() {
        use crate::infrastructure::security::FileAccessConfig;
        use std::env;

        let temp_dir = env::temp_dir();
        let test_file = temp_dir.join("test_read_content.txt");
        std::fs::write(&test_file, "Hello, World!").ok();

        let canonical_path = test_file.canonicalize().unwrap_or(test_file.clone());
        let file_access_config = Arc::new(FileAccessConfig::new(vec![temp_dir.clone()]));
        let file_storage = Arc::new(
            MockFileStorage::new().with_file(&canonical_path.to_string_lossy(), "Hello, World!"),
        );
        let use_case = ReadFileContentUseCase::new(file_storage, file_access_config);

        let request = ReadFileContentRequestDto {
            path: test_file.to_string_lossy().to_string(),
        };

        let result = use_case.execute(request).await;

        std::fs::remove_file(&test_file).ok();

        assert!(result.is_ok());
        let content_dto = result.unwrap();
        assert_eq!(content_dto.content, "Hello, World!");
        assert_eq!(content_dto.size_bytes, 13);
        assert_eq!(content_dto.encoding, "UTF-8");
    }

    #[tokio::test]
    async fn test_read_file_content_file_not_found() {
        use crate::infrastructure::security::FileAccessConfig;
        use std::env;

        let temp_dir = env::temp_dir();
        let file_access_config = Arc::new(FileAccessConfig::new(vec![temp_dir.clone()]));
        let file_storage = Arc::new(MockFileStorage::new()); // No files
        let use_case = ReadFileContentUseCase::new(file_storage, file_access_config);

        let nonexistent = temp_dir.join("nonexistent_read_test.txt");
        let request = ReadFileContentRequestDto {
            path: nonexistent.to_string_lossy().to_string(),
        };

        let result = use_case.execute(request).await;

        assert!(result.is_err());
        assert!(matches!(result, Err(AppError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_read_file_content_file_too_large() {
        use crate::infrastructure::security::FileAccessConfig;
        use std::env;

        let temp_dir = env::temp_dir();
        let test_file = temp_dir.join("large_test.txt");
        std::fs::write(&test_file, "").ok();

        let canonical_path = test_file.canonicalize().unwrap_or(test_file.clone());
        let file_access_config = Arc::new(FileAccessConfig::new(vec![temp_dir.clone()]));
        let file_storage =
            Arc::new(MockFileStorage::new().with_large_file(&canonical_path.to_string_lossy()));
        let use_case = ReadFileContentUseCase::new(file_storage, file_access_config);

        let request = ReadFileContentRequestDto {
            path: test_file.to_string_lossy().to_string(),
        };

        let result = use_case.execute(request).await;

        std::fs::remove_file(&test_file).ok();

        assert!(result.is_err());
        assert!(matches!(result, Err(AppError::FileTooLarge { .. })));
    }

    #[tokio::test]
    async fn test_read_directory_fails() {
        use crate::infrastructure::security::FileAccessConfig;
        use std::env;

        let temp_dir = env::temp_dir();
        let test_dir = temp_dir.join("test_read_dir");
        std::fs::create_dir_all(&test_dir).ok();

        let canonical_path = test_dir.canonicalize().unwrap_or(test_dir.clone());
        let file_access_config = Arc::new(FileAccessConfig::new(vec![temp_dir.clone()]));
        let file_storage =
            Arc::new(MockFileStorage::new().with_directory(&canonical_path.to_string_lossy()));
        let use_case = ReadFileContentUseCase::new(file_storage, file_access_config);

        let request = ReadFileContentRequestDto {
            path: test_dir.to_string_lossy().to_string(),
        };

        let result = use_case.execute(request).await;

        std::fs::remove_dir(&test_dir).ok();

        assert!(result.is_err());
        assert!(matches!(result, Err(AppError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn test_read_file_content_falls_back_to_structured_extraction() {
        use crate::infrastructure::security::FileAccessConfig;
        use std::env;

        let temp_dir = env::temp_dir();
        let test_file = temp_dir.join("test_read_content_fallback.md");
        std::fs::write(&test_file, "# Header\n\nFallback extraction works.").ok();

        let canonical_path = test_file.canonicalize().unwrap_or(test_file.clone());
        let file_access_config = Arc::new(FileAccessConfig::new(vec![temp_dir.clone()]));
        let file_storage = Arc::new(
            MockFileStorage::new()
                .with_file(
                    &canonical_path.to_string_lossy(),
                    "# Header\n\nFallback extraction works.",
                )
                .with_read_error(),
        );
        let use_case = ReadFileContentUseCase::new(file_storage, file_access_config);

        let request = ReadFileContentRequestDto {
            path: test_file.to_string_lossy().to_string(),
        };

        let result = use_case.execute(request).await;

        std::fs::remove_file(&test_file).ok();

        assert!(result.is_ok());
        let content_dto = result.unwrap();
        assert!(content_dto.content.contains("Fallback extraction works."));
    }
}
