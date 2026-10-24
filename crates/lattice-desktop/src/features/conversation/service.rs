//! Conversation service for managing LLM conversations.

use lattice_core::application::ports::conversation_repository::ConversationRepositoryPort;
use lattice_core::domain::conversation::{
    Conversation, ConversationAggregate, ConversationMessage, MessageRole,
};
use lattice_core::shared::error::{AppError, Result};
use std::sync::Arc;

pub struct ConversationService {
    repository: Arc<dyn ConversationRepositoryPort>,
}

impl ConversationService {
    pub fn new(repository: Arc<dyn ConversationRepositoryPort>) -> Self {
        Self { repository }
    }

    pub async fn create_conversation(
        &self,
        title: String,
        system_prompt: Option<String>,
    ) -> Result<Conversation> {
        let _aggregate = ConversationAggregate::new(title.clone(), system_prompt.clone())?;

        self.repository
            .create(&title, system_prompt.as_deref())
            .await
    }

    pub async fn get_conversation(&self, id: &str) -> Result<Option<ConversationAggregate>> {
        self.repository.load_aggregate(id).await
    }

    pub async fn list_conversations(
        &self,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Conversation>> {
        self.repository.list(limit, offset).await
    }

    pub async fn delete_conversation(&self, id: &str) -> Result<()> {
        self.repository.delete(id).await
    }

    pub async fn rename_conversation(&self, id: &str, new_title: String) -> Result<()> {
        if new_title.trim().is_empty() {
            return Err(AppError::InvalidInput("Title cannot be empty".into()));
        }

        self.repository.update_title(id, &new_title).await
    }

    pub async fn update_system_prompt(
        &self,
        id: &str,
        system_prompt: Option<String>,
    ) -> Result<()> {
        self.repository
            .update_system_prompt(id, system_prompt.as_deref())
            .await
    }

    pub async fn add_user_message(
        &self,
        conversation_id: &str,
        content: String,
        tokens: i64,
    ) -> Result<ConversationMessage> {
        if content.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Message content cannot be empty".into(),
            ));
        }

        self.repository
            .add_message(conversation_id, MessageRole::User, &content, tokens, None)
            .await
    }

    pub async fn add_assistant_message(
        &self,
        conversation_id: &str,
        content: String,
        tokens: i64,
    ) -> Result<ConversationMessage> {
        if content.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Message content cannot be empty".into(),
            ));
        }

        self.repository
            .add_message(
                conversation_id,
                MessageRole::Assistant,
                &content,
                tokens,
                None,
            )
            .await
    }

    pub async fn add_assistant_message_with_metadata(
        &self,
        conversation_id: &str,
        content: String,
        tokens: i64,
        metadata: Option<String>,
    ) -> Result<ConversationMessage> {
        if content.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Message content cannot be empty".into(),
            ));
        }

        self.repository
            .add_message(
                conversation_id,
                MessageRole::Assistant,
                &content,
                tokens,
                metadata.as_deref(),
            )
            .await
    }

    pub async fn prune_conversation_to_limit(
        &self,
        conversation_id: &str,
        max_tokens: i64,
    ) -> Result<()> {
        if max_tokens <= 0 {
            return Err(AppError::InvalidInput(
                "Token limit must be positive".into(),
            ));
        }

        let mut aggregate = self
            .repository
            .load_aggregate(conversation_id)
            .await?
            .ok_or_else(|| {
                AppError::NotFound(format!("Conversation not found: {}", conversation_id))
            })?;

        let before_ids: std::collections::HashSet<String> =
            aggregate.messages().iter().map(|m| m.id.clone()).collect();

        aggregate.prune_to_token_limit(max_tokens)?;

        let after_ids: std::collections::HashSet<String> =
            aggregate.messages().iter().map(|m| m.id.clone()).collect();

        let messages_to_delete: Vec<String> = before_ids.difference(&after_ids).cloned().collect();

        if !messages_to_delete.is_empty() {
            self.repository
                .delete_messages(conversation_id, &messages_to_delete)
                .await?;
        }

        Ok(())
    }

    pub async fn add_document_reference(
        &self,
        conversation_id: &str,
        document_id: String,
        chunk_id: Option<String>,
        relevance_score: Option<f32>,
    ) -> Result<()> {
        self.repository
            .add_document_reference(
                conversation_id,
                &document_id,
                chunk_id.as_deref(),
                relevance_score,
            )
            .await
    }

    pub async fn add_message_with_status(
        &self,
        conversation_id: &str,
        role: MessageRole,
        content: String,
        tokens: i64,
        status: String,
    ) -> Result<ConversationMessage> {
        if content.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Message content cannot be empty".into(),
            ));
        }

        self.repository
            .add_message_with_status(conversation_id, role, &content, tokens, None, &status)
            .await
    }

    pub async fn update_message_status(&self, message_id: &str, status: String) -> Result<()> {
        self.repository
            .update_message_status(message_id, &status)
            .await
    }
}

