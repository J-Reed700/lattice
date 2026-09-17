//! Create Conversation Use Case

use crate::features::conversation::dto::{
    CreateConversationRequestDto, CreateConversationResponseDto,
};
use crate::features::conversation::mapper::ConversationDtoMapper;
use crate::features::conversation::ConversationServiceTrait;
use crate::shared::error::Result;
use std::sync::Arc;

/// Use case for creating a new conversation
pub struct CreateConversationUseCase {
    conversation_service: Arc<dyn ConversationServiceTrait>,
}

impl CreateConversationUseCase {
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
    /// * `request` - CreateConversationRequestDto containing title, model_name, and optional system_prompt
    ///
    /// # Returns
    ///
    /// CreateConversationResponseDto with the created conversation
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if title or model_name is empty
    /// - `AppError::Database` if database insert fails
    pub async fn execute(
        &self,
        request: CreateConversationRequestDto,
    ) -> Result<CreateConversationResponseDto> {
        let conversation = self
            .conversation_service
            .create_conversation(request.title, request.model_name, request.system_prompt)
            .await?;

        let conversation_dto = ConversationDtoMapper::to_dto(&conversation);

        Ok(CreateConversationResponseDto {
            conversation: conversation_dto,
            status: "success".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Conversation;
    use crate::features::conversation::ConversationServiceTrait;
    use crate::shared::domain_types::ConversationId;
    use crate::shared::error::AppError;
    use async_trait::async_trait;
    use chrono::Utc;

    struct MockConversationService {
        should_fail: bool,
    }

    impl MockConversationService {
        fn new() -> Self {
            Self { should_fail: false }
        }

        fn with_failure() -> Self {
            Self { should_fail: true }
        }
    }

    #[async_trait]
    impl ConversationServiceTrait for MockConversationService {
        async fn fail_pending_turn(&self, _: &str) -> Result<()> {
            panic!("Turn failure is not part of create-conversation tests")
        }
        async fn complete_turn(
            &self,
            _: &str,
            _: &str,
            _: String,
            _: i64,
            _: Option<String>,
        ) -> Result<crate::domain::conversation::ConversationMessage> {
            panic!("Turn finalization is not part of create-conversation tests")
        }
        async fn create_conversation(
            &self,
            title: String,
            model_name: String,
            system_prompt: Option<String>,
        ) -> Result<Conversation> {
            if self.should_fail {
                return Err(AppError::Database("Database error".to_string()));
            }

            Ok(Conversation {
                id: ConversationId::new(),
                title,
                model_name,
                system_prompt,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                message_count: 0,
                total_tokens: 0,
            })
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
        ) -> Result<Option<crate::domain::ConversationAggregate>> {
            unimplemented!()
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

        async fn compact_conversation(
            &self,
            _conversation_id: &str,
            _summary_text: String,
            _up_to_message_id: &str,
            _summary_tokens: i64,
        ) -> Result<crate::domain::conversation::CompactionRecord> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn test_create_conversation_success() {
        let mock_service = Arc::new(MockConversationService::new());
        let use_case = CreateConversationUseCase::new(mock_service);

        let request = CreateConversationRequestDto {
            title: "My Conversation".to_string(),
            model_name: "claude-sonnet-4-5-20250929".to_string(),
            system_prompt: Some("You are helpful".to_string()),
        };

        let response = use_case.execute(request).await.unwrap();

        assert_eq!(response.status, "success");
        assert_eq!(response.conversation.title, "My Conversation");
        assert_eq!(
            response.conversation.model_name,
            "claude-sonnet-4-5-20250929"
        );
        assert_eq!(response.conversation.message_count, 0);
        assert_eq!(response.conversation.total_tokens, 0);
    }

    #[tokio::test]
    async fn test_create_conversation_failure() {
        let mock_service = Arc::new(MockConversationService::with_failure());
        let use_case = CreateConversationUseCase::new(mock_service);

        let request = CreateConversationRequestDto {
            title: "My Conversation".to_string(),
            model_name: "claude-sonnet-4-5-20250929".to_string(),
            system_prompt: None,
        };

        let result = use_case.execute(request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_conversation_without_system_prompt() {
        let mock_service = Arc::new(MockConversationService::new());
        let use_case = CreateConversationUseCase::new(mock_service);

        let request = CreateConversationRequestDto {
            title: "Simple Chat".to_string(),
            model_name: "claude-sonnet-4-5-20250929".to_string(),
            system_prompt: None,
        };

        let response = use_case.execute(request).await.unwrap();

        assert_eq!(response.status, "success");
        assert_eq!(response.conversation.title, "Simple Chat");
        assert!(response.conversation.system_prompt.is_none());
    }
}
