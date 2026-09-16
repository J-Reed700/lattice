//! Main file storage service implementation.
//!
//! This module provides the high-level FileStorageService that orchestrates
//! file storage operations using hash, database, and file system modules.

use super::{
    fs_ops, hash,
    models::{FileRecord, DEFAULT_MAX_FILE_SIZE},
    queries,
};
use crate::shared::error::{AppError, Result};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// File storage service with content-addressed storage and deduplication.
///
/// Files are stored using their SHA256 hash as the storage path, enabling
/// automatic deduplication. Reference counting ensures files are only deleted
/// when no longer needed.
pub struct FileStorageService {
    vault_path: PathBuf,
    pool: SqlitePool,
    max_file_size: u64,
}

impl FileStorageService {
    /// Creates a new FileStorageService with default max file size (50MB).
    ///
    /// # Arguments
    ///
    /// * `vault_path` - Root directory for file storage
    /// * `pool` - SQLite connection pool for metadata
    pub fn new(vault_path: PathBuf, pool: SqlitePool) -> Self {
        Self::with_max_size(vault_path, pool, DEFAULT_MAX_FILE_SIZE)
    }

    /// Creates a new FileStorageService with custom max file size.
    ///
    /// # Arguments
    ///
    /// * `vault_path` - Root directory for file storage
    /// * `pool` - SQLite connection pool for metadata
    /// * `max_file_size` - Maximum allowed file size in bytes
    pub fn with_max_size(vault_path: PathBuf, pool: SqlitePool, max_file_size: u64) -> Self {
        Self {
            vault_path,
            pool,
            max_file_size,
        }
    }

    /// Stores a file in the lattice with automatic deduplication.
    ///
    /// # Process
    ///
    /// 1. Validate file size
    /// 2. Compute SHA256 hash
    /// 3. Check if file already exists (by hash)
    /// 4. If exists, increment reference count and return existing record
    /// 5. If new, insert database record and copy file to storage
    ///
    /// # Arguments
    ///
    /// * `source_path` - Validated path to the source file (prevents directory traversal)
    /// * `mime_type` - MIME type of the file
    /// * `metadata` - Optional JSON metadata
    ///
    /// # Returns
    ///
    /// FileRecord with file information and storage location
    ///
    /// # Errors
    ///
    /// - `FileTooLarge` if file exceeds max_file_size
    /// - `FileStorage` if file operations fail
    /// - `Database` if database operations fail
    ///
    /// # Security
    ///
    /// The `ValidatedFilePath` parameter ensures path validation has occurred,
    /// preventing directory traversal attacks (CWE-22).
    pub async fn store_file(
        &self,
        source_path: crate::shared::domain_types::ValidatedFilePath,
        mime_type: &str,
        metadata: Option<serde_json::Value>,
    ) -> Result<FileRecord> {
        // Extract validated path for internal use
        let path = source_path.as_path();

        // Validate file exists and get size
        let file_metadata = tokio::fs::metadata(path)
            .await
            .map_err(|e| AppError::FileStorage(format!("Failed to read file metadata: {}", e)))?;

        if !file_metadata.is_file() {
            return Err(AppError::FileStorage(format!(
                "Path is not a file: {}",
                path.display()
            )));
        }

        let size_bytes = file_metadata.len();
        if size_bytes > self.max_file_size {
            return Err(AppError::FileTooLarge {
                path: path.display().to_string(),
                size_bytes,
                max_size_bytes: self.max_file_size,
            });
        }

        // Compute content hash
        let content_hash = hash::compute_hash(path).await?;
        hash::validate_hash(&content_hash)?;

        // Check for existing file with same hash
        if let Some(mut existing) = queries::get_file_by_hash(&self.pool, &content_hash).await? {
            // Verify sizes match (detect hash collisions)
            if existing.size_bytes as u64 == size_bytes {
                queries::increment_ref_count(&self.pool, &existing.id).await?;
                existing.ref_count += 1; // Update local copy to match database
                return Ok(existing);
            } else {
                tracing::error!(
                    "Hash collision detected: {} ({}B) vs {} ({}B) both hash to {}",
                    path.display(),
                    size_bytes,
                    existing.file_name,
                    existing.size_bytes,
                    content_hash
                );
                return Err(AppError::FileStorage(
                    "Hash collision detected - file has same hash but different size".to_string(),
                ));
            }
        }

        // Extract file metadata
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let file_extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase());

