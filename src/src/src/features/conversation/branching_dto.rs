//! DTOs for regenerate, edit-and-resend, and branch operations.
//!
//! No schema change: a branch is a sibling conversation with copied messages.
//! `MessageDto` and `ConversationDto` are reused rather than mirrored.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TruncateConversationAfterRequestDto {
    pub conversation_id: String,
    pub message_id: String,
    /// When true, `message_id` itself is deleted as well. Defaults to false.
    #[serde(default)]
    pub inclusive: bool,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TruncateConversationAfterResponseDto {
    pub conversation_id: String,
    pub deleted_count: u32,
    /// The conversation's remaining messages, oldest first.
    pub messages: Vec<crate::features::conversation::dto::MessageDto>,
}

#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ForkConversationRequestDto {
    pub conversation_id: String,
    /// Copy messages up to and including this id. `None` copies everything.
    #[serde(default)]
    pub up_to_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ForkConversationResponseDto {
    pub conversation: crate::features::conversation::dto::ConversationDto,
    pub copied_message_count: u32,
}
