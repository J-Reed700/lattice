//! Storage type definitions.

use serde::{Deserialize, Serialize};

/// Database record for a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentRecord {
    pub id: String,
    pub file_path: String,
    pub file_name: String,
    pub file_type: Option<String>,
    pub mime_type: String,
    pub size_bytes: i64,
    pub checksum: String,
    pub status: String,

    /// Document path (normalized)
    pub path: String,

    /// Timestamp when document was indexed (ISO 8601 format)
    pub indexed_at: String,
}
