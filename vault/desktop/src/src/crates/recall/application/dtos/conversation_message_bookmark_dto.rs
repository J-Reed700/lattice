//! DTOs for message-level bookmarks inside conversations.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMessageBookmarkDto {
    pub id: String,
    pub conversation_id: String,
    pub conversation_title: String,
    pub space_id: String,
    pub message_id: String,
    pub message_role: String,
    pub message_preview: String,
    pub title: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkConversationMessageRequestDto {
    pub conversation_id: String,
    pub message_id: String,
    pub title: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UnbookmarkConversationMessageRequestDto {
    pub conversation_id: String,
    pub message_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ListMessageBookmarksQueryDto {
    pub conversation_id: Option<String>,
    pub query: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ListMessageBookmarksResponseDto {
    pub bookmarks: Vec<ConversationMessageBookmarkDto>,
    pub total: usize,
}
