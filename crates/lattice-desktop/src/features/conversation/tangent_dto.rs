//! Tangents are ordinary conversation transcripts with a parent and a passage.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateTangentRequestDto {
    pub conversation_id: String,
    pub message_id: String,
    pub selected_text: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConversationTangentDto {
    pub conversation_id: String,
    pub parent_conversation_id: String,
    pub source_conversation_id: String,
    pub source_message_id: String,
    pub selected_text: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    /// The inherited transcript is context, not the tangent's own discussion.
    pub context_message_count: i64,
}
