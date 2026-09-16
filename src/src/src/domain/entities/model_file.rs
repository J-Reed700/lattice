use crate::domain::value_objects::model_status::FileStatus;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelFile {
    pub id: String,
    pub model_id: String,
    pub file_name: String,
    pub file_path: String,
    pub relative_path: String,
    pub size_bytes: i64,
    pub downloaded_bytes: i64,
    pub checksum_sha256: Option<String>,
    pub download_url: String,
    pub status: FileStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub downloaded_at: Option<DateTime<Utc>>,
}