#[async_trait::async_trait]
impl crate::features::conversation::ConversationServiceTrait for ConversationService {
    async fn fail_pending_turn(&self, user_message_id: &str) -> Result<()> {
        self.repository.fail_pending_turn(user_message_id).await
    }

    async fn complete_turn(
        &self,
        conversation_id: &str,
        user_message_id: &str,
        content: String,
        tokens: i64,
        metadata: Option<String>,
    ) -> Result<ConversationMessage> {
        if content.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Message content cannot be empty".into(),
            ));
        }
        self.repository
            .complete_turn(conversation_id, user_message_id, content, tokens, metadata)
            .await
    }

    async fn create_conversation(
        &self,
        title: String,
        system_prompt: Option<String>,
    ) -> Result<Conversation> {
        self.create_conversation(title, system_prompt).await
    }

    async fn get_conversation(&self, id: &str) -> Result<Option<ConversationAggregate>> {
        self.get_conversation(id).await
    }

    async fn list_conversations(
        &self,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Conversation>> {
        self.list_conversations(limit, offset).await
    }

    async fn delete_conversation(&self, id: &str) -> Result<()> {
        self.delete_conversation(id).await
    }

    async fn rename_conversation(&self, id: &str, new_title: String) -> Result<()> {
        self.rename_conversation(id, new_title).await
    }

    async fn update_system_prompt(&self, id: &str, system_prompt: Option<String>) -> Result<()> {
        self.update_system_prompt(id, system_prompt).await
    }

    async fn add_user_message(
        &self,
        conversation_id: &str,
        content: String,
        tokens: i64,
    ) -> Result<ConversationMessage> {
        self.add_user_message(conversation_id, content, tokens)
            .await
    }

    async fn add_assistant_message(
        &self,
        conversation_id: &str,
        content: String,
        tokens: i64,
    ) -> Result<ConversationMessage> {
        self.add_assistant_message(conversation_id, content, tokens)
            .await
    }

    async fn add_assistant_message_with_metadata(
        &self,
        conversation_id: &str,
        content: String,
        tokens: i64,
        metadata: Option<String>,
    ) -> Result<ConversationMessage> {
        self.add_assistant_message_with_metadata(conversation_id, content, tokens, metadata)
            .await
    }

    async fn prune_conversation_to_limit(
        &self,
        conversation_id: &str,
        max_tokens: i64,
    ) -> Result<()> {
        self.prune_conversation_to_limit(conversation_id, max_tokens)
            .await
    }

    async fn add_document_reference(
        &self,
        conversation_id: &str,
        document_id: String,
        chunk_id: Option<String>,
        relevance_score: Option<f32>,
    ) -> Result<()> {
        self.add_document_reference(conversation_id, document_id, chunk_id, relevance_score)
            .await
    }

    async fn add_message_with_status(
        &self,
        conversation_id: &str,
        role: MessageRole,
        content: String,
        tokens: i64,
        status: String,
    ) -> Result<ConversationMessage> {
        self.add_message_with_status(conversation_id, role, content, tokens, status)
            .await
    }

    async fn add_message_with_metadata(
        &self,
        conversation_id: &str,
        role: MessageRole,
        content: String,
        tokens: i64,
        status: String,
        metadata: Option<String>,
    ) -> Result<ConversationMessage> {
        if content.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Message content cannot be empty".into(),
            ));
        }

        self.repository
            .add_message_with_status(
                conversation_id,
                role,
                &content,
                tokens,
                metadata.as_deref(),
                &status,
            )
            .await
    }

    async fn update_message_status(&self, message_id: &str, status: String) -> Result<()> {
        self.update_message_status(message_id, status).await
    }

    async fn set_message_metadata_fields(
        &self,
        message_id: &str,
        fields: Vec<(String, serde_json::Value)>,
    ) -> Result<()> {
        self.repository
            .set_message_metadata_fields(message_id, fields)
            .await
    }
}

#[async_trait::async_trait]
impl lattice_core::application::ports::ConversationHistoryPort for ConversationService {
    async fn get_conversation(&self, id: &str) -> Result<Option<ConversationAggregate>> {
        self.get_conversation(id).await
    }
}

#[cfg(test)]
mod tests;
