//! Maps between database models and domain entities for conversations, their
//! messages and document references.

use chrono::{DateTime, Utc};
use lattice_core::domain::conversation::{
    ChatModelChoice, CompactionRecord, Conversation, ConversationMessage, DocumentReference,
    MessageRole,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::types::ConversationId;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Database model for conversations, mirroring the schema. Must not leak
/// outside the infrastructure layer.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ConversationModel {
    pub id: String,
    pub title: String,
    pub model_choice: Option<String>,
    pub system_prompt: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: i64,
    pub total_tokens: i64,
}

/// `conversations.model_choice`, in [`ChatModelChoice`]'s stored form.
pub struct ModelChoiceColumn;

impl ModelChoiceColumn {
    pub fn to_column(choice: &ChatModelChoice) -> Result<Option<String>> {
        choice
            .to_stored()
            .map_err(|e| AppError::Serialization(format!("model choice {choice:?}: {e}")))
    }
}

/// Database model for conversation messages.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ConversationMessageModel {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub tokens: i64,
    pub created_at: String,
    pub metadata: Option<String>,
    pub status: String, // 'pending', 'completed', 'failed'
}

/// Database model for document references.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DocumentReferenceModel {
    pub document_id: String,
    pub chunk_id: Option<String>,
    pub relevance_score: Option<f32>,
    pub added_at: String,
}

/// Database model for conversation summaries (context compaction).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ConversationSummaryModel {
    pub id: String,
    pub conversation_id: String,
    pub summary_text: String,
    pub up_to_message_id: String,
    pub original_message_count: i64,
    pub original_tokens: i64,
    pub summary_tokens: i64,
    pub compression_ratio: f64,
    pub created_at: String,
}

/// Mapper for Conversation entity and database model.
///
/// Provides bidirectional conversion between domain entities and database models.
pub struct ConversationRowMapper;

impl ConversationRowMapper {
    /// Converts a database model to a domain conversation.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidData` if timestamps cannot be parsed
    /// - `AppError::InvalidData` if conversation ID is invalid
    /// - `AppError::Database` if the model choice is unreadable
    pub fn to_entity(model: &ConversationModel) -> Result<Conversation> {
        let created_at = DateTime::parse_from_rfc3339(&model.created_at)
            .map_err(|e| AppError::InvalidData(format!("Invalid created_at timestamp: {}", e)))?
            .with_timezone(&Utc);

        let updated_at = DateTime::parse_from_rfc3339(&model.updated_at)
            .map_err(|e| AppError::InvalidData(format!("Invalid updated_at timestamp: {}", e)))?
            .with_timezone(&Utc);

        let id = ConversationId::from_str(&model.id)?;

        Ok(Conversation {
            id,
            title: model.title.clone(),
            model_choice: ChatModelChoice::from_column(&model.id, model.model_choice.as_deref())?,
            system_prompt: model.system_prompt.clone(),
            created_at,
            updated_at,
            message_count: model.message_count,
            total_tokens: model.total_tokens,
        })
    }

    /// Converts a batch of database models, skipping any that fail to convert.
    pub fn to_entities(models: &[ConversationModel]) -> Vec<Conversation> {
        models
            .iter()
            .filter_map(|model| Self::to_entity(model).ok())
            .collect()
    }
}

/// Mapper for ConversationMessage entity and database model.
pub struct ConversationMessageMapper;

impl ConversationMessageMapper {
    /// Convert database model to domain entity.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidData` if timestamps cannot be parsed
    /// - `AppError::InvalidData` if conversation ID is invalid
    /// - `AppError::InvalidInput` if message role is invalid
    pub fn to_entity(model: &ConversationMessageModel) -> Result<ConversationMessage> {
        let created_at = DateTime::parse_from_rfc3339(&model.created_at)
            .map_err(|e| AppError::InvalidData(format!("Invalid created_at timestamp: {}", e)))?
            .with_timezone(&Utc);

        let conversation_id = ConversationId::from_str(&model.conversation_id)?;
        let role = MessageRole::from_str(&model.role)?;

        Ok(ConversationMessage {
            id: model.id.clone(),
            conversation_id,
            role,
            content: model.content.clone(),
            tokens: model.tokens,
            created_at,
            metadata: model.metadata.clone(),
            status: model.status.clone(),
        })
    }

