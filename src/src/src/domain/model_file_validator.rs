//! Model File Validator Service
//!
//! Validates that downloaded models contain valid model files.

use crate::domain::ports::file_access::{ChecksumService, FileSystemAccess};
use crate::shared::error::AppError;
use std::path::Path;
use std::sync::Arc;

/// File expectation for verification
#[derive(Debug, Clone)]
pub struct FileExpectation {
    pub filename: String,
    pub size_bytes: Option<u64>,
    pub checksum_sha256: Option<String>,
}

impl FileExpectation {
    pub fn new(filename: String) -> Self {
        Self {
            filename,
            size_bytes: None,
            checksum_sha256: None,
        }
    }

    pub fn with_size(mut self, size: u64) -> Self {
        self.size_bytes = Some(size);
        self
    }

    pub fn with_checksum(mut self, checksum: String) -> Self {
        self.checksum_sha256 = Some(checksum);
        self
    }
}

/// Service for validating model file presence and validity.
///
/// Checks that a model directory contains at least one valid model file
/// (`.gguf` or `.bin`) with a minimum size.
///
/// # Example
///
/// ```rust,no_run
/// use lattice::domain::model_file_validator::ModelFileValidator;
/// use std::path::Path;
///
/// let validator = ModelFileValidator::new();
/// let model_dir = Path::new("/path/to/model");
///
/// if validator.has_valid_model_files(model_dir) {
///     println!("Model is valid");
/// }
/// ```
pub struct ModelFileValidator {
    min_file_size: u64,
    checksum_service: Arc<dyn ChecksumService>,
    file_system: Arc<dyn FileSystemAccess>,
}

impl ModelFileValidator {
    /// Minimum file size for valid model files (1KB)
    const MIN_FILE_SIZE: u64 = 1024;

    /// Create a new model file validator with dependency injection.
    ///
    /// # Arguments
    ///
    /// * `checksum_service` - Service for calculating file checksums
    /// * `file_system` - Service for file system operations
    pub fn new(
        checksum_service: Arc<dyn ChecksumService>,
        file_system: Arc<dyn FileSystemAccess>,
    ) -> Self {
        Self {
            min_file_size: Self::MIN_FILE_SIZE,
            checksum_service,
            file_system,
        }
    }

    /// Check if directory contains valid model files.
    ///
    /// A directory is considered valid if it contains at least one:
    /// - `.gguf` file (GGUF format)
    /// - `.bin` file (legacy format)
    /// - `.onnx` file (ONNX format)
    /// - `.onnx_data` file (ONNX data)
    /// - `.json` file (model config)
    /// - `.model` file (tokenizer model, e.g., .bpe.model)
    ///
    /// Files must be ≥ 1KB in size.
    ///
    /// # Arguments
    ///
    /// * `model_dir` - Directory to check
    ///
    /// # Returns
    ///
    /// * `true` - Directory contains valid model files
    /// * `false` - Directory doesn't exist, is empty, or has no valid files
    pub async fn has_valid_model_files(&self, model_dir: &Path) -> bool {
        if !self.file_system.exists(model_dir).await.unwrap_or(false) {
            return false;
        }

        if !self
            .file_system
            .is_directory(model_dir)
            .await
            .unwrap_or(false)
        {
            return false;
        }

        let Ok(entries) = self.file_system.read_directory(model_dir).await else {
            return false;
        };

        for entry in entries {
            if !entry.is_file {
                continue;
            }

            if self.is_valid_model_file(&entry.path, entry.size).await {
                return true;
            }
        }

        false
    }

