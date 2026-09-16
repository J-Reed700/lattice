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