    /// Convert a batch of database models to domain entities.
    pub fn to_entities(models: &[ConversationMessageModel]) -> Vec<ConversationMessage> {
        models
            .iter()
            .filter_map(|model| Self::to_entity(model).ok())
            .collect()
    }
}

/// Mapper for DocumentReference entity and database model.
pub struct DocumentReferenceMapper;

impl DocumentReferenceMapper {
    /// Convert database model to domain entity.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidData` if timestamp cannot be parsed
    pub fn to_entity(model: &DocumentReferenceModel) -> Result<DocumentReference> {
        let added_at = DateTime::parse_from_rfc3339(&model.added_at)
            .map_err(|e| AppError::InvalidData(format!("Invalid added_at timestamp: {}", e)))?
            .with_timezone(&Utc);

        Ok(DocumentReference {
            document_id: model.document_id.clone(),
            chunk_id: model.chunk_id.clone(),
            relevance_score: model.relevance_score,
            added_at,
        })
    }

    /// Convert a batch of database models to domain entities.
    pub fn to_entities(models: &[DocumentReferenceModel]) -> Vec<DocumentReference> {
        models
            .iter()
            .filter_map(|model| Self::to_entity(model).ok())
            .collect()
    }
}

/// Mapper for CompactionRecord entity and database model.
pub struct ConversationSummaryMapper;

