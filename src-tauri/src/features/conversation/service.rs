//! Conversation service for managing LLM conversations
//!
//! This service provides business logic for conversation management including:
//! - Creating and managing conversations
//! - Adding messages and tracking tokens
//! - Pruning conversation history for token limits
//! - Managing document context for RAG
//!
//! # Architecture
//!
//! Following the service layer pattern:
//! - **Service**: Business logic and orchestration
//! - **Repository**: Data persistence
//! - **Domain**: Business rules and invariants
//!
//! # Usage
//!
//! ```rust,no_run
//! use lattice::services::ConversationService;
//! use std::sync::Arc;
//! use lattice_core::application::ports::conversation_repository::ConversationRepositoryPort;
//!
//! # async fn example(repository: Arc<dyn ConversationRepositoryPort>) -> Result<(), Box<dyn std::error::Error>> {
//! let service = ConversationService::new(repository);
//!
//! // Create conversation
//! let conversation = service.create_conversation(
//!     "My Chat".to_string(),
//!     "claude-sonnet-4-5-20250929".to_string(),
//!     Some("You are helpful.".to_string())
//! ).await?;
//!
//! // Add message
//! service.add_user_message(
//!     &conversation.id.to_string(),
//!     "Hello!".to_string(),
//!     10
//! ).await?;
//!
//! # Ok(())
//! # }
//! ```

use lattice_core::application::ports::conversation_repository::ConversationRepositoryPort;
use lattice_core::domain::conversation::{
    Conversation, ConversationAggregate, ConversationMessage, MessageRole,
};
use lattice_core::shared::error::{AppError, Result};
use std::sync::Arc;

/// Service for conversation management
///
/// Provides high-level operations for managing conversations,
/// coordinating between domain logic and repository persistence.
pub struct ConversationService {
    repository: Arc<dyn ConversationRepositoryPort>,
}

impl ConversationService {
    /// Create a new conversation service
    ///
    /// # Arguments
    /// * `repository` - Conversation persistence adapter
    pub fn new(repository: Arc<dyn ConversationRepositoryPort>) -> Self {
        Self { repository }
    }

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
    /// ```rust,no_run
    /// # use lattice::services::ConversationService;
    /// # async fn example(service: ConversationService) -> Result<(), Box<dyn std::error::Error>> {
    /// let conversation = service.create_conversation(
    ///     "My Chat".to_string(),
    ///     "claude-sonnet-4-5-20250929".to_string(),
    ///     Some("You are a helpful assistant.".to_string())
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_conversation(
        &self,
        title: String,
        model_name: String,
        system_prompt: Option<String>,
    ) -> Result<Conversation> {
        let _aggregate =
            ConversationAggregate::new(title.clone(), model_name.clone(), system_prompt.clone())?;

        // Persist to database
        self.repository
            .create(&title, &model_name, system_prompt.as_deref())
            .await
    }

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
    pub async fn get_conversation(&self, id: &str) -> Result<Option<ConversationAggregate>> {
        self.repository.load_aggregate(id).await
    }

    /// List conversations ordered by most recently updated
    ///
    /// # Arguments
    /// * `limit` - Maximum number of conversations to return (default: 100)
    /// * `offset` - Number of conversations to skip (for pagination, default: 0)
    ///
    /// # Returns
    /// Vector of conversation entities (without messages)
    ///
    /// # Errors
    /// - `AppError::Database` if database query fails
    ///
    /// # Example
    /// ```rust,no_run
    /// # use lattice::services::ConversationService;
    /// # async fn example(service: ConversationService) -> Result<(), Box<dyn std::error::Error>> {
    /// // Get first page of 20 conversations
    /// let conversations = service.list_conversations(Some(20), Some(0)).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_conversations(
        &self,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Conversation>> {
        self.repository.list(limit, offset).await
    }

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
    pub async fn delete_conversation(&self, id: &str) -> Result<()> {
        self.repository.delete(id).await
    }

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
    pub async fn rename_conversation(&self, id: &str, new_title: String) -> Result<()> {
        if new_title.trim().is_empty() {
            return Err(AppError::InvalidInput("Title cannot be empty".into()));
        }

        self.repository.update_title(id, &new_title).await
    }

