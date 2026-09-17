//! # Conversation DTOs
//!
//! Data Transfer Objects for conversation management operations.
//!
//! These DTOs handle requests and responses for creating, updating, and managing conversations.

use serde::{Deserialize, Serialize};

/// Conversation representation.
///
/// Simple DTO for conversation data across application boundaries.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConversationDto {
    /// Conversation identifier
    pub id: String,

    /// Conversation title
    pub title: String,

    /// LLM model name (e.g., "claude-sonnet-4-5-20250929")
    pub model_name: String,

    /// Optional system prompt
    pub system_prompt: Option<String>,

    /// Creation timestamp (ISO 8601)
    pub created_at: String,

    /// Last update timestamp (ISO 8601)
    pub updated_at: String,

    /// Number of messages in conversation
    pub message_count: i64,

    /// Total tokens used
    pub total_tokens: i64,

    /// Space/environment identifier that owns this conversation
    pub space_id: Option<String>,

    /// Whether this conversation is saved to the user's shelf
    pub is_saved: Option<bool>,

    /// Whether this conversation is bookmarked/starred
    pub is_bookmarked: Option<bool>,

    /// Whether this conversation is pinned in lists
    pub is_pinned: Option<bool>,

    /// Whether this conversation is archived
    pub is_archived: Option<bool>,

    /// Optional timestamp when conversation was saved
    pub saved_at: Option<String>,

    /// Optional timestamp when conversation was bookmarked
    pub bookmarked_at: Option<String>,

    /// Optional timestamp when conversation was pinned
    pub pinned_at: Option<String>,

    /// Optional timestamp when conversation was archived
    pub archived_at: Option<String>,

    /// Optional preview of the most recent message content
    pub last_message_preview: Option<String>,

    /// Active compaction summary, if the conversation's older messages have
    /// been folded into one (see `conversation_summaries`).
    pub compaction: Option<CompactionRecordDto>,
}

/// Conversation message representation.
///
/// Simple DTO for message data across application boundaries.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MessageDto {
    /// Message identifier
    pub id: String,

    /// Conversation identifier this message belongs to
    pub conversation_id: String,

    /// Message role (user, assistant, system)
    pub role: String,

    /// Message content
    pub content: String,

    /// Token count for this message
    pub tokens: i64,

    /// Creation timestamp (ISO 8601)
    pub created_at: String,

    /// Optional metadata (JSON string)
    pub metadata: Option<String>,

    /// Message status (pending/completed/failed)
    pub status: String,
}

/// Request to create a new conversation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateConversationRequestDto {
    /// Conversation title
    pub title: String,

    /// LLM model name
    pub model_name: String,

    /// Optional system prompt
    pub system_prompt: Option<String>,
}

/// Response from creating a conversation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateConversationResponseDto {
    /// The created conversation
    pub conversation: ConversationDto,

    /// Status message
    pub status: String,
}

/// Query parameters for listing conversations.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ListConversationsQuery {
    /// Maximum number of conversations to return
    pub limit: Option<i64>,

    /// Offset for pagination
    pub offset: Option<i64>,
}

/// Response from listing conversations.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ListConversationsResponseDto {
    /// List of conversations
    pub conversations: Vec<ConversationDto>,

    /// Total count
    pub total: usize,
}

/// Request to get a conversation by ID.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GetConversationRequestDto {
    /// Conversation identifier
    pub conversation_id: String,
}

/// Response from getting a conversation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GetConversationResponseDto {
    /// The conversation (if found)
    pub conversation: Option<ConversationDto>,
}

/// Request to get conversation messages.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GetConversationMessagesRequestDto {
    /// Conversation identifier
    pub conversation_id: String,
}

/// Response from getting conversation messages.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GetConversationMessagesResponseDto {
    /// List of messages
    pub messages: Vec<MessageDto>,

    /// Total count
    pub total: usize,
}

/// Request to rename a conversation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RenameConversationRequestDto {
    /// Conversation identifier
    pub conversation_id: String,

    /// New title
    pub new_title: String,
}

/// Response from renaming a conversation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RenameConversationResponseDto {
    /// Status message
    pub status: String,
}

/// Request to delete a conversation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DeleteConversationRequestDto {
    /// Conversation identifier
    pub conversation_id: String,
}

/// Response from deleting a conversation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DeleteConversationResponseDto {
    /// Status message
    pub status: String,
}

