//! Service trait definitions.

use async_trait::async_trait;
use lattice_core::shared::error::Result;

#[async_trait]
pub trait ConversationServiceTrait: Send + Sync {
    async fn fail_pending_turn(&self, user_message_id: &str) -> Result<()>;

    /// Persist both sides of a successfully completed turn as one operation.
    async fn complete_turn(
        &self,
        conversation_id: &str,
        user_message_id: &str,
        content: String,
        tokens: i64,
        metadata: Option<String>,
    ) -> Result<lattice_core::domain::conversation::ConversationMessage>;

    async fn create_conversation(
        &self,
        title: String,
        system_prompt: Option<String>,
    ) -> Result<lattice_core::domain::conversation::Conversation>;

    async fn get_conversation(
        &self,
        id: &str,
    ) -> Result<Option<lattice_core::domain::conversation::ConversationAggregate>>;

    async fn list_conversations(
        &self,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<lattice_core::domain::conversation::Conversation>>;

    async fn delete_conversation(&self, id: &str) -> Result<()>;

    async fn rename_conversation(&self, id: &str, new_title: String) -> Result<()>;

    async fn update_system_prompt(&self, id: &str, system_prompt: Option<String>) -> Result<()>;

    async fn add_user_message(
        &self,
        conversation_id: &str,
        content: String,
        tokens: i64,
    ) -> Result<lattice_core::domain::conversation::ConversationMessage>;

    async fn add_assistant_message(
        &self,
        conversation_id: &str,
        content: String,
        tokens: i64,
    ) -> Result<lattice_core::domain::conversation::ConversationMessage>;

    async fn add_assistant_message_with_metadata(
        &self,
        conversation_id: &str,
        content: String,
        tokens: i64,
        metadata: Option<String>,
    ) -> Result<lattice_core::domain::conversation::ConversationMessage>;

    async fn prune_conversation_to_limit(
        &self,
        conversation_id: &str,
        max_tokens: i64,
    ) -> Result<()>;

    async fn add_document_reference(
        &self,
        conversation_id: &str,
        document_id: String,
        chunk_id: Option<String>,
        relevance_score: Option<f32>,
    ) -> Result<()>;

    async fn add_message_with_status(
        &self,
        conversation_id: &str,
        role: lattice_core::domain::conversation::MessageRole,
        content: String,
        tokens: i64,
        status: String,
    ) -> Result<lattice_core::domain::conversation::ConversationMessage>;

    /// Add a message with status and metadata; default delegates to add_message_with_status.
    async fn add_message_with_metadata(
        &self,
        conversation_id: &str,
        role: lattice_core::domain::conversation::MessageRole,
        content: String,
        tokens: i64,
        status: String,
        metadata: Option<String>,
    ) -> Result<lattice_core::domain::conversation::ConversationMessage> {
        let _ = metadata;
        self.add_message_with_status(conversation_id, role, content, tokens, status)
            .await
    }

    async fn update_message_status(&self, message_id: &str, status: String) -> Result<()>;

    /// Set top-level keys of a persisted message's metadata, keeping the rest.
    async fn set_message_metadata_fields(
        &self,
        message_id: &str,
        fields: Vec<(String, serde_json::Value)>,
    ) -> Result<()>;
}