        let file_id = Uuid::new_v4().to_string();
        let storage_path = hash::get_storage_path(&content_hash)?;

        let metadata_json = metadata
            .as_ref()
            .and_then(|m| serde_json::to_string(m).ok());

        // Insert database record
        queries::insert_file_record(
            &self.pool,
            &file_id,
            &content_hash,
            &file_name,
            &file_extension,
            mime_type,
            size_bytes as i64,
            &storage_path,
            metadata_json.as_deref(),
        )
        .await?;

        // Retrieve the actual record (may have been inserted by another request due to OR IGNORE)
        let file_record = queries::get_file_by_hash(&self.pool, &content_hash)
            .await?
            .ok_or_else(|| AppError::Database("File not found after insert".to_string()))?;

        // If another request inserted the record, just increment its ref count
        if file_record.id != file_id {
            queries::increment_ref_count(&self.pool, &file_record.id).await?;
            return Ok(file_record);
        }

        // Copy file to storage (only if we successfully inserted the record)
        let full_path = self.vault_path.join(&storage_path);

        if let Some(parent) = full_path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                // Attempt to rollback database record on failure
                if let Err(delete_err) =
                    queries::delete_file_record(&self.pool, &file_record.id).await
                {
                    tracing::error!(
                        "Failed to rollback file record during cleanup: {}",
                        delete_err
                    );
                }
                return Err(AppError::FileStorage(format!(
                    "Failed to create directory: {}",
                    e
                )));
            }
        }

        if let Err(e) = fs_ops::copy_file_atomic(path, &full_path).await {
            // Attempt to rollback database record on failure
            if let Err(delete_err) = queries::delete_file_record(&self.pool, &file_record.id).await
            {
                tracing::error!(
                    "Failed to rollback file record during cleanup: {}",
                    delete_err
                );
            }
            return Err(e);
        }

        Ok(file_record)
    }

    /// Gets the full file system path for a stored file.
    ///
    /// # Arguments
    ///
    /// * `file_id` - File ID to look up
    ///
    /// # Returns
    ///
    /// Full PathBuf to the file
    ///
    /// # Errors
    ///
    /// - `NotFound` if file record doesn't exist
    /// - `FileStorage` if file doesn't exist on disk
    pub async fn get_file_path(&self, file_id: &str) -> Result<PathBuf> {
        let storage_path = queries::get_storage_path(&self.pool, file_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("File not found: {}", file_id)))?;

        let full_path = self.vault_path.join(&storage_path);
        if !full_path.exists() {
            return Err(AppError::FileStorage(format!(
                "File not found on disk: {}",
                full_path.display()
            )));
        }

        Ok(full_path)
    }

    /// Gets a file record by its content hash.
    ///
    /// # Arguments
    ///
    /// * `hash` - SHA256 hash to search for
    pub async fn get_file_by_hash(&self, hash: &str) -> Result<Option<FileRecord>> {
        queries::get_file_by_hash(&self.pool, hash).await
    }

    /// Gets a file record by its ID.
    ///
    /// # Arguments
    ///
    /// * `file_id` - File ID to search for
    pub async fn get_file_by_id(&self, file_id: &str) -> Result<Option<FileRecord>> {
        queries::get_file_by_id(&self.pool, file_id).await
    }

    /// Deletes a file, using reference counting.
    ///
    /// If the file has multiple references, only decrements the ref_count.
    /// If ref_count reaches 1, deletes the file from both disk and database.
    ///
    /// # Arguments
    ///
    /// * `file_id` - File ID to delete
    ///
    /// # Errors
    ///
    /// - `NotFound` if file doesn't exist
    /// - `Database` if database operations fail
    /// - `FileStorage` if file deletion fails
    pub async fn delete_file(&self, file_id: &str) -> Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;

        // Get file record
        let file_record = queries::get_file_by_id(&self.pool, file_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("File not found: {}", file_id)))?;

        if file_record.ref_count <= 1 {
            // Delete physical file
            let full_path = self.vault_path.join(&file_record.storage_path);
            if full_path.exists() {
                tokio::fs::remove_file(&full_path).await.map_err(|e| {
                    AppError::FileStorage(format!("Failed to delete file from disk: {}", e))
                })?;
            }

            // Delete database record
            sqlx::query("DELETE FROM files WHERE id = ?1")
                .bind(file_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::Database(format!("Failed to delete file record: {}", e)))?;
        } else {
            // Just decrement ref count
            queries::decrement_ref_count(&mut tx, file_id).await?;
        }

        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    /// Increments the reference count for a file.
    ///
    /// Also updates the accessed_at timestamp.
    ///
    /// # Arguments
    ///
    /// * `file_id` - File ID to increment
    pub async fn increment_ref_count(&self, file_id: &str) -> Result<()> {
        queries::increment_ref_count(&self.pool, file_id).await
    }

    /// Cleans up orphaned files with ref_count = 0.
    ///
    /// Deletes both the database records and physical files.
    ///
    /// # Returns
    ///
    /// Number of files cleaned up
    pub async fn cleanup_orphaned_files(&self) -> Result<usize> {
        let orphaned_files = queries::get_orphaned_files(&self.pool).await?;
        let count = orphaned_files.len();

        for (id, storage_path) in orphaned_files {
            // Delete from disk
            let full_path = self.vault_path.join(&storage_path);
            if full_path.exists() {
                if let Err(e) = tokio::fs::remove_file(&full_path).await {
                    tracing::warn!(
                        "Failed to delete orphaned file {}: {}",
                        full_path.display(),
                        e
                    );
                }
            }

            // Delete from database
            queries::delete_file_record(&self.pool, &id).await?;
        }

        Ok(count)
    }

    /// Verifies file integrity by checking existence and hash.
    ///
    /// # Arguments
    ///
    /// * `file_id` - File ID to verify
    ///
    /// # Returns
    ///
    /// `true` if file exists and hash matches, `false` otherwise
    pub async fn verify_file(&self, file_id: &str) -> Result<bool> {
        let record = self.get_file_by_id(file_id).await?;

        match record {
            Some(file_record) => {
                let full_path = self.vault_path.join(&file_record.storage_path);

                // Check file exists
                if !full_path.exists() {
                    return Ok(false);
                }

                // Verify hash
                let current_hash = hash::compute_hash(&full_path).await?;
                Ok(current_hash == file_record.content_hash)
            }
            None => Ok(false),
        }
    }

    /// Marks a file as indexed for search.
    ///
    /// # Arguments
    ///
    /// * `file_id` - File ID to mark
    pub async fn mark_as_indexed(&self, file_id: &str) -> Result<()> {
        queries::mark_as_indexed(&self.pool, file_id).await
    }
}

