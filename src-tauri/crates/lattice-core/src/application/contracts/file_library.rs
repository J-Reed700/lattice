//! File-library read models shared with command adapters.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct IndexedFolder {
    pub path: String,
    pub recursive: bool,
    pub enabled: bool,
    pub last_scan: Option<String>,
    pub document_count: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct IndexingActivity {
    pub id: String,
    pub action: String,
    pub file_path: String,
    pub status: String,
    pub timestamp: String,
    pub details: Option<String>,
}