impl ConversationSummaryMapper {
    /// Convert database model to domain entity.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidData` if the timestamp cannot be parsed
    /// - `AppError::InvalidData` if the conversation ID is invalid
    pub fn to_entity(model: &ConversationSummaryModel) -> Result<CompactionRecord> {
        let created_at = DateTime::parse_from_rfc3339(&model.created_at)
            .map_err(|e| AppError::InvalidData(format!("Invalid created_at timestamp: {}", e)))?
            .with_timezone(&Utc);

        let conversation_id = ConversationId::from_str(&model.conversation_id)?;

        Ok(CompactionRecord {
            id: model.id.clone(),
            conversation_id,
            summary_text: model.summary_text.clone(),
            up_to_message_id: model.up_to_message_id.clone(),
            original_message_count: model.original_message_count,
            original_tokens: model.original_tokens,
            summary_tokens: model.summary_tokens,
            compression_ratio: model.compression_ratio,
            created_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use lattice_core::domain::models::model_ref::ModelRef;

    #[test]
    fn test_conversation_mapper_round_trip() {
        let id = ConversationId::new();
        let choice = ChatModelChoice::Explicit {
            model: ModelRef::Local {
                model_id: "ornith-1.5-9b".into(),
            },
        };
        let model = ConversationModel {
            id: id.to_string(),
            title: "Test Chat".to_string(),
            model_choice: ModelChoiceColumn::to_column(&choice).unwrap(),
            system_prompt: Some("Be helpful".to_string()),
            created_at: "2026-01-02T03:04:05+00:00".to_string(),
            updated_at: "2026-01-03T04:05:06+00:00".to_string(),
            message_count: 5,
            total_tokens: 1000,
        };

        let converted = ConversationRowMapper::to_entity(&model).unwrap();

        assert_eq!(converted.id, id);
        assert_eq!(converted.title, "Test Chat");
        assert_eq!(converted.model_choice, choice);
        assert_eq!(converted.system_prompt.as_deref(), Some("Be helpful"));
        assert_eq!(
            converted.created_at,
            DateTime::parse_from_rfc3339("2026-01-02T03:04:05+00:00")
                .unwrap()
                .with_timezone(&Utc)
        );
        assert_eq!(
            converted.updated_at,
            DateTime::parse_from_rfc3339("2026-01-03T04:05:06+00:00")
                .unwrap()
                .with_timezone(&Utc)
        );
        assert_eq!(converted.message_count, 5);
        assert_eq!(converted.total_tokens, 1000);
    }

    #[test]
    fn an_unreadable_model_choice_names_its_conversation() {
        for unreadable in ["", r#"{"kind":"local"}"#] {
            let error = ChatModelChoice::from_column("chat-7", Some(unreadable)).unwrap_err();
            assert!(
                matches!(&error, AppError::Database(message)
                    if message.starts_with("conversation chat-7 has an unreadable model_choice: ")),
                "{unreadable:?} gave {error:?}"
            );
        }
    }

    #[test]
    fn test_message_mapper_round_trip() {
        let conversation_id = ConversationId::new();
        let model = ConversationMessageModel {
            id: "msg-123".to_string(),
            conversation_id: conversation_id.to_string(),
            role: MessageRole::User.to_string(),
            content: "Hello!".to_string(),
            tokens: 10,
            created_at: "2026-01-02T03:04:05+00:00".to_string(),
            metadata: Some(r#"{"key":"value"}"#.to_string()),
            status: "completed".to_string(),
        };

        let converted = ConversationMessageMapper::to_entity(&model).unwrap();

        assert_eq!(converted.id, "msg-123");
        assert_eq!(converted.conversation_id, conversation_id);
        assert_eq!(converted.role, MessageRole::User);
        assert_eq!(converted.content, "Hello!");
        assert_eq!(converted.tokens, 10);
        assert_eq!(
            converted.created_at,
            DateTime::parse_from_rfc3339("2026-01-02T03:04:05+00:00")
                .unwrap()
                .with_timezone(&Utc)
        );
        assert_eq!(converted.metadata.as_deref(), Some(r#"{"key":"value"}"#));
        assert_eq!(converted.status, "completed");
    }

    #[test]
    fn test_document_reference_mapper_round_trip() {
        let model = DocumentReferenceModel {
            document_id: "doc-123".to_string(),
            chunk_id: Some("chunk-456".to_string()),
            relevance_score: Some(0.95),
            added_at: "2026-01-02T03:04:05+00:00".to_string(),
        };

        let converted = DocumentReferenceMapper::to_entity(&model).unwrap();

        assert_eq!(converted.document_id, "doc-123");
        assert_eq!(converted.chunk_id.as_deref(), Some("chunk-456"));
        assert_eq!(converted.relevance_score, Some(0.95));
        assert_eq!(
            converted.added_at,
            DateTime::parse_from_rfc3339("2026-01-02T03:04:05+00:00")
                .unwrap()
                .with_timezone(&Utc)
        );
    }

    #[test]
    fn test_compaction_summary_mapper_round_trip() {
        let conversation_id = ConversationId::new();
        let model = ConversationSummaryModel {
            id: "sum-123".to_string(),
            conversation_id: conversation_id.to_string(),
            summary_text: "User asked about RAG; assistant explained it.".to_string(),
            up_to_message_id: "msg-456".to_string(),
            original_message_count: 6,
            original_tokens: 4000,
            summary_tokens: 250,
            compression_ratio: 0.0625,
            created_at: "2026-01-02T03:04:05+00:00".to_string(),
        };

        let converted = ConversationSummaryMapper::to_entity(&model).unwrap();

        assert_eq!(converted.id, "sum-123");
        assert_eq!(converted.conversation_id, conversation_id);
        assert_eq!(
            converted.summary_text,
            "User asked about RAG; assistant explained it."
        );
        assert_eq!(converted.up_to_message_id, "msg-456");
        assert_eq!(converted.original_message_count, 6);
        assert_eq!(converted.original_tokens, 4000);
        assert_eq!(converted.summary_tokens, 250);
        assert!((converted.compression_ratio - 0.0625).abs() < f64::EPSILON);
        assert_eq!(
            converted.created_at,
            DateTime::parse_from_rfc3339("2026-01-02T03:04:05+00:00")
                .unwrap()
                .with_timezone(&Utc)
        );
    }

    #[test]
    fn test_message_role_conversion() {
        for role in [
            MessageRole::User,
            MessageRole::Assistant,
            MessageRole::System,
        ] {
            let role_str = role.to_string();
            let parsed = MessageRole::from_str(&role_str).unwrap();
            assert_eq!(parsed, role);
        }
    }

    #[test]
    fn test_invalid_message_role() {
        let result = MessageRole::from_str("invalid");
        assert!(result.is_err());
    }
}