// =============================================================================
// Trait Implementation
// =============================================================================

use crate::infrastructure::services::traits::FileStorageServiceTrait;
use async_trait::async_trait;

#[async_trait]
impl FileStorageServiceTrait for FileStorageService {
    async fn store_file(
        &self,
        source_path: crate::shared::domain_types::ValidatedFilePath,
        mime_type: &str,
        metadata: Option<serde_json::Value>,
    ) -> Result<crate::infrastructure::services::traits::FileRecord> {
        // Call the existing method and convert the result
        let record = FileStorageService::store_file(self, source_path, mime_type, metadata).await?;

        // Convert from internal FileRecord to trait FileRecord
        Ok(crate::infrastructure::services::traits::FileRecord {
            id: record.id,
            content_hash: record.content_hash,
            file_name: record.file_name,
            file_extension: record.file_extension,
            mime_type: record.mime_type,
            size_bytes: record.size_bytes,
            storage_path: record.storage_path,
            is_indexed: record.is_indexed,
            created_at: record.created_at,
            accessed_at: record.accessed_at,
            ref_count: record.ref_count,
            metadata: record.metadata,
        })
    }

    async fn get_file_path(&self, file_id: &str) -> Result<std::path::PathBuf> {
        FileStorageService::get_file_path(self, file_id).await
    }

    async fn delete_file(&self, file_id: &str) -> Result<()> {
        FileStorageService::delete_file(self, file_id).await
    }

    async fn get_file_metadata(&self, file_id: &str) -> Result<Option<serde_json::Value>> {
        let record = FileStorageService::get_file_by_id(self, file_id).await?;
        Ok(record.and_then(|r| r.metadata))
    }
}
