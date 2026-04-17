//! Get Conversation Messages Use Case

use crate::application::dtos::conversation_dto::{
    GetConversationMessagesRequestDto, GetConversationMessagesResponseDto,
};
use crate::application::mappers::conversation_mapper::MessageMapper;
use crate::infrastructure::services::traits::ConversationServiceTrait;
use crate::shared::error::{AppError, Result};
use std::sync::Arc;

/// Use case for getting all messages for a conversation
pub struct GetConversationMessagesUseCase {
    conversation_service: Arc<dyn ConversationServiceTrait>,
}

impl GetConversationMessagesUseCase {
    /// Create a new use case instance
    pub fn new(conversation_service: Arc<dyn ConversationServiceTrait>) -> Self {
        Self {
            conversation_service,
        }
    }

    /// Execute the use case
    ///
    /// # Arguments
    ///
    /// * `request` - GetConversationMessagesRequestDto with conversation_id
    ///
    /// # Returns
    ///
    /// GetConversationMessagesResponseDto with messages list
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::Database` if database query fails
    pub async fn execute(
        &self,
        request: GetConversationMessagesRequestDto,
    ) -> Result<GetConversationMessagesResponseDto> {
        // Get conversation aggregate from service
        let aggregate = self
            .conversation_service
            .get_conversation(&request.conversation_id)
            .await?
            .ok_or_else(|| {
                AppError::NotFound(format!(
                    "Conversation not found: {}",
                    request.conversation_id
                ))
            })?;

        // Get messages from aggregate
        let messages = aggregate.messages();

        // Convert to DTOs
        let message_dtos = MessageMapper::to_dto_list(messages);
        let total = message_dtos.len();

        Ok(GetConversationMessagesResponseDto {
            messages: message_dtos,
            total,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Conversation, ConversationAggregate, MessageRole};
    use crate::infrastructure::services::traits::ConversationServiceTrait;
    use crate::shared::domain_types::ConversationId;
    use crate::shared::error::AppError;
    use async_trait::async_trait;
    use chrono::Utc;

    struct MockConversationService {
        aggregate: Option<ConversationAggregate>,
    }

    impl MockConversationService {
        fn new() -> Self {
            let mut aggregate = ConversationAggregate::new(
                "Test Conversation".to_string(),
                "claude-sonnet-4-5-20250929".to_string(),
                Some("System prompt".to_string()),
            )
            .unwrap();

            // Add some messages
            aggregate
                .add_message(MessageRole::User, "Hello!".to_string(), 5)
                .unwrap();
            aggregate
                .add_message(MessageRole::Assistant, "Hi there!".to_string(), 10)
                .unwrap();

            Self {
                aggregate: Some(aggregate),
            }
        }

        fn with_none() -> Self {
            Self { aggregate: None }
        }
    }

    #[async_trait]
    impl ConversationServiceTrait for MockConversationService {
        async fn create_conversation(
            &self,
            _title: String,
            _model_name: String,
            _system_prompt: Option<String>,
        ) -> Result<Conversation> {
            unimplemented!()
        }

        async fn list_conversations(
            &self,
            _limit: Option<i64>,
            _offset: Option<i64>,
        ) -> Result<Vec<Conversation>> {
            unimplemented!()
        }

        async fn get_conversation(
            &self,
            _conversation_id: &str,
        ) -> Result<Option<ConversationAggregate>> {
            Ok(self.aggregate.clone())
        }

        async fn rename_conversation(
            &self,
            _conversation_id: &str,
            _new_title: String,
        ) -> Result<()> {
            unimplemented!()
        }

        async fn delete_conversation(&self, _conversation_id: &str) -> Result<()> {
            unimplemented!()
        }

        async fn update_system_prompt(
            &self,
            _id: &str,
            _system_prompt: Option<String>,
        ) -> Result<()> {
            unimplemented!()
        }

        async fn add_user_message(
            &self,
            _conversation_id: &str,
            _content: String,
            _tokens: i64,
        ) -> Result<crate::domain::conversation::ConversationMessage> {
            unimplemented!()
        }

        async fn add_assistant_message(
            &self,
            _conversation_id: &str,
            _content: String,
            _tokens: i64,
        ) -> Result<crate::domain::conversation::ConversationMessage> {
            unimplemented!()
        }

        async fn prune_conversation_to_limit(
            &self,
            _conversation_id: &str,
            _max_tokens: i64,
        ) -> Result<()> {
            unimplemented!()
        }

        async fn add_document_reference(
            &self,
            _conversation_id: &str,
            _document_id: String,
            _chunk_id: Option<String>,
            _relevance_score: Option<f32>,
        ) -> Result<()> {
            unimplemented!()
        }

        async fn add_assistant_message_with_metadata(
            &self,
            _conversation_id: &str,
            _content: String,
            _tokens: i64,
            _metadata: Option<String>,
        ) -> Result<crate::domain::conversation::ConversationMessage> {
            unimplemented!()
        }

        async fn add_message_with_status(
            &self,
            _conversation_id: &str,
            _role: crate::domain::conversation::MessageRole,
            _content: String,
            _tokens: i64,
            _status: String,
        ) -> Result<crate::domain::conversation::ConversationMessage> {
            unimplemented!()
        }

        async fn update_message_status(&self, _message_id: &str, _status: String) -> Result<()> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn test_get_conversation_messages_success() {
        let mock_service = Arc::new(MockConversationService::new());
        let use_case = GetConversationMessagesUseCase::new(mock_service);

        let request = GetConversationMessagesRequestDto {
            conversation_id: "conv-123".to_string(),
        };

        let response = use_case.execute(request).await.unwrap();

        assert_eq!(response.total, 2);
        assert_eq!(response.messages[0].role, "user");
        assert_eq!(response.messages[0].content, "Hello!");
        assert_eq!(response.messages[1].role, "assistant");
        assert_eq!(response.messages[1].content, "Hi there!");
    }

    #[tokio::test]
    async fn test_get_conversation_messages_not_found() {
        let mock_service = Arc::new(MockConversationService::with_none());
        let use_case = GetConversationMessagesUseCase::new(mock_service);

        let request = GetConversationMessagesRequestDto {
            conversation_id: "non-existent".to_string(),
        };

        let result = use_case.execute(request).await;
        assert!(result.is_err());
        match result {
            Err(AppError::NotFound(_)) => (),
            _ => panic!("Expected NotFound error"),
        }
    }
}