    /// Verify all expected files are present and match expected sizes/checksums
    pub async fn verify_expected_files(
        &self,
        model_dir: &Path,
        expected_files: &[FileExpectation],
    ) -> Result<bool, AppError> {
        use tracing::{debug, warn};

        if !self.file_system.exists(model_dir).await? {
            debug!(
                path = %model_dir.display(),
                "Model directory does not exist"
            );
            return Ok(false);
        }

        if !self.file_system.is_directory(model_dir).await? {
            debug!(
                path = %model_dir.display(),
                "Path exists but is not a directory"
            );
            return Ok(false);
        }

        for expectation in expected_files {
            if expectation.filename.starts_with('*') {
                if !self.has_valid_model_files(model_dir).await {
                    debug!(
                        path = %model_dir.display(),
                        pattern = %expectation.filename,
                        "No valid model files found matching wildcard pattern"
                    );
                    return Ok(false);
                }
                continue;
            }

            let file_path = model_dir.join(&expectation.filename);

            if !self.file_system.exists(&file_path).await? {
                debug!(
                    file = %expectation.filename,
                    path = %file_path.display(),
                    "Expected file is missing"
                );
                return Ok(false);
            }

            if let Some(expected_size) = expectation.size_bytes {
                let actual_size = self.file_system.file_size(&file_path).await?;
                if actual_size != expected_size {
                    warn!(
                        file = %expectation.filename,
                        expected_size = expected_size,
                        actual_size = actual_size,
                        "File size mismatch"
                    );
                    return Ok(false);
                }
            }

            if let Some(expected_checksum) = &expectation.checksum_sha256 {
                let actual_checksum = self.checksum_service.calculate_sha256(&file_path).await?;
                if actual_checksum != *expected_checksum {
                    warn!(
                        file = %expectation.filename,
                        expected = %expected_checksum,
                        actual = %actual_checksum,
                        "Checksum mismatch"
                    );
                    return Ok(false);
                }
            }

            debug!(
                file = %expectation.filename,
                "File verified successfully"
            );
        }

        Ok(true)
    }

    /// Check if a specific file is a valid model file.
    ///
    /// # Arguments
    ///
    /// * `file_path` - File to check
    /// * `file_size` - File size in bytes
    ///
    /// # Returns
    ///
    /// * `true` - File is a valid model file (`.gguf`, `.bin`, `.onnx`, `.onnx_data`, `.json`, or `.model` with size ≥ 1KB)
    /// * `false` - File is invalid or doesn't meet criteria
    async fn is_valid_model_file(&self, file_path: &Path, file_size: u64) -> bool {
        // Check extension
        let Some(extension) = file_path.extension() else {
            return false;
        };

        let ext_str = extension.to_string_lossy().to_lowercase();
        if !matches!(
            ext_str.as_str(),
            "gguf" | "bin" | "onnx" | "onnx_data" | "json" | "model"
        ) {
            return false;
        }

        // Check file size
        file_size >= self.min_file_size
    }
}

