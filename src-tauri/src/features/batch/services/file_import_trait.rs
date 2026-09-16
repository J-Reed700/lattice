//! Batch File Import Service Trait
//!
//! Defines the contract for batch file import operations.
//! This trait is the "stud" (public interface) that different implementations can connect to.

use crate::application::factories::FileMetadataFactory;
use crate::domain::value_objects::file_metadata::FileMetadata;
use crate::shared::domain_types::ValidatedFilePath;
use crate::shared::error::AppError;
use async_trait::async_trait;
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

/// File processing result returned after successful import
///
/// Contains metadata about the imported file and created document.
///
/// # Example
///
/// ```rust
/// let info = ProcessedFileInfo {
///     file_name: "document.pdf".to_string(),
///     document_id: "doc-123".to_string(),
///     chunk_count: 10,
///     size_bytes: 1024,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct ProcessedFileInfo {
    /// Original file name
    pub file_name: String,

    /// Created document ID
    pub document_id: String,

    /// Number of chunks created
    pub chunk_count: usize,

    /// File size in bytes
    pub size_bytes: i64,
}

/// Batch file import service trait
///
/// Defines operations for batch file import with background processing and progress tracking.
/// Implementations must handle:
/// - File validation (MIME type, size limits)
/// - Background processing (non-blocking)
/// - Progress tracking via BatchJobRepository
/// - Individual file failure isolation (one failure doesn't stop batch)
///
/// # Architecture
///
/// Implementations should keep orchestration separate from file processing:
/// - **Stud (Public Interface)**: This trait defines what batch import must do
/// - **Bricks (Implementations)**: BatchFileImportService (production), MockBatchFileImportService (testing)
/// - **Regeneratable**: Can swap implementations without changing callers
///
/// # Security
///
/// All file paths must be validated with ValidatedFilePath to prevent directory traversal (CWE-22).
/// File size and MIME type validation prevents resource exhaustion (CWE-770).
///
/// # Example
///
/// ```rust,no_run
/// use lattice::infrastructure::services::traits::BatchFileImportServiceTrait;
///
/// async fn import_files(
///     service: &dyn BatchFileImportServiceTrait,
///     paths: Vec<ValidatedFilePath>,
/// ) -> Result<String, AppError> {
///     // Start batch import (returns job_id for tracking)
///     let job_id = service.start_batch_import(paths).await?;
///
///     // Job runs in background, caller can poll status
///     Ok(job_id)
/// }
/// ```
#[async_trait]
pub trait BatchFileImportServiceTrait: Send + Sync {
    /// Start batch file import job
    ///
    /// Creates a batch job in the database and spawns a background task to process files.
    /// Returns immediately with a job_id for progress tracking.
    ///
    /// # Process Flow
    ///
    /// 1. Validate all files (MIME type, size, accessibility)
    /// 2. Create batch job record (status: pending)
    /// 3. Create batch items for each file
    /// 4. Spawn background Tokio task for processing
    /// 5. Return job_id
    ///
    /// # Arguments
    ///
    /// * `file_paths` - Validated file paths to import (max 100 files)
    ///
    /// # Returns
    ///
    /// Job ID (UUID) for progress tracking
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if any file fails validation
    /// - `AppError::Database` if job creation fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use lattice::infrastructure::services::traits::BatchFileImportServiceTrait;
    /// # use lattice::domain::value_objects::ValidatedFilePath;
    /// # async fn example(service: &dyn BatchFileImportServiceTrait) -> Result<(), Box<dyn std::error::Error>> {
    /// let paths = vec![
    ///     ValidatedFilePath::new("/path/to/file1.pdf")?,
    ///     ValidatedFilePath::new("/path/to/file2.txt")?,
    /// ];
    ///
    /// let job_id = service.start_batch_import(paths).await?;
    /// println!("Job started: {}", job_id);
    /// # Ok(())
    /// # }
    /// ```
    async fn start_batch_import(
        &self,
        file_paths: Vec<ValidatedFilePath>,
    ) -> Result<String, AppError>;

    /// Validate file before import
    ///
    /// Checks that the file:
    /// - Exists and is readable
    /// - Has a supported MIME type (PDF, DOCX, TXT, MD)
    /// - Is within size limits (max 50MB)
    ///
    /// Called before processing to fail fast on invalid files.
    ///
    /// # Arguments
    ///
    /// * `path` - Validated file path to check
    ///
    /// # Returns
    ///
    /// File metadata if validation succeeds
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if file type is unsupported
    /// - `AppError::InvalidInput` if file size exceeds limit
    /// - `AppError::IoError` if file is not accessible
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use lattice::infrastructure::services::traits::BatchFileImportServiceTrait;
    /// # use lattice::domain::value_objects::ValidatedFilePath;
    /// # fn example(service: &dyn BatchFileImportServiceTrait) -> Result<(), Box<dyn std::error::Error>> {
    /// let path = ValidatedFilePath::new("/path/to/file.pdf")?;
    /// let metadata = service.validate_file(&path)?;
    /// println!("File: {}, Size: {} bytes", metadata.file_name(), metadata.size_bytes());
    /// # Ok(())
    /// # }
    /// ```
    fn validate_file(&self, path: &ValidatedFilePath) -> Result<FileMetadata, AppError>;
}

