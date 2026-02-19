//! # Conversation Mapper
//!
//! Converts between domain conversation models and DTOs.
//!
//! This mapper handles conversion between conversation domain models and their DTO representations.

use crate::application::dtos::conversation_dto::{ConversationDto, MessageDto};
use crate::domain::{Conversation, ConversationAggregate, ConversationMessage};

/// Mapper for conversation-related conversions.
pub struct ConversationMapper;

impl ConversationMapper {
    /// Convert Conversation domain model to DTO.
    ///
    /// # Arguments
    ///
    /// * `conversation` - Domain conversation entity
    ///
    /// # Returns
    ///
    /// Conversation DTO for JSON serialization
    pub fn to_dto(conversation: &Conversation) -> ConversationDto {
        ConversationDto {
            id: conversation.id.to_string(),
            title: conversation.title.clone(),
            model_name: conversation.model_name.clone(),
            system_prompt: conversation.system_prompt.clone(),
            created_at: conversation.created_at.to_rfc3339(),
            updated_at: conversation.updated_at.to_rfc3339(),
            message_count: conversation.message_count,
            total_tokens: conversation.total_tokens,
            space_id: None,
            is_saved: None,
            is_bookmarked: None,
            is_pinned: None,
            is_archived: None,
            saved_at: None,
            bookmarked_at: None,
            pinned_at: None,
            archived_at: None,
            last_message_preview: None,
        }
    }

    /// Convert ConversationAggregate to DTO (extracts the conversation entity).
    ///
    /// # Arguments
    ///
    /// * `aggregate` - Conversation aggregate root
    ///
    /// # Returns
    ///
    /// Conversation DTO for JSON serialization
    pub fn aggregate_to_dto(aggregate: &ConversationAggregate) -> ConversationDto {
        Self::to_dto(aggregate.conversation())
    }
}

/// Mapper for message-related conversions.
pub struct MessageMapper;

impl MessageMapper {
    /// Convert ConversationMessage domain model to DTO.
    ///
    /// # Arguments
    ///
    /// * `message` - Domain message entity
    ///
    /// # Returns
    ///
    /// Message DTO for JSON serialization
    pub fn to_dto(message: &ConversationMessage) -> MessageDto {
        MessageDto {
            id: message.id.clone(),
            conversation_id: message.conversation_id.to_string(),
            role: message.role.to_string(),
            content: message.content.clone(),
            tokens: message.tokens,
            created_at: message.created_at.to_rfc3339(),
            metadata: message.metadata.clone(),
            status: message.status.clone(),
        }
    }

    /// Convert a list of ConversationMessages to DTOs.
    ///
    /// # Arguments
    ///
    /// * `messages` - Slice of domain message entities
    ///
    /// # Returns
    ///
    /// Vector of message DTOs for JSON serialization
    pub fn to_dto_list(messages: &[ConversationMessage]) -> Vec<MessageDto> {
        messages.iter().map(Self::to_dto).collect()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::MessageRole;
    use crate::shared::domain_types::ConversationId;
    use chrono::Utc;

    #[test]
    fn test_conversation_to_dto() {
        let conversation = Conversation {
            id: ConversationId::new(),
            title: "Test Conversation".to_string(),
            model_name: "claude-sonnet-4-5-20250929".to_string(),
            system_prompt: Some("You are helpful".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            message_count: 5,
            total_tokens: 1000,
        };

        let dto = ConversationMapper::to_dto(&conversation);

        assert_eq!(dto.id, conversation.id.to_string());
        assert_eq!(dto.title, "Test Conversation");
        assert_eq!(dto.model_name, "claude-sonnet-4-5-20250929");
        assert_eq!(dto.message_count, 5);
        assert_eq!(dto.total_tokens, 1000);
    }

    #[test]
    fn test_message_to_dto() {
        let conversation_id = ConversationId::new();
        let message = ConversationMessage {
            id: "msg-123".to_string(),
            conversation_id: conversation_id.clone(),
            role: MessageRole::User,
            content: "Hello!".to_string(),
            tokens: 5,
            created_at: Utc::now(),
            metadata: None,
            status: "completed".to_string(),
        };

        let dto = MessageMapper::to_dto(&message);

        assert_eq!(dto.id, "msg-123");
        assert_eq!(dto.conversation_id, conversation_id.to_string());
        assert_eq!(dto.role, "user");
        assert_eq!(dto.content, "Hello!");
        assert_eq!(dto.tokens, 5);
    }

    #[test]
    fn test_message_to_dto_list() {
        let conversation_id = ConversationId::new();
        let messages = vec![
            ConversationMessage {
                id: "msg-1".to_string(),
                conversation_id: conversation_id.clone(),
                role: MessageRole::User,
                content: "Question".to_string(),
                tokens: 5,
                created_at: Utc::now(),
                metadata: None,
                status: "completed".to_string(),
            },
            ConversationMessage {
                id: "msg-2".to_string(),
                conversation_id: conversation_id.clone(),
                role: MessageRole::Assistant,
                content: "Answer".to_string(),
                tokens: 10,
                created_at: Utc::now(),
                metadata: None,
                status: "completed".to_string(),
            },
        ];

        let dtos = MessageMapper::to_dto_list(&messages);

        assert_eq!(dtos.len(), 2);
        assert_eq!(dtos[0].role, "user");
        assert_eq!(dtos[1].role, "assistant");
    }

    #[test]
    fn test_aggregate_to_dto() {
        let aggregate = ConversationAggregate::new(
            "Test Aggregate".to_string(),
            "claude-sonnet-4-5-20250929".to_string(),
            Some("System prompt".to_string()),
        )
        .unwrap();

        let dto = ConversationMapper::aggregate_to_dto(&aggregate);

        assert_eq!(dto.title, "Test Aggregate");
        assert_eq!(dto.model_name, "claude-sonnet-4-5-20250929");
        assert_eq!(dto.message_count, 0);
    }
}
