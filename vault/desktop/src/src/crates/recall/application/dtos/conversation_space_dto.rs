//! DTOs for conversation spaces and explorer operations.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSpaceDto {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub accent_color: Option<String>,
    pub space_prompt: Option<String>,
    pub default_model_name: Option<String>,
    pub tool_preferences_json: Option<String>,
    pub is_archived: bool,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateConversationSpaceRequestDto {
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub accent_color: Option<String>,
    pub space_prompt: Option<String>,
    pub default_model_name: Option<String>,
    pub tool_preferences_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConversationSpaceRequestDto {
    pub space_id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub accent_color: Option<String>,
    pub space_prompt: Option<String>,
    pub default_model_name: Option<String>,
    pub tool_preferences_json: Option<String>,
    pub is_archived: Option<bool>,
    pub sort_order: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveConversationSpaceRequestDto {
    pub space_id: String,
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MoveConversationToSpaceRequestDto {
    pub conversation_id: String,
    pub space_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SetConversationStateRequestDto {
    pub conversation_id: String,
    pub value: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSpaceMemberDto {
    pub space_id: String,
    pub member_id: String,
    pub display_name: String,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    pub role: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpsertConversationSpaceMemberRequestDto {
    pub space_id: String,
    pub member_id: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RemoveConversationSpaceMemberRequestDto {
    pub space_id: String,
    pub member_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ListConversationsExplorerQueryDto {
    pub space_id: Option<String>,
    pub query: Option<String>,
    pub saved_only: Option<bool>,
    pub bookmarked_only: Option<bool>,
    pub pinned_only: Option<bool>,
    pub has_message_bookmarks: Option<bool>,
    pub include_archived: Option<bool>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}
