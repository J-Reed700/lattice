//! Recent Documents DTOs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct RecentDocumentDto {
    pub id: String,
    pub document_id: String,
    pub document_name: String,
    pub document_path: String,
    pub file_type: Option<String>,
    pub last_accessed_at: String,
    pub access_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackAccessRequestDto {
    pub document_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackAccessResponseDto {
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRecentDocumentsRequestDto {
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    20
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRecentDocumentsResponseDto {
    pub documents: Vec<RecentDocumentDto>,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClearRecentHistoryRequestDto {
    pub before_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClearRecentHistoryResponseDto {
    pub cleared_count: usize,
    pub status: String,
}