/// Request to compact a conversation's oldest messages into an LLM summary.
///
/// The oldest messages are folded into a summary so the LLM context window
/// carries the distilled past instead of the raw history. The most recent
/// `keep_recent_messages` messages stay raw.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CompactConversationRequestDto {
    /// Conversation identifier
    pub conversation_id: String,

    /// Number of most-recent messages to keep raw (default 4).
    pub keep_recent_messages: Option<i64>,
}

/// A compaction record as returned to the client.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CompactionRecordDto {
    /// Stable identifier for this compaction
    pub id: String,

    /// Conversation this compaction belongs to
    pub conversation_id: String,

    /// The LLM-produced summary of the compacted messages
    pub summary_text: String,

    /// Id of the last message folded into the summary (inclusive boundary)
    pub up_to_message_id: String,

    /// How many messages were folded into the summary
    pub original_message_count: i64,

    /// Total tokens of the folded messages before summarization
    pub original_tokens: i64,

    /// Token count of the summary itself
    pub summary_tokens: i64,

    /// `summary_tokens / original_tokens`, clamped to `(0.0, 1.0]`
    pub compression_ratio: f64,

    /// When the compaction was created (ISO 8601)
    pub created_at: String,
}

impl CompactionRecordDto {
    /// Build a DTO from the domain record.
    pub fn from_record(record: &crate::domain::conversation::CompactionRecord) -> Self {
        Self {
            id: record.id.clone(),
            conversation_id: record.conversation_id.to_string(),
            summary_text: record.summary_text.clone(),
            up_to_message_id: record.up_to_message_id.clone(),
            original_message_count: record.original_message_count,
            original_tokens: record.original_tokens,
            summary_tokens: record.summary_tokens,
            compression_ratio: record.compression_ratio,
            created_at: record.created_at.to_rfc3339(),
        }
    }
}

/// Response from compacting a conversation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CompactConversationResponseDto {
    /// The compaction that was applied
    pub compaction: CompactionRecordDto,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversation_dto_serialization() {
        let conversation = ConversationDto {
            id: "conv-123".to_string(),
            title: "My Conversation".to_string(),
            model_name: "claude-sonnet-4-5-20250929".to_string(),
            system_prompt: Some("You are a helpful assistant.".to_string()),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            updated_at: "2025-01-01T00:00:00Z".to_string(),
            message_count: 5,
            total_tokens: 1000,
            space_id: Some("space_general".to_string()),
            is_saved: Some(false),
            is_bookmarked: Some(false),
            is_pinned: Some(false),
            is_archived: Some(false),
            saved_at: None,
            bookmarked_at: None,
            pinned_at: None,
            archived_at: None,
            last_message_preview: None,
            compaction: None,
        };

        let json = serde_json::to_string(&conversation).unwrap();
        let deserialized: ConversationDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, "conv-123");
        assert_eq!(deserialized.title, "My Conversation");
        assert_eq!(deserialized.message_count, 5);
    }

    #[test]
    fn test_message_dto_serialization() {
        let message = MessageDto {
            id: "msg-456".to_string(),
            conversation_id: "conv-123".to_string(),
            role: "user".to_string(),
            content: "Hello!".to_string(),
            tokens: 5,
            created_at: "2025-01-01T00:00:00Z".to_string(),
            metadata: None,
            status: "completed".to_string(),
        };

        let json = serde_json::to_string(&message).unwrap();
        let deserialized: MessageDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, "msg-456");
        assert_eq!(deserialized.role, "user");
        assert_eq!(deserialized.content, "Hello!");
        assert_eq!(deserialized.status, "completed");
    }

    #[test]
    fn test_create_conversation_request_dto_serialization() {
        let request = CreateConversationRequestDto {
            title: "New Chat".to_string(),
            model_name: "claude-sonnet-4-5-20250929".to_string(),
            system_prompt: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: CreateConversationRequestDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.title, "New Chat");
        assert_eq!(deserialized.model_name, "claude-sonnet-4-5-20250929");
    }

    #[test]
    fn test_list_conversations_query_serialization() {
        let query = ListConversationsQuery {
            limit: Some(10),
            offset: Some(0),
        };

        let json = serde_json::to_string(&query).unwrap();
        let deserialized: ListConversationsQuery = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.limit, Some(10));
        assert_eq!(deserialized.offset, Some(0));
    }

    #[test]
    fn test_rename_conversation_request_dto_serialization() {
        let request = RenameConversationRequestDto {
            conversation_id: "conv-789".to_string(),
            new_title: "Updated Title".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: RenameConversationRequestDto = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.conversation_id, "conv-789");
        assert_eq!(deserialized.new_title, "Updated Title");
    }
}
