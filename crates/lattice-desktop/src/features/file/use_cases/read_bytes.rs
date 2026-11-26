//! Read File Bytes Use Case
//!
//! Reads the content of a file as raw bytes with memory-based admission.

use crate::features::file::dto::ReadFileBytesRequestDto;
use crate::infrastructure::security::FileAccessConfig;
use crate::infrastructure::services::memory_admission::{
    check_read_bytes_admission, MemoryProbe, SysinfoMemoryProbe,
};
use lattice_core::application::ports::FileStoragePort;
use lattice_core::shared::error::{AppError, Result};
use std::sync::Arc;

/// Use case for reading file content as bytes.
///
/// This use case:
/// 1. Validates the file path (security: CWE-22)
/// 2. Checks file existence
/// 3. Checks available system memory headroom for the binary IPC transfer (CWE-400 / CWE-770)
/// 4. Reads file content as bytes
///
/// # Security
///
/// - Path validation prevents directory traversal (CWE-22)
/// - Memory-based admission prevents resource exhaustion (CWE-770, CWE-400)
pub struct ReadFileBytesUseCase {
    file_storage: Arc<dyn FileStoragePort>,
    file_access_config: Arc<FileAccessConfig>,
    memory_probe: Arc<dyn MemoryProbe>,
}

impl ReadFileBytesUseCase {
    /// Create a new use case instance.
    pub fn new(
        file_storage: Arc<dyn FileStoragePort>,
        file_access_config: Arc<FileAccessConfig>,
    ) -> Self {
        Self {
            file_storage,
            file_access_config,
            memory_probe: Arc::new(SysinfoMemoryProbe),
        }
    }

    /// Override the memory probe (primarily for deterministic testing).
    #[must_use]
    pub fn with_memory_probe(mut self, memory_probe: Arc<dyn MemoryProbe>) -> Self {
        self.memory_probe = memory_probe;
        self
    }

    /// Execute the use case.
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if file does not exist
    /// - `AppError::InvalidInput` if path is invalid or is a directory
    /// - `AppError::FileTooLarge` if file exceeds available-memory headroom
    /// - `AppError::Io` if file cannot be read
    pub async fn execute(&self, request: ReadFileBytesRequestDto) -> Result<Vec<u8>> {
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

        let metadata = self.file_storage.metadata(&validated_path).await?;

        if metadata.is_directory {
            return Err(AppError::InvalidInput(format!(
                "Cannot read directory as file: {}",
                validated_path.display()
            )));
        }

        // Check memory admission against measured available OS memory (CWE-400 / CWE-770)
        let decision =
            check_read_bytes_admission(metadata.size, self.memory_probe.available_memory_bytes());
        if !decision.admitted {
            return Err(AppError::FileTooLarge {
                path: validated_path.display().to_string(),
                size_bytes: metadata.size,
                max_size_bytes: decision.max_input_bytes.unwrap_or(0),
            });
        }

        self.file_storage.read_file_bytes(&validated_path).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::path::Path;

    struct MockFileStorage {
        files: Vec<(String, Vec<u8>, u64)>,
    }

    impl MockFileStorage {
        fn new() -> Self {
            Self { files: Vec::new() }
        }

        fn with_file(mut self, path: &str, content: &[u8]) -> Self {
            let size = content.len() as u64;
            self.files.push((path.to_string(), content.to_vec(), size));
            self
        }

        fn with_sized_file(mut self, path: &str, content: &[u8], reported_size: u64) -> Self {
            self.files
                .push((path.to_string(), content.to_vec(), reported_size));
            self
        }
    }

    #[async_trait]
    impl FileStoragePort for MockFileStorage {
        async fn read_file(&self, _path: &Path) -> Result<String> {
            Ok(String::new())
        }

        async fn read_file_bytes(&self, path: &Path) -> Result<Vec<u8>> {
            let path_str = path.to_string_lossy().to_string();
            self.files
                .iter()
                .find(|(p, _, _)| p == &path_str || p == &path.display().to_string())
                .map(|(_, content, _)| content.clone())
                .ok_or_else(|| AppError::NotFound(format!("File not found: {}", path.display())))
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

        async fn metadata(
            &self,
            path: &Path,
        ) -> Result<lattice_core::application::ports::FileMetadata> {
            let path_str = path.to_string_lossy().to_string();
            let path_with_slash = format!("{}/", path_str);
            let now = std::time::SystemTime::now();
            let modified_at = now.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;

            self.files
                .iter()
                .find(|(p, _, _)| {
                    p == &path_str || p == &path.display().to_string() || p == &path_with_slash
                })
                .map(|(p, _, size)| {
                    let is_directory = p.ends_with('/');
                    Ok(lattice_core::application::ports::FileMetadata {
                        size: *size,
                        modified_at,
                        is_file: true,
                        is_directory,
                    })
                })
                .unwrap_or_else(|| {
                    Err(AppError::NotFound(format!(
                        "File not found: {}",
                        path.display()
                    )))
                })
        }
    }

    #[tokio::test]
    async fn test_read_file_bytes_success() {
        use crate::infrastructure::services::memory_admission::FixedMemoryProbe;

        let temp_root = std::env::temp_dir().canonicalize().unwrap();
        let file_path = temp_root.join("file.bin");
        let storage =
            MockFileStorage::new().with_file(file_path.to_string_lossy().as_ref(), b"hello");
        let access_config = Arc::new(FileAccessConfig::new(vec![temp_root]));
        let use_case = ReadFileBytesUseCase::new(Arc::new(storage), access_config)
            .with_memory_probe(Arc::new(FixedMemoryProbe(Some(64 * 1024 * 1024))));

        let result = use_case
            .execute(ReadFileBytesRequestDto {
                path: file_path.to_string_lossy().into_owned(),
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), b"hello".to_vec());
    }

    #[tokio::test]
    async fn test_read_file_bytes_admits_within_available_memory_and_refuses_above_headroom() {
        use crate::infrastructure::services::memory_admission::FixedMemoryProbe;

        const MIB: u64 = 1024 * 1024;
        let temp_root = std::env::temp_dir().canonicalize().unwrap();
        let large_pdf = temp_root.join("textbook.pdf");
        let oversized_bin = temp_root.join("oversized.bin");
        let storage = Arc::new(
            MockFileStorage::new()
                .with_sized_file(large_pdf.to_string_lossy().as_ref(), b"%PDF-1.7", 150 * MIB)
                .with_sized_file(oversized_bin.to_string_lossy().as_ref(), b"", 300 * MIB),
        );
        let access_config = Arc::new(FileAccessConfig::new(vec![temp_root]));

        // 1 GiB available -> 512 MiB headroom -> 256 MiB binary IPC limit (no 100 MiB cap).
        let use_case = ReadFileBytesUseCase::new(storage, access_config)
            .with_memory_probe(Arc::new(FixedMemoryProbe(Some(1024 * MIB))));

        let allowed = use_case
            .execute(ReadFileBytesRequestDto {
                path: large_pdf.to_string_lossy().into_owned(),
            })
            .await;
        assert_eq!(allowed.unwrap(), b"%PDF-1.7".to_vec());

        let rejected = use_case
            .execute(ReadFileBytesRequestDto {
                path: oversized_bin.to_string_lossy().into_owned(),
            })
            .await;
        assert!(matches!(
            rejected,
            Err(AppError::FileTooLarge {
                max_size_bytes,
                ..
            }) if max_size_bytes == 256 * MIB
        ));
    }
}
