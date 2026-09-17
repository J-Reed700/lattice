//! `ConversationRepositoryPort` adapter over the inherent methods.

use super::ConversationRepository;
use crate::domain::conversation::{
    CompactionRecord, Conversation, ConversationAggregate, ConversationMessage, MessageRole,
};
use crate::shared::error::Result;

#[async_trait::async_trait]
impl crate::application::ports::conversation_repository::ConversationRepositoryPort
    for ConversationRepository
{
    async fn fail_pending_turn(&self, user_message_id: &str) -> Result<()> {
        sqlx::query("UPDATE conversation_messages SET status='failed' WHERE id=? AND role='user' AND status='pending'")
            .bind(user_message_id).execute(&self.pool).await?;
        Ok(())
    }
    async fn complete_turn(
        &self,
        conversation_id: &str,
        user_message_id: &str,
        content: String,
        tokens: i64,
        metadata: Option<String>,
    ) -> Result<ConversationMessage> {
        ConversationRepository::complete_turn(
            self,
            conversation_id,
            user_message_id,
            content,
            tokens,
            metadata,
        )
        .await
    }
    async fn create<'a>(
        &self,
        title: &str,
        model_name: &str,
        system_prompt: Option<&'a str>,
    ) -> Result<Conversation> {
        ConversationRepository::create(self, title, model_name, system_prompt).await
    }
    async fn load_aggregate(&self, id: &str) -> Result<Option<ConversationAggregate>> {
        ConversationRepository::load_aggregate(self, id).await
    }
    async fn list(&self, limit: Option<i64>, offset: Option<i64>) -> Result<Vec<Conversation>> {
        ConversationRepository::list(self, limit, offset).await
    }
    async fn delete(&self, id: &str) -> Result<()> {
        ConversationRepository::delete(self, id).await
    }
    async fn update_title(&self, id: &str, title: &str) -> Result<()> {
        ConversationRepository::update_title(self, id, title).await
    }
    async fn update_system_prompt<'a>(
        &self,
        id: &str,
        system_prompt: Option<&'a str>,
    ) -> Result<()> {
        ConversationRepository::update_system_prompt(self, id, system_prompt).await
    }
    async fn add_message<'a>(
        &self,
        conversation_id: &str,
        role: MessageRole,
        content: &str,
        tokens: i64,
        metadata: Option<&'a str>,
    ) -> Result<ConversationMessage> {
        ConversationRepository::add_message(self, conversation_id, role, content, tokens, metadata)
            .await
    }
    async fn delete_messages(&self, conversation_id: &str, message_ids: &[String]) -> Result<u64> {
        ConversationRepository::delete_messages(self, conversation_id, message_ids).await
    }
    async fn add_document_reference<'a>(
        &self,
        conversation_id: &str,
        document_id: &str,
        chunk_id: Option<&'a str>,
        relevance_score: Option<f32>,
    ) -> Result<()> {
        ConversationRepository::add_document_reference(
            self,
            conversation_id,
            document_id,
            chunk_id,
            relevance_score,
        )
        .await
    }
    async fn add_message_with_status<'a>(
        &self,
        conversation_id: &str,
        role: MessageRole,
        content: &str,
        tokens: i64,
        metadata: Option<&'a str>,
        status: &str,
    ) -> Result<ConversationMessage> {
        ConversationRepository::add_message_with_status(
            self,
            conversation_id,
            role,
            content,
            tokens,
            metadata,
            status,
        )
        .await
    }
    async fn update_message_status(&self, message_id: &str, status: &str) -> Result<()> {
        ConversationRepository::update_message_status(self, message_id, status).await
    }
    async fn save_compaction(&self, record: &CompactionRecord) -> Result<()> {
        ConversationRepository::upsert_summary(self, record).await
    }
}
