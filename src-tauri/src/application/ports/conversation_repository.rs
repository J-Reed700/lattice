//! Persistence operations required by conversation orchestration.

use crate::domain::conversation::{
    CompactionRecord, Conversation, ConversationAggregate, ConversationMessage, MessageRole,
};
use crate::shared::error::Result;

/// Storage boundary for conversation orchestration; adapters own persistence details.
/// This interface does not imply a transaction spanning multiple calls.
#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
pub trait ConversationRepositoryPort: Send + Sync {
    /// Mark only a still-pending user turn failed; never undo a committed turn.
    async fn fail_pending_turn(&self, user_message_id: &str) -> Result<()>;
    /// Complete a pending user message and insert its assistant response atomically.
    async fn complete_turn(
        &self,
        conversation_id: &str,
        user_message_id: &str,
        content: String,
        tokens: i64,
        metadata: Option<String>,
    ) -> Result<ConversationMessage>;
    async fn create<'a>(
        &self,
        title: &str,
        model_name: &str,
        system_prompt: Option<&'a str>,
    ) -> Result<Conversation>;
    async fn load_aggregate(&self, id: &str) -> Result<Option<ConversationAggregate>>;
    async fn list(&self, limit: Option<i64>, offset: Option<i64>) -> Result<Vec<Conversation>>;
    async fn delete(&self, id: &str) -> Result<()>;
    async fn update_title(&self, id: &str, title: &str) -> Result<()>;
    async fn update_system_prompt<'a>(
        &self,
        id: &str,
        system_prompt: Option<&'a str>,
    ) -> Result<()>;
    async fn add_message<'a>(
        &self,
        conversation_id: &str,
        role: MessageRole,
        content: &str,
        tokens: i64,
        metadata: Option<&'a str>,
    ) -> Result<ConversationMessage>;
    /// Atomically delete the selected messages and their bookmarks, and recount
    /// conversation totals. IDs belonging to other conversations are ignored.
    /// An empty selection is a no-op. Failure must leave all three unchanged.
    async fn delete_messages(&self, conversation_id: &str, message_ids: &[String]) -> Result<u64>;
    async fn add_document_reference<'a>(
        &self,
        conversation_id: &str,
        document_id: &str,
        chunk_id: Option<&'a str>,
        relevance_score: Option<f32>,
    ) -> Result<()>;
    async fn add_message_with_status<'a>(
        &self,
        conversation_id: &str,
        role: MessageRole,
        content: &str,
        tokens: i64,
        metadata: Option<&'a str>,
        status: &str,
    ) -> Result<ConversationMessage>;
    async fn update_message_status(&self, message_id: &str, status: &str) -> Result<()>;
    /// Persist (upsert) the active compaction summary for a conversation.
    async fn save_compaction(&self, record: &CompactionRecord) -> Result<()>;
}
