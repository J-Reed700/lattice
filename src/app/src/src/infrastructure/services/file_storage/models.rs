//! Data models for file storage.
//!
//! This module contains the core data structures used by the file storage system,
//! including file records and configuration constants.

/// Maximum file size (50MB)
pub const DEFAULT_MAX_FILE_SIZE: u64 = 50 * 1024 * 1024;

/// Represents a file record in the storage system.
///
/// Files are content-addressed using SHA256 hashes and support reference counting
/// for deduplication.
#[derive(Debug, Clone)]
pub struct FileRecord {
    /// Unique identifier for this file record
    pub id: String,
    /// SHA256 hash of file content (64 hex chars)
    pub content_hash: String,
    /// Original file name
    pub file_name: String,
    /// File extension (lowercase, without dot)
    pub file_extension: Option<String>,
    /// MIME type of the file
    pub mime_type: String,
    /// File size in bytes
    pub size_bytes: i64,
    /// Relative storage path from vault root
    pub storage_path: String,
    /// Whether the file has been indexed for search
    pub is_indexed: bool,
    /// Unix timestamp when file was created
    pub created_at: i64,
    /// Unix timestamp when file was last accessed
    pub accessed_at: i64,
    /// Reference count for deduplication (1 = single reference)
    pub ref_count: i32,
    /// Optional JSON metadata
    pub metadata: Option<serde_json::Value>,
}
