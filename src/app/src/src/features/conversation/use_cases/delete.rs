//! Delete Conversation Use Case

use crate::features::conversation::dto::{
    DeleteConversationRequestDto, DeleteConversationResponseDto,
};
use crate::features::conversation::ConversationServiceTrait;
use crate::shared::error::Result;
use std::sync::Arc;

/// Use case for deleting a conversation and all its messages
pub struct DeleteConversationUseCase {
    conversation_service: Arc<dyn ConversationServiceTrait>,
}

impl DeleteConversationUseCase {
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
    /// * `request` - DeleteConversationRequestDto with conversation_id
    ///
    /// # Returns
    ///
    /// DeleteConversationResponseDto with status
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::Database` if database deletion fails
    pub async fn execute(
        &self,
        request: DeleteConversationRequestDto,
    ) -> Result<DeleteConversationResponseDto> {
        // Delete conversation via service
        self.conversation_service
            .delete_conversation(&request.conversation_id)
            .await?;

        Ok(DeleteConversationResponseDto {
            status: "success".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Conversation, ConversationAggregate};
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
            unimplemented!()
        }

        async fn rename_conversation(
            &self,
            _conversation_id: &str,
            _new_title: String,
        ) -> Result<()> {
            unimplemented!()
        }

        async fn delete_conversation(&self, conversation_id: &str) -> Result<()> {
            if self.should_fail {
                return Err(AppError::NotFound(format!(
                    "Conversation not found: {}",
                    conversation_id
                )));
            }

            Ok(())
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
    async fn test_delete_conversation_success() {
        let mock_service = Arc::new(MockConversationService::new());
        let use_case = DeleteConversationUseCase::new(mock_service);

        let request = DeleteConversationRequestDto {
            conversation_id: "conv-123".to_string(),
        };

        let response = use_case.execute(request).await.unwrap();

        assert_eq!(response.status, "success");
    }

    #[tokio::test]
    async fn test_delete_conversation_not_found() {
        let mock_service = Arc::new(MockConversationService::with_failure());
        let use_case = DeleteConversationUseCase::new(mock_service);

        let request = DeleteConversationRequestDto {
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
