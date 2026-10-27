//! Service trait definitions for dependency injection.

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
    /// Creates a conversation; it follows the Settings default model.
    ///
    /// # Errors
    /// - `AppError::InvalidInput` if the title is empty
    /// - `AppError::Database` if database insert fails
    async fn create_conversation(
        &self,
        title: String,
        system_prompt: Option<String>,
    ) -> Result<lattice_core::domain::conversation::Conversation>;

    /// A conversation with all its messages and document references, or `None`.
    ///
    /// # Errors
    /// - `AppError::Database` if database query fails
    async fn get_conversation(
        &self,
        id: &str,
    ) -> Result<Option<lattice_core::domain::conversation::ConversationAggregate>>;

    /// Conversations, most recently updated first, without messages.
    ///
    /// # Errors
    /// - `AppError::Database` if database query fails
    async fn list_conversations(
        &self,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<lattice_core::domain::conversation::Conversation>>;

    /// Deletes a conversation; its messages and document references cascade.
    ///
    /// # Errors
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::Database` if deletion fails
    async fn delete_conversation(&self, id: &str) -> Result<()>;

    /// Renames a conversation.
    ///
    /// # Errors
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::InvalidInput` if new_title is empty
    /// - `AppError::Database` if update fails
    async fn rename_conversation(&self, id: &str, new_title: String) -> Result<()>;

    /// Sets a conversation's system prompt, or removes it with `None`.
    ///
    /// # Errors
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::Database` if update fails
    async fn update_system_prompt(&self, id: &str, system_prompt: Option<String>) -> Result<()>;

    /// Adds a user message.
    ///
    /// # Errors
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::InvalidInput` if content is empty
    /// - `AppError::Database` if insert fails
    async fn add_user_message(
        &self,
        conversation_id: &str,
        content: String,
        tokens: i64,
    ) -> Result<lattice_core::domain::conversation::ConversationMessage>;

    /// Adds an assistant message.
    ///
    /// # Errors
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::InvalidInput` if content is empty
    /// - `AppError::Database` if insert fails
    async fn add_assistant_message(
        &self,
        conversation_id: &str,
        content: String,
        tokens: i64,
    ) -> Result<lattice_core::domain::conversation::ConversationMessage>;

    /// Adds an assistant message with optional JSON metadata (e.g. serialized
    /// sources).
    ///
    /// # Errors
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::InvalidInput` if content is empty
    /// - `AppError::Database` if insert fails
    async fn add_assistant_message_with_metadata(
        &self,
        conversation_id: &str,
        content: String,
        tokens: i64,
        metadata: Option<String>,
    ) -> Result<lattice_core::domain::conversation::ConversationMessage>;

    /// Removes the oldest messages until the total is within `max_tokens`, always
    /// keeping the last message.
    ///
    /// # Errors
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::InvalidInput` if max_tokens is <= 0
    /// - `AppError::Database` if deletion or update fails
    async fn prune_conversation_to_limit(
        &self,
        conversation_id: &str,
        max_tokens: i64,
    ) -> Result<()>;

    /// Adds a document reference to the conversation's context; duplicates are
    /// ignored.
    ///
    /// # Errors
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::Database` if insert fails
    async fn add_document_reference(
        &self,
        conversation_id: &str,
        document_id: String,
        chunk_id: Option<String>,
        relevance_score: Option<f32>,
    ) -> Result<()>;

    /// Adds a message with a status (`pending`, `completed` or `failed`), for
    /// two-phase commit.
    ///
    /// # Errors
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::InvalidInput` if content is empty
    /// - `AppError::Database` if insert fails
    async fn add_message_with_status(
        &self,
        conversation_id: &str,
        role: lattice_core::domain::conversation::MessageRole,
        content: String,
        tokens: i64,
        status: String,
    ) -> Result<lattice_core::domain::conversation::ConversationMessage>;

    /// Add a message with a specific status and a metadata payload.
    ///
    /// The default ignores `metadata` and delegates to
    /// [`Self::add_message_with_status`], so test doubles that never read
    /// metadata need no change. The real service overrides it.
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

    /// Updates a message's status, for two-phase commit.
    ///
    /// # Errors
    /// - `AppError::NotFound` if message doesn't exist
    /// - `AppError::Database` if update fails
    async fn update_message_status(&self, message_id: &str, status: String) -> Result<()>;

    /// Set top-level keys of a persisted message's metadata, keeping the rest.
    async fn set_message_metadata_fields(
        &self,
        message_id: &str,
        fields: Vec<(String, serde_json::Value)>,
    ) -> Result<()>;
}
