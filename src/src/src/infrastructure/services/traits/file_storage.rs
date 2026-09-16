//! Service trait definitions
//!
//! This module defines trait interfaces for dependency injection.

use crate::shared::domain_types::ValidatedFilePath;
use crate::shared::error::Result;
use async_trait::async_trait;

/// File record type for storage service
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileRecord {
    pub id: String,
    pub content_hash: String,
    pub file_name: String,
    pub file_extension: Option<String>,
    pub mime_type: String,
    pub size_bytes: i64,
    pub storage_path: String,
    pub is_indexed: bool,
    pub created_at: i64,
    pub accessed_at: i64,
    pub ref_count: i32,
    pub metadata: Option<serde_json::Value>,
}

#[async_trait]
pub trait FileStorageServiceTrait: Send + Sync {
    /// Store file with metadata
    ///
    /// # Arguments
    /// * `source_path` - Validated path to source file (prevents directory traversal)
    /// * `mime_type` - MIME type of file
    /// * `metadata` - Optional JSON metadata
    ///
    /// # Returns
    /// FileRecord with storage location and hash
    ///
    /// # Errors
    /// - `AppError::FileTooLarge` if file exceeds max size
    /// - `AppError::FileStorage` if file operations fail
    /// - `AppError::Database` if database insert fails
    ///
    /// # Security
    /// The `ValidatedFilePath` parameter ensures path validation has occurred
    /// before calling this method, preventing directory traversal attacks (CWE-22).
    async fn store_file(
        &self,
        source_path: ValidatedFilePath,
        mime_type: &str,
        metadata: Option<serde_json::Value>,
    ) -> Result<FileRecord>;

    /// Get file path by ID
    ///
    /// # Arguments
    /// * `file_id` - File ID to look up
    ///
    /// # Returns
    /// Full path to stored file
    ///
    /// # Errors
    /// - `AppError::NotFound` if file record doesn't exist
    /// - `AppError::FileStorage` if file doesn't exist on disk
    async fn get_file_path(&self, file_id: &str) -> Result<std::path::PathBuf>;

    /// Delete file by ID (with reference counting)
    ///
    /// Decrements ref count. If count reaches 0, deletes file from disk.
    ///
    /// # Arguments
    /// * `file_id` - File ID to delete
    ///
    /// # Errors
    /// - `AppError::NotFound` if file doesn't exist
    /// - `AppError::Database` if transaction fails
    /// - `AppError::FileStorage` if disk deletion fails
    async fn delete_file(&self, file_id: &str) -> Result<()>;

    /// Get file metadata by ID
    ///
    /// # Arguments
    /// * `file_id` - File ID to look up
    ///
    /// # Returns
    /// Optional JSON metadata
    ///
    /// # Errors
    /// - `AppError::Database` if query fails
    async fn get_file_metadata(&self, file_id: &str) -> Result<Option<serde_json::Value>>;
}