// Note: Removed Default impl - requires explicit dependency injection

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::TempDir;

    use crate::domain::ports::file_access::DirectoryEntry;

    // ========================================================================
    // Mock ChecksumService for Testing
    // ========================================================================

    /// Mock implementation of ChecksumService for testing.
    /// Returns pre-configured checksums based on file paths.
    #[derive(Clone)]
    struct MockChecksumService {
        checksums: HashMap<PathBuf, String>,
    }

    impl MockChecksumService {
        fn new() -> Self {
            Self {
                checksums: HashMap::new(),
            }
        }

        fn with_checksum(mut self, path: PathBuf, checksum: String) -> Self {
            self.checksums.insert(path, checksum);
            self
        }
    }

    #[async_trait::async_trait]
    impl ChecksumService for MockChecksumService {
        async fn calculate_sha256(&self, file_path: &Path) -> Result<String, AppError> {
            self.checksums.get(file_path).cloned().ok_or_else(|| {
                AppError::FileSystem(format!(
                    "Mock checksum not configured for: {}",
                    file_path.display()
                ))
            })
        }
    }

    // ========================================================================
    // Mock FileSystemAccess for Testing
    // ========================================================================

    /// Mock implementation of FileSystemAccess for testing.
    /// Uses real filesystem operations internally but through the port interface.
    struct MockFileSystemAccess;

    #[async_trait::async_trait]
    impl FileSystemAccess for MockFileSystemAccess {
        async fn exists(&self, path: &Path) -> Result<bool, AppError> {
            Ok(path.exists())
        }

        async fn is_directory(&self, path: &Path) -> Result<bool, AppError> {
            Ok(path.is_dir())
        }

        async fn read_directory(&self, path: &Path) -> Result<Vec<DirectoryEntry>, AppError> {
            let mut entries = Vec::new();
            let read_dir = std::fs::read_dir(path)
                .map_err(|e| AppError::FileSystem(format!("Failed to read directory: {}", e)))?;

            for entry in read_dir.flatten() {
                let path = entry.path();
                let metadata = std::fs::metadata(&path)
                    .map_err(|e| AppError::FileSystem(format!("Failed to read metadata: {}", e)))?;

                entries.push(DirectoryEntry {
                    path,
                    is_file: metadata.is_file(),
                    size: metadata.len(),
                });
            }

            Ok(entries)
        }

        async fn file_size(&self, path: &Path) -> Result<u64, AppError> {
            let metadata = std::fs::metadata(path)
                .map_err(|e| AppError::FileSystem(format!("Failed to read file size: {}", e)))?;
            Ok(metadata.len())
        }
    }

    // ========================================================================
    // Tests - Using Mock for Domain Purity
    // ========================================================================

    #[tokio::test]
    async fn test_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let checksum_mock = Arc::new(MockChecksumService::new());
        let fs_mock = Arc::new(MockFileSystemAccess);
        let validator = ModelFileValidator::new(checksum_mock, fs_mock);

        assert!(!validator.has_valid_model_files(temp_dir.path()).await);
    }

    #[tokio::test]
    async fn test_nonexistent_directory() {
        let checksum_mock = Arc::new(MockChecksumService::new());
        let fs_mock = Arc::new(MockFileSystemAccess);
        let validator = ModelFileValidator::new(checksum_mock, fs_mock);
        let nonexistent = Path::new("/definitely/does/not/exist/12345");

        assert!(!validator.has_valid_model_files(nonexistent).await);
    }

    #[tokio::test]
    async fn test_valid_gguf_file() {
        let temp_dir = TempDir::new().unwrap();
        let model_file = temp_dir.path().join("model.gguf");

        // Create file with sufficient size
        let mut file = fs::File::create(&model_file).unwrap();
        file.write_all(&vec![0u8; 2048]).unwrap();

        let checksum_mock = Arc::new(MockChecksumService::new());
        let fs_mock = Arc::new(MockFileSystemAccess);
        let validator = ModelFileValidator::new(checksum_mock, fs_mock);
        assert!(validator.has_valid_model_files(temp_dir.path()).await);
    }

    #[tokio::test]
    async fn test_valid_bin_file() {
        let temp_dir = TempDir::new().unwrap();
        let model_file = temp_dir.path().join("model.bin");

        // Create file with sufficient size
        let mut file = fs::File::create(&model_file).unwrap();
        file.write_all(&vec![0u8; 2048]).unwrap();

        let checksum_mock = Arc::new(MockChecksumService::new());
        let fs_mock = Arc::new(MockFileSystemAccess);
        let validator = ModelFileValidator::new(checksum_mock, fs_mock);
        assert!(validator.has_valid_model_files(temp_dir.path()).await);
    }

    #[tokio::test]
    async fn test_file_too_small() {
        let temp_dir = TempDir::new().unwrap();
        let model_file = temp_dir.path().join("model.gguf");

        // Create file with insufficient size (< 1KB)
        let mut file = fs::File::create(&model_file).unwrap();
        file.write_all(&vec![0u8; 512]).unwrap();

        let checksum_mock = Arc::new(MockChecksumService::new());
        let fs_mock = Arc::new(MockFileSystemAccess);
        let validator = ModelFileValidator::new(checksum_mock, fs_mock);
        assert!(!validator.has_valid_model_files(temp_dir.path()).await);
    }

    #[tokio::test]
    async fn test_wrong_extension() {
        let temp_dir = TempDir::new().unwrap();
        let model_file = temp_dir.path().join("model.txt");

        // Create file with sufficient size but wrong extension
        let mut file = fs::File::create(&model_file).unwrap();
        file.write_all(&vec![0u8; 2048]).unwrap();

        let checksum_mock = Arc::new(MockChecksumService::new());
        let fs_mock = Arc::new(MockFileSystemAccess);
        let validator = ModelFileValidator::new(checksum_mock, fs_mock);
        assert!(!validator.has_valid_model_files(temp_dir.path()).await);
    }

    #[tokio::test]
    async fn test_multiple_files_with_one_valid() {
        let temp_dir = TempDir::new().unwrap();

        // Create invalid file
        let invalid_file = temp_dir.path().join("readme.txt");
        let mut file = fs::File::create(&invalid_file).unwrap();
        file.write_all(b"Some text").unwrap();

        // Create valid file
        let valid_file = temp_dir.path().join("model.gguf");
        let mut file = fs::File::create(&valid_file).unwrap();
        file.write_all(&vec![0u8; 2048]).unwrap();

        let checksum_mock = Arc::new(MockChecksumService::new());
        let fs_mock = Arc::new(MockFileSystemAccess);
        let validator = ModelFileValidator::new(checksum_mock, fs_mock);
        assert!(validator.has_valid_model_files(temp_dir.path()).await);
    }

    #[tokio::test]
    async fn test_verify_expected_files_with_checksum() {
        let temp_dir = TempDir::new().unwrap();
        let model_file = temp_dir.path().join("model.gguf");

        // Create file with sufficient size
        let mut file = fs::File::create(&model_file).unwrap();
        file.write_all(&vec![0u8; 2048]).unwrap();
        drop(file);

        // Configure mock to return specific checksum for this file
        let expected_checksum = "abc123def456".to_string();
        let checksum_mock = Arc::new(
            MockChecksumService::new().with_checksum(model_file.clone(), expected_checksum.clone()),
        );
        let fs_mock = Arc::new(MockFileSystemAccess);
        let validator = ModelFileValidator::new(checksum_mock, fs_mock);

        // Test with matching checksum
        let expectations = vec![FileExpectation::new("model.gguf".to_string())
            .with_size(2048)
            .with_checksum(expected_checksum.clone())];

        assert!(validator
            .verify_expected_files(temp_dir.path(), &expectations)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_verify_expected_files_checksum_mismatch() {
        let temp_dir = TempDir::new().unwrap();
        let model_file = temp_dir.path().join("model.gguf");

        // Create file with sufficient size
        let mut file = fs::File::create(&model_file).unwrap();
        file.write_all(&vec![0u8; 2048]).unwrap();
        drop(file);

        // Configure mock to return different checksum
        let actual_checksum = "abc123def456".to_string();
        let expected_checksum = "different_hash".to_string();
        let checksum_mock =
            Arc::new(MockChecksumService::new().with_checksum(model_file.clone(), actual_checksum));
        let fs_mock = Arc::new(MockFileSystemAccess);
        let validator = ModelFileValidator::new(checksum_mock, fs_mock);

        // Test with non-matching checksum
        let expectations = vec![FileExpectation::new("model.gguf".to_string())
            .with_size(2048)
            .with_checksum(expected_checksum)];

        assert!(!validator
            .verify_expected_files(temp_dir.path(), &expectations)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_verify_expected_files_wildcard() {
        let temp_dir = TempDir::new().unwrap();
        let model_file = temp_dir.path().join("model.gguf");

        // Create file with sufficient size
        let mut file = fs::File::create(&model_file).unwrap();
        file.write_all(&vec![0u8; 2048]).unwrap();
        drop(file);

        let checksum_mock = Arc::new(MockChecksumService::new());
        let fs_mock = Arc::new(MockFileSystemAccess);
        let validator = ModelFileValidator::new(checksum_mock, fs_mock);

        // Test wildcard pattern
        let expectations = vec![FileExpectation::new("*.gguf".to_string())];

        assert!(validator
            .verify_expected_files(temp_dir.path(), &expectations)
            .await
            .unwrap());
    }
}
