//! Database query operations for file storage.
//!
//! This module contains all database operations for managing file records,
//! including CRUD operations, reference counting, and cleanup.

use super::models::FileRecord;
use crate::shared::error::{AppError, Result};
use chrono::Utc;
use sqlx::SqlitePool;

/// Database row type for file records.
///
/// Fields: id, content_hash, file_name, file_extension, mime_type, size_bytes,
/// storage_path, is_indexed, created_at, accessed_at, ref_count, metadata
type FileRecordRow = (
    String,
    String,
    String,
    Option<String>,
    String,
    i64,
    String,
    i32,
    i64,
    i64,
    i32,
    Option<String>,
);

/// Converts a database row tuple into a FileRecord.
///
/// This helper function handles the common pattern of converting SQLx query results
/// into our FileRecord struct.
fn row_to_file_record(row: FileRecordRow) -> FileRecord {
    let (
        id,
        content_hash,
        file_name,
        file_extension,
        mime_type,
        size_bytes,
        storage_path,
        is_indexed,
        created_at,
        accessed_at,
        ref_count,
        metadata,
    ) = row;
    FileRecord {
        id,
        content_hash,
        file_name,
        file_extension,
        mime_type,
        size_bytes,
        storage_path,
        is_indexed: is_indexed != 0,
        created_at,
        accessed_at,
        ref_count,
        metadata: metadata.and_then(|m| serde_json::from_str(&m).ok()),
    }
}

/// Retrieves a file record by its content hash.
///
/// # Arguments
///
/// * `pool` - Database connection pool
/// * `hash` - SHA256 hash to search for
///
/// # Returns
///
/// `Some(FileRecord)` if found, `None` otherwise
pub(crate) async fn get_file_by_hash(pool: &SqlitePool, hash: &str) -> Result<Option<FileRecord>> {
    let record = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            Option<String>,
            String,
            i64,
            String,
            i32,
            i64,
            i64,
            i32,
            Option<String>,
        ),
    >(
        r#"
        SELECT id, content_hash, file_name, file_extension, mime_type,
               size_bytes, storage_path, is_indexed, created_at, accessed_at,
               ref_count, metadata
        FROM files
        WHERE content_hash = ?
        "#,
    )
    .bind(hash)
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("Failed to query file by hash: {}", e)))?;

    Ok(record.map(row_to_file_record))
}

/// Retrieves a file record by its ID.
///
/// # Arguments
///
/// * `pool` - Database connection pool
/// * `file_id` - File ID to search for
///
/// # Returns
///
/// `Some(FileRecord)` if found, `None` otherwise
pub(crate) async fn get_file_by_id(pool: &SqlitePool, file_id: &str) -> Result<Option<FileRecord>> {
    let record = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            Option<String>,
            String,
            i64,
            String,
            i32,
            i64,
            i64,
            i32,
            Option<String>,
        ),
    >(
        r#"
        SELECT id, content_hash, file_name, file_extension, mime_type,
               size_bytes, storage_path, is_indexed, created_at, accessed_at,
               ref_count, metadata
        FROM files
        WHERE id = ?
        "#,
    )
    .bind(file_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("Failed to query file by id: {}", e)))?;

    Ok(record.map(row_to_file_record))
}