    /// Update the system prompt of a conversation
    ///
    /// # Arguments
    /// * `id` - Conversation ID
    /// * `system_prompt` - New system prompt (None to remove)
    ///
    /// # Errors
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::Database` if update fails
    pub async fn update_system_prompt(
        &self,
        id: &str,
        system_prompt: Option<String>,
    ) -> Result<()> {
        self.repository
            .update_system_prompt(id, system_prompt.as_deref())
            .await
    }

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
    ///
    /// # Example
    /// ```rust,no_run
    /// # use lattice::services::ConversationService;
    /// # async fn example(service: ConversationService, conversation_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    /// let message = service.add_user_message(
    ///     conversation_id,
    ///     "What is RAG?".to_string(),
    ///     50
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
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

    /// Add an assistant message with metadata to the conversation
    ///
    /// # Arguments
    /// * `conversation_id` - Conversation ID
    /// * `content` - Message content
    /// * `tokens` - Token count for this message
    /// * `metadata` - Optional JSON metadata (e.g., serialized sources)
    ///
    /// # Returns
    /// The created message
    ///
    /// # Errors
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::InvalidInput` if content is empty
    /// - `AppError::Database` if insert fails
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
    /// ```rust,no_run
    /// # use lattice::services::ConversationService;
    /// # async fn example(service: ConversationService, conversation_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    /// // Keep only recent 10K tokens of history
    /// service.prune_conversation_to_limit(conversation_id, 10000).await?;
    /// # Ok(())
    /// # }
    /// ```
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

        // Capture all message IDs before pruning
        let before_ids: std::collections::HashSet<String> =
            aggregate.messages().iter().map(|m| m.id.clone()).collect();

        aggregate.prune_to_token_limit(max_tokens)?;

        // Collect IDs of messages that were kept
        let after_ids: std::collections::HashSet<String> =
            aggregate.messages().iter().map(|m| m.id.clone()).collect();

        // Find messages that were removed (in before but not in after)
        let messages_to_delete: Vec<String> = before_ids.difference(&after_ids).cloned().collect();

        if !messages_to_delete.is_empty() {
            self.repository
                .delete_messages(conversation_id, &messages_to_delete)
                .await?;
        }

        Ok(())
    }

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

    /// Add a message with a specific status (for two-phase commit).
    ///
    /// This method supports the two-phase commit pattern:
    /// 1. Save user message with 'pending' status
    /// 2. Generate LLM response (can fail)
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
    /// The created message with message_id
    ///
    /// # Errors
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::InvalidInput` if content is empty
    /// - `AppError::Database` if insert fails
    ///
    /// # Example
    /// ```rust,no_run
    /// # use lattice::services::ConversationService;
    /// # async fn example(service: ConversationService, conversation_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    /// // Two-phase commit pattern
    /// let user_msg = service.add_message_with_status(
    ///     conversation_id,
    ///     "What is RAG?",
    ///     50,
    ///     "pending" // Mark as pending
    /// ).await?;
    ///
    /// // Try operation (e.g., LLM generation)
    /// match generate_response().await {
    ///     Ok(response) => {
    ///         // Success: mark completed
    ///         service.update_message_status(&user_msg.id, "completed").await?;
    ///         service.add_message_with_status(conversation_id, response, 200, "completed").await?;
    ///     }
    ///     Err(e) => {
    ///         // Failure: mark failed
    ///         service.update_message_status(&user_msg.id, "failed").await?;
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
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

    /// Update the status of a message (for two-phase commit).
    ///
    /// # Arguments
    /// * `message_id` - ID of the message to update
    /// * `status` - New status ('pending', 'completed', 'failed')
    ///
    /// # Errors
    /// - `AppError::NotFound` if message doesn't exist
    /// - `AppError::Database` if update fails
    ///
    /// # Example
    /// ```rust,no_run
    /// # use lattice::services::ConversationService;
    /// # async fn example(service: ConversationService, message_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    /// // Mark message as completed after operation succeeds
    /// service.update_message_status(message_id, "completed").await?;
    /// # Ok(())
    /// # }
    /// ```
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
        model_name: String,
        system_prompt: Option<String>,
    ) -> Result<Conversation> {
        self.create_conversation(title, model_name, system_prompt)
            .await
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
