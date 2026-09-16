//! # Conversation Mapper
//!
//! Converts between domain conversation models and DTOs.
//!
//! This mapper handles conversion between conversation domain models and their DTO representations.

use crate::domain::{Conversation, ConversationAggregate};
use crate::features::conversation::dto::ConversationDto;

/// Mapper for conversation-related conversions.
pub struct ConversationDtoMapper;

impl ConversationDtoMapper {
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

#[cfg(test)]
mod tests {
    use super::*;
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

        let dto = ConversationDtoMapper::to_dto(&conversation);

        assert_eq!(dto.id, conversation.id.to_string());
        assert_eq!(dto.title, "Test Conversation");
        assert_eq!(dto.model_name, "claude-sonnet-4-5-20250929");
        assert_eq!(dto.message_count, 5);
        assert_eq!(dto.total_tokens, 1000);
    }

    #[test]
    fn test_aggregate_to_dto() {
        let aggregate = ConversationAggregate::new(
            "Test Aggregate".to_string(),
            "claude-sonnet-4-5-20250929".to_string(),
            Some("System prompt".to_string()),
        )
        .unwrap();

        let dto = ConversationDtoMapper::aggregate_to_dto(&aggregate);

        assert_eq!(dto.title, "Test Aggregate");
        assert_eq!(dto.model_name, "claude-sonnet-4-5-20250929");
        assert_eq!(dto.message_count, 0);
    }
}
