//! Converts between domain conversation models and DTOs.

use crate::features::conversation::dto::{CompactionRecordDto, ConversationDto};
use lattice_core::domain::{Conversation, ConversationAggregate};

/// Mapper for conversation-related conversions.
pub struct ConversationDtoMapper;

impl ConversationDtoMapper {
    /// Convert Conversation domain model to DTO.
    pub fn to_dto(conversation: &Conversation) -> ConversationDto {
        ConversationDto {
            id: conversation.id.to_string(),
            title: conversation.title.clone(),
            model_choice: conversation.model_choice.clone(),
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
            compaction: None,
            forked_from_conversation_id: None,
            forked_from_message_id: None,
            explorer_root: None,
            tangent_parent_id: None,
        }
    }

    /// Convert ConversationAggregate to DTO (extracts the conversation entity).
    pub fn aggregate_to_dto(aggregate: &ConversationAggregate) -> ConversationDto {
        let mut dto = Self::to_dto(aggregate.conversation());
        dto.compaction = aggregate.compaction().map(CompactionRecordDto::from_record);
        dto
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use lattice_core::domain::conversation::ChatModelChoice;
    use lattice_core::domain::models::model_ref::ModelRef;
    use lattice_core::shared::types::ConversationId;

    #[test]
    fn test_conversation_to_dto() {
        let conversation = Conversation {
            id: ConversationId::new(),
            title: "Test Conversation".to_string(),
            model_choice: ChatModelChoice::Explicit {
                model: ModelRef::Local {
                    model_id: "ornith-1.5-9b".into(),
                },
            },
            system_prompt: Some("You are helpful".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            message_count: 5,
            total_tokens: 1000,
        };

        let dto = ConversationDtoMapper::to_dto(&conversation);

        assert_eq!(dto.id, conversation.id.to_string());
        assert_eq!(dto.title, "Test Conversation");
        assert_eq!(dto.model_choice, conversation.model_choice);
        assert_eq!(dto.message_count, 5);
        assert_eq!(dto.total_tokens, 1000);
    }

    #[test]
    fn test_aggregate_to_dto() {
        let aggregate = ConversationAggregate::new(
            "Test Aggregate".to_string(),
            Some("System prompt".to_string()),
        )
        .unwrap();

        let dto = ConversationDtoMapper::aggregate_to_dto(&aggregate);

        assert_eq!(dto.title, "Test Aggregate");
        assert_eq!(dto.model_choice, ChatModelChoice::Default);
        assert_eq!(dto.message_count, 0);
    }
}
