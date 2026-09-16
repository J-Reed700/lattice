//! # Conversation Mapper
//!
//! Maps between database models and domain entities for conversations.
//!
//! This mapper handles the transformation of conversation-related data:
//! - Conversation metadata
//! - Conversation messages
//! - Document references

use crate::domain::conversation::{
    Conversation, ConversationMessage, DocumentReference, MessageRole,
};
use crate::domain_types::ConversationId;
use crate::shared::error::{AppError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

// ============================================================================
// Database Models (Anemic DTOs)
// ============================================================================

/// Database model for conversations.
///
/// Anemic data structure that mirrors the database schema.
/// Contains no business logic - only data transfer.
///
/// **Important**: This type should NEVER leak outside the infrastructure layer.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ConversationModel {
    pub id: String,
    pub title: String,
    pub model_name: String,
    pub system_prompt: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: i64,
    pub total_tokens: i64,
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

// ============================================================================
// Conversation Mapper
// ============================================================================

/// Mapper for Conversation entity and database model.
///
/// Provides bidirectional conversion between domain entities and database models.
pub struct ConversationRowMapper;

impl ConversationRowMapper {
    /// Convert domain entity to database model.
    ///
    /// # Arguments
    ///
    /// * `entity` - Domain conversation entity
    ///
    /// # Returns
    ///
    /// Database model ready for persistence.
    pub fn to_model(entity: &Conversation) -> ConversationModel {
        ConversationModel {
            id: entity.id.to_string(),
            title: entity.title.clone(),
            model_name: entity.model_name.clone(),
            system_prompt: entity.system_prompt.clone(),
            created_at: entity.created_at.to_rfc3339(),
            updated_at: entity.updated_at.to_rfc3339(),
            message_count: entity.message_count,
            total_tokens: entity.total_tokens,
        }
    }

    /// Convert database model to domain entity.
    ///
    /// # Arguments
    ///
    /// * `model` - Database model from SQLx
    ///
    /// # Returns
    ///
    /// Domain entity with business logic.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidData` if timestamps cannot be parsed
    /// - `AppError::InvalidData` if conversation ID is invalid
    pub fn to_entity(model: &ConversationModel) -> Result<Conversation> {
        // Parse timestamps
        let created_at = DateTime::parse_from_rfc3339(&model.created_at)
            .map_err(|e| AppError::InvalidData(format!("Invalid created_at timestamp: {}", e)))?
            .with_timezone(&Utc);

        let updated_at = DateTime::parse_from_rfc3339(&model.updated_at)
            .map_err(|e| AppError::InvalidData(format!("Invalid updated_at timestamp: {}", e)))?
            .with_timezone(&Utc);

        // Parse conversation ID
        let id = ConversationId::from_str(&model.id)?;

        Ok(Conversation {
            id,
            title: model.title.clone(),
            model_name: model.model_name.clone(),
            system_prompt: model.system_prompt.clone(),
            created_at,
            updated_at,
            message_count: model.message_count,
            total_tokens: model.total_tokens,
        })
    }

    /// Convert a batch of database models to domain entities.
    ///
    /// Continues processing on errors, collecting all successful conversions.
    ///
    /// # Arguments
    ///
    /// * `models` - Slice of database models
    ///
    /// # Returns
    ///
    /// Vector of successfully converted entities.
    pub fn to_entities(models: &[ConversationModel]) -> Vec<Conversation> {
        models
            .iter()
            .filter_map(|model| Self::to_entity(model).ok())
            .collect()
    }
}

// ============================================================================
// Message Mapper
// ============================================================================

/// Mapper for ConversationMessage entity and database model.
pub struct ConversationMessageMapper;

impl ConversationMessageMapper {
    /// Convert domain entity to database model.
    pub fn to_model(entity: &ConversationMessage) -> ConversationMessageModel {
        ConversationMessageModel {
            id: entity.id.clone(),
            conversation_id: entity.conversation_id.to_string(),
            role: entity.role.to_string(),
            content: entity.content.clone(),
            tokens: entity.tokens,
            created_at: entity.created_at.to_rfc3339(),
            metadata: entity.metadata.clone(),
            status: entity.status.clone(),
        }
    }

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

// ============================================================================
// Document Reference Mapper
// ============================================================================

/// Mapper for DocumentReference entity and database model.
pub struct DocumentReferenceMapper;

impl DocumentReferenceMapper {
    /// Convert domain entity to database model.
    pub fn to_model(entity: &DocumentReference) -> DocumentReferenceModel {
        DocumentReferenceModel {
            document_id: entity.document_id.clone(),
            chunk_id: entity.chunk_id.clone(),
            relevance_score: entity.relevance_score,
            added_at: entity.added_at.to_rfc3339(),
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_conversation_mapper_round_trip() {
        let entity = Conversation {
            id: ConversationId::new(),
            title: "Test Chat".to_string(),
            model_name: "claude-sonnet-4-5-20250929".to_string(),
            system_prompt: Some("Be helpful".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            message_count: 5,
            total_tokens: 1000,
        };

        let model = ConversationRowMapper::to_model(&entity);
        let converted = ConversationRowMapper::to_entity(&model).unwrap();

        assert_eq!(converted.id, entity.id);
        assert_eq!(converted.title, entity.title);
        assert_eq!(converted.model_name, entity.model_name);
        assert_eq!(converted.system_prompt, entity.system_prompt);
        assert_eq!(converted.message_count, entity.message_count);
        assert_eq!(converted.total_tokens, entity.total_tokens);
    }

    #[test]
    fn test_message_mapper_round_trip() {
        let entity = ConversationMessage {
            id: "msg-123".to_string(),
            conversation_id: ConversationId::new(),
            role: MessageRole::User,
            content: "Hello!".to_string(),
            tokens: 10,
            created_at: Utc::now(),
            metadata: Some(r#"{"key":"value"}"#.to_string()),
            status: "completed".to_string(),
        };

        let model = ConversationMessageMapper::to_model(&entity);
        let converted = ConversationMessageMapper::to_entity(&model).unwrap();

        assert_eq!(converted.id, entity.id);
        assert_eq!(converted.conversation_id, entity.conversation_id);
        assert_eq!(converted.role, entity.role);
        assert_eq!(converted.content, entity.content);
        assert_eq!(converted.tokens, entity.tokens);
        assert_eq!(converted.metadata, entity.metadata);
        assert_eq!(converted.status, entity.status);
    }

    #[test]
    fn test_document_reference_mapper_round_trip() {
        let entity = DocumentReference {
            document_id: "doc-123".to_string(),
            chunk_id: Some("chunk-456".to_string()),
            relevance_score: Some(0.95),
            added_at: Utc::now(),
        };

        let model = DocumentReferenceMapper::to_model(&entity);
        let converted = DocumentReferenceMapper::to_entity(&model).unwrap();

        assert_eq!(converted.document_id, entity.document_id);
        assert_eq!(converted.chunk_id, entity.chunk_id);
        assert_eq!(converted.relevance_score, entity.relevance_score);
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
