//! Recent-document records exchanged through application ports.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[specta(rename = "RecentDocumentDto")]
pub struct RecentDocumentRecord {
    pub id: String,
    pub document_id: String,
    pub document_name: String,
    pub document_path: String,
    pub file_type: Option<String>,
    pub last_accessed_at: String,
    pub access_count: i64,
}

/// Recent document with access metadata
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RecentDocument {
    pub id: String,
    pub document_id: String,
    pub document_name: String,
    pub document_path: String,
    pub file_type: Option<String>,
    pub last_accessed_at: String,
    pub access_count: i64,
}

impl From<RecentDocumentRecord> for RecentDocument {
    fn from(record: RecentDocumentRecord) -> Self {
        Self {
            id: record.id,
            document_id: record.document_id,
            document_name: record.document_name,
            document_path: record.document_path,
            file_type: record.file_type,
            last_accessed_at: record.last_accessed_at,
            access_count: record.access_count,
        }
    }
}
