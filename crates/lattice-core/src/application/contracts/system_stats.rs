//! Library-wide counts the stats page shows.

use serde::{Deserialize, Serialize};

/// System statistics response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatsDto {
    /// Total number of indexed documents
    pub total_documents: i64,
    /// Total number of chunks across all documents
    pub total_chunks: i64,
    /// Total number of tags in the system
    pub total_tags: i64,
    /// Total storage size in bytes
    pub storage_size_bytes: i64,
}
