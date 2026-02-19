//! Conversation Repository Port
//!
//! Port interface for conversation persistence operations.
//!
//! This port defines the contract for storing and retrieving conversation
//! aggregates, including conversation metadata, messages, and document references.

use crate::domain::conversation::{
    Conversation, ConversationAggregate, ConversationMessage, DocumentReference, MessageRole,
};
use crate::shared::error::Result;
use async_trait::async_trait;

/// Port for conversation repository operations.
///
/// Defines the interface for conversation persistence. Infrastructure
/// implementations handle the actual database operations.
///
/// # Thread Safety
///
/// All methods must be thread-safe (`Send + Sync`).
///
/// # Example
///
/// ```rust,no_run
/// use vault_desktop::application::ports::ConversationRepositoryPort;
/// use vault_desktop::domain::conversation::MessageRole;
///
/// async fn example(repo: &impl ConversationRepositoryPort) -> Result<()> {
///     // Create conversation
///     let conversation = repo.create_conversation(
///         "My Chat",
///         "claude-sonnet-4-5-20250929",
///         Some("You are helpful"),
///     ).await?;
///
///     // Add message
///     let message = repo.add_message(
///         conversation.id.as_str(),
///         MessageRole::User,
///         "Hello!",
///         10,
///         None,
///     ).await?;
///
///     // Load full aggregate
///     let aggregate = repo.find_aggregate_by_id(conversation.id.as_str()).await?;
///
///     Ok(())
/// }
/// ```
#[async_trait]
pub trait ConversationRepositoryPort: Send + Sync {
    /// Create a new conversation.
    ///
    /// # Arguments
    ///
    /// * `title` - Human-readable conversation title
    /// * `model_name` - LLM model identifier
    /// * `system_prompt` - Optional system instructions
    ///
    /// # Returns
    ///
    /// The created conversation entity.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if title or model_name is empty
    /// - `AppError::Database` if insert fails
    async fn create_conversation(
        &self,
        title: &str,
        model_name: &str,
        system_prompt: Option<&str>,
    ) -> Result<Conversation>;

    /// Find conversation by ID (metadata only, no messages).
    ///
    /// # Arguments
    ///
    /// * `id` - Conversation ID
    ///
    /// # Returns
    ///
    /// `Some(Conversation)` if found, `None` otherwise.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    async fn find_by_id(&self, id: &str) -> Result<Option<Conversation>>;

    /// Find full conversation aggregate by ID.
    ///
    /// Loads conversation with all messages and document references.
    ///
    /// # Arguments
    ///
    /// * `id` - Conversation ID
    ///
    /// # Returns
    ///
    /// `Some(ConversationAggregate)` if found, `None` otherwise.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    async fn find_aggregate_by_id(&self, id: &str) -> Result<Option<ConversationAggregate>>;

    /// List conversations ordered by updated_at descending.
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of results (default: 100)
    /// * `offset` - Number of results to skip (default: 0)
    ///
    /// # Returns
    ///
    /// List of conversation entities (metadata only).
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    async fn find_all(&self, limit: Option<i64>, offset: Option<i64>) -> Result<Vec<Conversation>>;

    /// Update conversation title.
    ///
    /// # Arguments
    ///
    /// * `id` - Conversation ID
    /// * `new_title` - New title
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::Database` if update fails
    async fn update_title(&self, id: &str, new_title: &str) -> Result<()>;

    /// Update conversation system prompt.
    ///
    /// # Arguments
    ///
    /// * `id` - Conversation ID
    /// * `system_prompt` - New system prompt (None to clear)
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::Database` if update fails
    async fn update_system_prompt(&self, id: &str, system_prompt: Option<&str>) -> Result<()>;

    /// Delete a conversation.
    ///
    /// Cascading delete removes all messages and document references.
    ///
    /// # Arguments
    ///
    /// * `id` - Conversation ID
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::Database` if delete fails
    async fn delete(&self, id: &str) -> Result<()>;

    /// Add a message to the conversation.
    ///
    /// Updates conversation message_count and total_tokens.
    ///
    /// # Arguments
    ///
    /// * `conversation_id` - Conversation ID
    /// * `role` - Message role (user, assistant, system)
    /// * `content` - Message text
    /// * `tokens` - Token count
    /// * `metadata` - Optional JSON metadata
    ///
    /// # Returns
    ///
    /// The created message.
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::Database` if insert fails
    async fn add_message(
        &self,
        conversation_id: &str,
        role: MessageRole,
        content: &str,
        tokens: i64,
        metadata: Option<&str>,
    ) -> Result<ConversationMessage>;

    /// Get all messages for a conversation.
    ///
    /// # Arguments
    ///
    /// * `conversation_id` - Conversation ID
    ///
    /// # Returns
    ///
    /// Messages ordered by created_at ascending.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    async fn get_messages(&self, conversation_id: &str) -> Result<Vec<ConversationMessage>>;

    /// Add a message with a specific status (for two-phase commit).
    ///
    /// Supports the two-phase commit pattern:
    /// 1. Create message with 'pending' status
    /// 2. Attempt operation (e.g., LLM generation)
    /// 3. Update status to 'completed' or 'failed'
    ///
    /// # Arguments
    ///
    /// * `conversation_id` - Conversation ID
    /// * `role` - Message role (user, assistant, system)
    /// * `content` - Message text
    /// * `tokens` - Token count
    /// * `metadata` - Optional JSON metadata
    /// * `status` - Message status ('pending', 'completed', 'failed')
    ///
    /// # Returns
    ///
    /// The created message.
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::Database` if insert fails
    async fn add_message_with_status(
        &self,
        conversation_id: &str,
        role: MessageRole,
        content: &str,
        tokens: i64,
        metadata: Option<&str>,
        status: &str,
    ) -> Result<ConversationMessage>;

    /// Update the status of a message.
    ///
    /// Used in two-phase commit to mark messages as 'completed' or 'failed'
    /// after the operation completes.
    ///
    /// # Arguments
    ///
    /// * `message_id` - ID of the message to update
    /// * `status` - New status ('pending', 'completed', 'failed')
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if message doesn't exist
    /// - `AppError::Database` if update fails
    async fn update_message_status(&self, message_id: &str, status: &str) -> Result<()>;

    /// Add document reference to conversation context.
    ///
    /// # Arguments
    ///
    /// * `conversation_id` - Conversation ID
    /// * `document_id` - Document ID
    /// * `chunk_id` - Optional chunk ID
    /// * `relevance_score` - Optional relevance score
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if insert fails
    async fn add_document_reference(
        &self,
        conversation_id: &str,
        document_id: &str,
        chunk_id: Option<&str>,
        relevance_score: Option<f32>,
    ) -> Result<()>;

    /// Get document references for a conversation.
    ///
    /// # Arguments
    ///
    /// * `conversation_id` - Conversation ID
    ///
    /// # Returns
    ///
    /// Document references ordered by added_at.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    async fn get_document_references(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<DocumentReference>>;

    /// Save a full conversation aggregate.
    ///
    /// This is used to persist changes made to the aggregate.
    /// Updates conversation metadata and message/token counts.
    ///
    /// # Arguments
    ///
    /// * `aggregate` - Conversation aggregate
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::Database` if update fails
    async fn save_aggregate(&self, aggregate: &ConversationAggregate) -> Result<()>;

    /// Count total conversations.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    async fn count(&self) -> Result<usize>;

    /// Check if conversation exists.
    ///
    /// # Arguments
    ///
    /// * `id` - Conversation ID
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    async fn exists(&self, id: &str) -> Result<bool>;
}
