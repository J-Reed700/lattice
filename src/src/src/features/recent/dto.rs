//! Recent Documents DTOs

use serde::{Deserialize, Serialize};

pub use crate::application::contracts::recent_documents::RecentDocumentRecord as RecentDocumentDto;

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
