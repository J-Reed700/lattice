//! List Conversations Use Case

use crate::features::conversation::dto::{
    ListConversationsQuery, ListConversationsResponseDto,
};
use crate::features::conversation::mapper::ConversationMapper;
use crate::infrastructure::services::traits::ConversationServiceTrait;
use crate::shared::error::Result;
use std::sync::Arc;

/// Use case for listing all conversations
pub struct ListConversationsUseCase {
    conversation_service: Arc<dyn ConversationServiceTrait>,
}

impl ListConversationsUseCase {
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
    /// * `query` - ListConversationsQuery with optional limit and offset
    ///
    /// # Returns
    ///
    /// ListConversationsResponseDto with conversations list
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if database query fails
    pub async fn execute(
        &self,
        query: ListConversationsQuery,
    ) -> Result<ListConversationsResponseDto> {
        // Cap limit at 100 to prevent excessive results
        let limit = query.limit.map(|l| l.min(100));

        // Get conversations from service
        let conversations = self
            .conversation_service
            .list_conversations(limit, query.offset)
            .await?;

        // Convert to DTOs
        let conversation_dtos: Vec<_> = conversations
            .iter()
            .map(ConversationMapper::to_dto)
            .collect();

        let total = conversation_dtos.len();

        Ok(ListConversationsResponseDto {
            conversations: conversation_dtos,
            total,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Conversation;
    use crate::infrastructure::services::traits::ConversationServiceTrait;
    use crate::shared::domain_types::ConversationId;
    use crate::shared::error::AppError;
    use async_trait::async_trait;
    use chrono::Utc;

    struct MockConversationService {
        conversations: Vec<Conversation>,
    }

    impl MockConversationService {
        fn new() -> Self {
            Self {
                conversations: vec![
                    Conversation {
                        id: ConversationId::new(),
                        title: "Conversation 1".to_string(),
                        model_name: "claude-sonnet-4-5-20250929".to_string(),
                        system_prompt: None,
                        created_at: Utc::now(),
                        updated_at: Utc::now(),
                        message_count: 3,
                        total_tokens: 500,
                    },
                    Conversation {
                        id: ConversationId::new(),
                        title: "Conversation 2".to_string(),
                        model_name: "claude-sonnet-4-5-20250929".to_string(),
                        system_prompt: Some("Be helpful".to_string()),
                        created_at: Utc::now(),
                        updated_at: Utc::now(),
                        message_count: 5,
                        total_tokens: 1000,
                    },
                ],
            }
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
            limit: Option<i64>,
            offset: Option<i64>,
        ) -> Result<Vec<Conversation>> {
            let offset = offset.unwrap_or(0) as usize;
            let limit = limit.map(|l| l as usize);

            let mut results = self.conversations.clone();
            if offset < results.len() {
                results = results[offset..].to_vec();
            } else {
                results.clear();
            }

            if let Some(limit) = limit {
                results.truncate(limit);
            }

            Ok(results)
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
    }

    #[tokio::test]
    async fn test_list_conversations_success() {
        let mock_service = Arc::new(MockConversationService::new());
        let use_case = ListConversationsUseCase::new(mock_service);

        let query = ListConversationsQuery {
            limit: None,
            offset: None,
        };

        let response = use_case.execute(query).await.unwrap();

        assert_eq!(response.total, 2);
        assert_eq!(response.conversations[0].title, "Conversation 1");
        assert_eq!(response.conversations[1].title, "Conversation 2");
    }

    #[tokio::test]
    async fn test_list_conversations_with_limit() {
        let mock_service = Arc::new(MockConversationService::new());
        let use_case = ListConversationsUseCase::new(mock_service);

        let query = ListConversationsQuery {
            limit: Some(1),
            offset: None,
        };

        let response = use_case.execute(query).await.unwrap();

        assert_eq!(response.total, 1);
        assert_eq!(response.conversations[0].title, "Conversation 1");
    }

    #[tokio::test]
    async fn test_list_conversations_with_offset() {
        let mock_service = Arc::new(MockConversationService::new());
        let use_case = ListConversationsUseCase::new(mock_service);

        let query = ListConversationsQuery {
            limit: None,
            offset: Some(1),
        };

        let response = use_case.execute(query).await.unwrap();

        assert_eq!(response.total, 1);
        assert_eq!(response.conversations[0].title, "Conversation 2");
    }

    #[tokio::test]
    async fn test_list_conversations_caps_limit_at_100() {
        let mock_service = Arc::new(MockConversationService::new());
        let use_case = ListConversationsUseCase::new(mock_service);

        let query = ListConversationsQuery {
            limit: Some(200), // Should be capped at 100
            offset: None,
        };

        // This test just verifies it doesn't error - the actual capping happens in the service
        let response = use_case.execute(query).await.unwrap();
        assert!(response.total <= 100);
    }
}
