//! Service trait definitions
//!
//! This module defines trait interfaces for dependency injection.

use crate::shared::error::Result;
use async_trait::async_trait;

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
    ) -> Result<crate::domain::conversation::ConversationMessage>;
    /// Create a new conversation
    ///
    /// # Arguments
    /// * `title` - Human-readable conversation title
    /// * `model_name` - LLM model identifier (e.g., "claude-sonnet-4-5-20250929")
    /// * `system_prompt` - Optional system instructions for the LLM
    ///
    /// # Returns
    /// The created conversation entity
    ///
    /// # Errors
    /// - `AppError::InvalidInput` if title or model_name is empty
    /// - `AppError::Database` if database insert fails
    ///
    /// # Example
    /// ```rust
    /// let conversation = service.create_conversation(
    ///     "My Chat".to_string(),
    ///     "claude-sonnet-4-5-20250929".to_string(),
    ///     Some("You are a helpful assistant.".to_string())
    /// ).await?;
    /// ```
    async fn create_conversation(
        &self,
        title: String,
        model_name: String,
        system_prompt: Option<String>,
    ) -> Result<crate::domain::conversation::Conversation>;

    /// Get a conversation by ID with all messages and context
    ///
    /// # Arguments
    /// * `id` - Conversation ID
    ///
    /// # Returns
    /// Full conversation aggregate with messages and document references,
    /// or `None` if not found
    ///
    /// # Errors
    /// - `AppError::Database` if database query fails
    async fn get_conversation(
        &self,
        id: &str,
    ) -> Result<Option<crate::domain::conversation::ConversationAggregate>>;

    /// List conversations ordered by most recently updated
    ///
    /// # Arguments
    /// * `limit` - Maximum number of conversations to return
    /// * `offset` - Number of conversations to skip (for pagination)
    ///
    /// # Returns
    /// Vector of conversation entities (without messages)
    ///
    /// # Errors
    /// - `AppError::Database` if database query fails
    ///
    /// # Example
    /// ```rust
    /// // Get first page of 20 conversations
    /// let conversations = service.list_conversations(20, 0).await?;
    /// ```
    async fn list_conversations(
        &self,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<crate::domain::conversation::Conversation>>;

    /// Delete a conversation and all its messages
    ///
    /// # Arguments
    /// * `id` - Conversation ID
    ///
    /// # Errors
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::Database` if deletion fails
    ///
    /// # Side Effects
    /// - Deletes all conversation messages (cascading delete)
    /// - Deletes all document references
    async fn delete_conversation(&self, id: &str) -> Result<()>;

    /// Rename a conversation
    ///
    /// # Arguments
    /// * `id` - Conversation ID
    /// * `new_title` - New title for the conversation
    ///
    /// # Errors
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::InvalidInput` if new_title is empty
    /// - `AppError::Database` if update fails
    async fn rename_conversation(&self, id: &str, new_title: String) -> Result<()>;

    /// Update the system prompt of a conversation
    ///
    /// # Arguments
    /// * `id` - Conversation ID
    /// * `system_prompt` - New system prompt (None to remove)
    ///
    /// # Errors
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::Database` if update fails
    async fn update_system_prompt(&self, id: &str, system_prompt: Option<String>) -> Result<()>;

    /// Add a user message to the conversation
    ///
    /// # Arguments
    /// * `conversation_id` - Conversation ID
    /// * `content` - Message content
    /// * `tokens` - Token count for this message
    ///
    /// # Returns
    /// The created message
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
    ) -> Result<crate::domain::conversation::ConversationMessage>;

    /// Add an assistant message to the conversation
    ///
    /// # Arguments
    /// * `conversation_id` - Conversation ID
    /// * `content` - Message content
    /// * `tokens` - Token count for this message
    ///
    /// # Returns
    /// The created message
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
    ) -> Result<crate::domain::conversation::ConversationMessage>;

    /// Add an assistant message with metadata to the conversation
    ///
    /// # Arguments
    /// * `conversation_id` - Conversation ID
    /// * `content` - Message content
    /// * `tokens` - Token count for this message
    /// * `metadata` - Optional JSON metadata string (e.g., serialized sources)
    ///
    /// # Returns
    /// The created message with metadata
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
    ) -> Result<crate::domain::conversation::ConversationMessage>;

    /// Prune conversation history to fit within token limit
    ///
    /// Removes oldest messages (keeping most recent) until total tokens <= max_tokens.
    /// Always preserves at least the last message.
    ///
    /// # Arguments
    /// * `conversation_id` - Conversation ID
    /// * `max_tokens` - Maximum token limit
    ///
    /// # Errors
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::InvalidInput` if max_tokens is <= 0
    /// - `AppError::Database` if deletion or update fails
    ///
    /// # Example
    /// ```rust
    /// // Keep only recent 10K tokens of history
    /// service.prune_conversation_to_limit("conv-123", 10000).await?;
    /// ```
    async fn prune_conversation_to_limit(
        &self,
        conversation_id: &str,
        max_tokens: i64,
    ) -> Result<()>;

    /// Add a document reference to conversation context
    ///
    /// Tracks which documents/chunks are relevant to this conversation.
    /// Used for RAG context tracking.
    ///
    /// # Arguments
    /// * `conversation_id` - Conversation ID
    /// * `document_id` - Document ID
    /// * `chunk_id` - Optional specific chunk ID
    /// * `relevance_score` - Optional relevance score (0.0 to 1.0)
    ///
    /// # Errors
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::Database` if insert fails
    ///
    /// # Note
    /// Duplicate references are ignored (idempotent)
    async fn add_document_reference(
        &self,
        conversation_id: &str,
        document_id: String,
        chunk_id: Option<String>,
        relevance_score: Option<f32>,
    ) -> Result<()>;

    /// Add a message with a specific status (for two-phase commit)
    ///
    /// This method supports the two-phase commit pattern:
    /// 1. Save message with 'pending' status
    /// 2. Attempt operation (e.g., LLM generation)
    /// 3. Update to 'completed' or 'failed' based on outcome
    ///
    /// # Arguments
    /// * `conversation_id` - Conversation ID
    /// * `role` - Message role (User, Assistant, System)
    /// * `content` - Message content
    /// * `tokens` - Token count
    /// * `status` - Message status ('pending', 'completed', 'failed')
    ///
    /// # Returns
    /// The created message with specified status
    ///
    /// # Errors
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::InvalidInput` if content is empty
    /// - `AppError::Database` if insert fails
    async fn add_message_with_status(
        &self,
        conversation_id: &str,
        role: crate::domain::conversation::MessageRole,
        content: String,
        tokens: i64,
        status: String,
    ) -> Result<crate::domain::conversation::ConversationMessage>;

    /// Update the status of a message (for two-phase commit)
    ///
    /// # Arguments
    /// * `message_id` - ID of the message to update
    /// * `status` - New status ('pending', 'completed', 'failed')
    ///
    /// # Errors
    /// - `AppError::NotFound` if message doesn't exist
    /// - `AppError::Database` if update fails
    async fn update_message_status(&self, message_id: &str, status: String) -> Result<()>;

    /// Compact the conversation's oldest messages into an LLM summary.
    ///
    /// # Arguments
    /// * `conversation_id` - Conversation ID
    /// * `summary_text` - LLM-produced summary of the compacted messages
    /// * `up_to_message_id` - id of the last message folded into the summary
    /// * `summary_tokens` - token count of the summary
    ///
    /// # Returns
    /// The created compaction record
    ///
    /// # Errors
    /// - `AppError::NotFound` if the conversation or boundary message is missing
    /// - `AppError::InvalidInput` if the summary is empty or carries no tokens
    async fn compact_conversation(
        &self,
        conversation_id: &str,
        summary_text: String,
        up_to_message_id: &str,
        summary_tokens: i64,
    ) -> Result<crate::domain::conversation::CompactionRecord>;
}