/// Mock implementation for testing
///
/// Provides a test double that can be configured to return specific results
/// or simulate failures. Used in unit and integration tests.
///
/// # Design Pattern: Test Double
///
/// This is a configurable mock that follows the "dependency inversion" principle.
/// Tests can inject this instead of the real service to:
/// - Avoid file I/O during tests
/// - Simulate errors and edge cases
/// - Verify service interactions
///
/// # Example
///
/// ```rust
/// use lattice::infrastructure::services::traits::MockBatchFileImportService;
/// use lattice::infrastructure::services::traits::ProcessedFileInfo;
///
/// let mock = MockBatchFileImportService::new();
///
/// // Configure success response
/// mock.set_success(
///     "/test/file.pdf",
///     ProcessedFileInfo {
///         file_name: "file.pdf".to_string(),
///         document_id: "doc-123".to_string(),
///         chunk_count: 5,
///         size_bytes: 1024,
///     },
/// );
///
/// // Configure failure
/// mock.set_failure("/test/bad.pdf", "Unsupported file type");
/// ```
pub struct MockBatchFileImportService {
    /// Pre-configured success responses keyed by file path
    responses: Arc<Mutex<HashMap<String, ProcessedFileInfo>>>,

    /// Pre-configured failures keyed by file path
    should_fail: Arc<Mutex<HashMap<String, String>>>,
}

impl MockBatchFileImportService {
    /// Create a new mock service with empty configuration
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(HashMap::new())),
            should_fail: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Configure a success response for a specific file path
    ///
    /// When validate_file is called with this path, it will succeed
    /// and return the configured ProcessedFileInfo.
    ///
    /// # Arguments
    ///
    /// * `path` - File path to configure (string representation)
    /// * `info` - ProcessedFileInfo to return
    ///
    /// # Example
    ///
    /// ```rust
    /// # use lattice::infrastructure::services::traits::{MockBatchFileImportService, ProcessedFileInfo};
    /// let mock = MockBatchFileImportService::new();
    /// mock.set_success("/test/file.pdf", ProcessedFileInfo {
    ///     file_name: "file.pdf".to_string(),
    ///     document_id: "doc-123".to_string(),
    ///     chunk_count: 10,
    ///     size_bytes: 2048,
    /// });
    /// ```
    pub fn set_success(&self, path: &str, info: ProcessedFileInfo) {
        self.responses.lock().insert(path.to_string(), info);
    }

    /// Configure a failure response for a specific file path
    ///
    /// When validate_file is called with this path, it will fail
    /// with the configured error message.
    ///
    /// # Arguments
    ///
    /// * `path` - File path to configure (string representation)
    /// * `error` - Error message to return
    ///
    /// # Example
    ///
    /// ```rust
    /// # use lattice::infrastructure::services::traits::MockBatchFileImportService;
    /// let mock = MockBatchFileImportService::new();
    /// mock.set_failure("/test/bad.pdf", "File too large");
    /// ```
    pub fn set_failure(&self, path: &str, error: &str) {
        self.should_fail
            .lock()
            .insert(path.to_string(), error.to_string());
    }
}

impl Default for MockBatchFileImportService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BatchFileImportServiceTrait for MockBatchFileImportService {
    async fn start_batch_import(
        &self,
        _file_paths: Vec<ValidatedFilePath>,
    ) -> Result<String, AppError> {
        Ok(uuid::Uuid::new_v4().to_string())
    }

    fn validate_file(&self, path: &ValidatedFilePath) -> Result<FileMetadata, AppError> {
        let path_str = path.as_path().to_string_lossy().to_string();

        if let Some(error) = self.should_fail.lock().get(&path_str) {
            return Err(AppError::InvalidInput(error.clone()));
        }

        FileMetadataFactory::from_path(path.as_path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_processed_file_info_default() {
        let info = ProcessedFileInfo::default();
        assert_eq!(info.file_name, "");
        assert_eq!(info.document_id, "");
        assert_eq!(info.chunk_count, 0);
        assert_eq!(info.size_bytes, 0);
    }

    #[test]
    fn test_processed_file_info_serialization() {
        let info = ProcessedFileInfo {
            file_name: "test.pdf".to_string(),
            document_id: "doc-123".to_string(),
            chunk_count: 5,
            size_bytes: 1024,
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("test.pdf"));
        assert!(json.contains("doc-123"));
    }

    #[tokio::test]
    async fn test_mock_batch_file_import_success() {
        let mock = MockBatchFileImportService::new();

        let paths = vec![ValidatedFilePath::new(std::env::temp_dir().join("test1.txt")).unwrap()];

        let job_id = mock.start_batch_import(paths).await.unwrap();
        assert!(!job_id.is_empty());
    }

    #[test]
    fn test_mock_set_success() {
        let mock = MockBatchFileImportService::new();

        mock.set_success(
            "/test/file.pdf",
            ProcessedFileInfo {
                file_name: "file.pdf".to_string(),
                document_id: "doc-123".to_string(),
                chunk_count: 10,
                size_bytes: 2048,
            },
        );

        let responses = mock.responses.lock();
        assert!(responses.contains_key("/test/file.pdf"));
    }

    #[test]
    fn test_mock_set_failure() {
        let mock = MockBatchFileImportService::new();

        mock.set_failure("/test/bad.pdf", "File too large");

        let failures = mock.should_fail.lock();
        assert_eq!(
            failures.get("/test/bad.pdf"),
            Some(&"File too large".to_string())
        );
    }
}
