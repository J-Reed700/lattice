//! Get Conversation Use Case

use crate::application::dtos::conversation_dto::{
    GetConversationRequestDto, GetConversationResponseDto,
};
use crate::application::mappers::conversation_mapper::ConversationMapper;
use crate::infrastructure::services::traits::ConversationServiceTrait;
use crate::shared::error::Result;
use std::sync::Arc;

/// Use case for getting a single conversation by ID
pub struct GetConversationUseCase {
    conversation_service: Arc<dyn ConversationServiceTrait>,
}

impl GetConversationUseCase {
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
    /// * `request` - GetConversationRequestDto with conversation_id
    ///
    /// # Returns
    ///
    /// GetConversationResponseDto with optional conversation (None if not found)
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if database query fails
    pub async fn execute(
        &self,
        request: GetConversationRequestDto,
    ) -> Result<GetConversationResponseDto> {
        // Get conversation aggregate from service
        let aggregate = self
            .conversation_service
            .get_conversation(&request.conversation_id)
            .await?;

        // Convert to DTO if found
        let conversation_dto = aggregate.map(|agg| ConversationMapper::aggregate_to_dto(&agg));

        Ok(GetConversationResponseDto {
            conversation: conversation_dto,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Conversation, ConversationAggregate};
    use crate::infrastructure::services::traits::ConversationServiceTrait;
    use crate::shared::domain_types::ConversationId;
    use crate::shared::error::AppError;
    use async_trait::async_trait;
    use chrono::Utc;

    struct MockConversationService {
        conversation: Option<ConversationAggregate>,
    }

    impl MockConversationService {
        fn new() -> Self {
            let aggregate = ConversationAggregate::new(
                "Test Conversation".to_string(),
                "claude-sonnet-4-5-20250929".to_string(),
                Some("System prompt".to_string()),
            )
            .unwrap();

            Self {
                conversation: Some(aggregate),
            }
        }

        fn with_none() -> Self {
            Self { conversation: None }
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
            Ok(self.conversation.clone())
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
    async fn test_get_conversation_success() {
        let mock_service = Arc::new(MockConversationService::new());
        let use_case = GetConversationUseCase::new(mock_service);

        let request = GetConversationRequestDto {
            conversation_id: "conv-123".to_string(),
        };

        let response = use_case.execute(request).await.unwrap();

        assert!(response.conversation.is_some());
        let conv = response.conversation.unwrap();
        assert_eq!(conv.title, "Test Conversation");
        assert_eq!(conv.model_name, "claude-sonnet-4-5-20250929");
    }

    #[tokio::test]
    async fn test_get_conversation_not_found() {
        let mock_service = Arc::new(MockConversationService::with_none());
        let use_case = GetConversationUseCase::new(mock_service);

        let request = GetConversationRequestDto {
            conversation_id: "non-existent".to_string(),
        };

        let response = use_case.execute(request).await.unwrap();

        assert!(response.conversation.is_none());
    }
}