/// Retrieves the storage path for a file by its ID.
///
/// # Arguments
///
/// * `pool` - Database connection pool
/// * `file_id` - File ID to look up
///
/// # Returns
///
/// Storage path string if file exists
pub(crate) async fn get_storage_path(pool: &SqlitePool, file_id: &str) -> Result<Option<String>> {
    let record: Option<(String,)> = sqlx::query_as("SELECT storage_path FROM files WHERE id = ?")
        .bind(file_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to fetch file: {}", e)))?;

    Ok(record.map(|(path,)| path))
}

/// Inserts a new file record into the database.
///
/// Uses `INSERT OR IGNORE` to handle race conditions where multiple requests
/// try to insert the same file hash simultaneously.
///
/// # Arguments
///
/// * `pool` - Database connection pool
/// * `file_id` - Unique file ID (UUID)
/// * `hash` - Content hash
/// * `file_name` - Original file name
/// * `file_extension` - File extension (lowercase, without dot)
/// * `mime_type` - MIME type
/// * `size_bytes` - File size in bytes
/// * `storage_path` - Relative storage path
/// * `metadata` - Optional JSON metadata
#[allow(clippy::too_many_arguments)]
pub(crate) async fn insert_file_record(
    pool: &SqlitePool,
    file_id: &str,
    hash: &str,
    file_name: &str,
    file_extension: &Option<String>,
    mime_type: &str,
    size_bytes: i64,
    storage_path: &str,
    metadata: Option<&str>,
) -> Result<()> {
    let now = Utc::now().timestamp();

    sqlx::query(
        r#"
        INSERT OR IGNORE INTO files (
            id, content_hash, file_name, file_extension, mime_type,
            size_bytes, storage_path, is_indexed, created_at, accessed_at,
            ref_count, metadata
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        "#,
    )
    .bind(file_id)
    .bind(hash)
    .bind(file_name)
    .bind(file_extension)
    .bind(mime_type)
    .bind(size_bytes)
    .bind(storage_path)
    .bind(0)
    .bind(now)
    .bind(now)
    .bind(1)
    .bind(metadata)
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("Failed to insert file record: {}", e)))?;

    Ok(())
}

/// Deletes a file record from the database.
///
/// # Arguments
///
/// * `pool` - Database connection pool
/// * `file_id` - File ID to delete
pub(crate) async fn delete_file_record(pool: &SqlitePool, file_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM files WHERE id = ?1")
        .bind(file_id)
        .execute(pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to delete file record: {}", e)))?;
    Ok(())
}

/// Increments the reference count for a file.
///
/// Also updates the `accessed_at` timestamp.
///
/// # Arguments
///
/// * `pool` - Database connection pool
/// * `file_id` - File ID to increment
///
/// # Errors
///
/// Returns `NotFound` error if file doesn't exist
pub(crate) async fn increment_ref_count(pool: &SqlitePool, file_id: &str) -> Result<()> {
    let now = Utc::now().timestamp();
    let result =
        sqlx::query("UPDATE files SET ref_count = ref_count + 1, accessed_at = ? WHERE id = ?")
            .bind(now)
            .bind(file_id)
            .execute(pool)
            .await
            .map_err(|e| AppError::Database(format!("Failed to increment ref_count: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("File not found: {}", file_id)));
    }

    Ok(())
}

/// Decrements the reference count for a file.
///
/// # Arguments
///
/// * `pool` - Database connection pool (transaction)
/// * `file_id` - File ID to decrement
pub(crate) async fn decrement_ref_count(
    pool: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    file_id: &str,
) -> Result<()> {
    sqlx::query("UPDATE files SET ref_count = ref_count - 1 WHERE id = ?1")
        .bind(file_id)
        .execute(&mut **pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to decrement ref_count: {}", e)))?;
    Ok(())
}

/// Marks a file as indexed.
///
/// # Arguments
///
/// * `pool` - Database connection pool
/// * `file_id` - File ID to mark
///
/// # Errors
///
/// Returns `NotFound` error if file doesn't exist
pub(crate) async fn mark_as_indexed(pool: &SqlitePool, file_id: &str) -> Result<()> {
    let result = sqlx::query("UPDATE files SET is_indexed = 1 WHERE id = ?")
        .bind(file_id)
        .execute(pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to mark file as indexed: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("File not found: {}", file_id)));
    }

    Ok(())
}

/// Retrieves all orphaned files (ref_count = 0).
///
/// # Arguments
///
/// * `pool` - Database connection pool
///
/// # Returns
///
/// Vector of (file_id, storage_path) tuples for orphaned files
pub(crate) async fn get_orphaned_files(pool: &SqlitePool) -> Result<Vec<(String, String)>> {
    let orphaned_files = sqlx::query_as::<_, (String, String)>(
        "SELECT id, storage_path FROM files WHERE ref_count = 0",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Database(format!("Failed to query orphaned files: {}", e)))?;

    Ok(orphaned_files)
}
