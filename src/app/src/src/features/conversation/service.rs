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
//! use sqlx::SqlitePool;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let pool = SqlitePool::connect("sqlite::memory:").await?;
//! let service = ConversationService::new(pool);
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

use crate::domain::conversation::{
    Conversation, ConversationAggregate, ConversationMessage, MessageRole,
};
use crate::infrastructure::persistence::repositories::ConversationRepository;
use crate::shared::error::{AppError, Result};
use sqlx::SqlitePool;

/// Service for conversation management
///
/// Provides high-level operations for managing conversations,
/// coordinating between domain logic and repository persistence.
pub struct ConversationService {
    repository: ConversationRepository,
}

impl ConversationService {
    /// Create a new conversation service
    ///
    /// # Arguments
    /// * `pool` - SQLite connection pool
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            repository: ConversationRepository::new(pool),
        }
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
        // Validate using domain logic first
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
        // Validate using domain logic
        if new_title.trim().is_empty() {
            return Err(AppError::InvalidInput("Title cannot be empty".into()));
        }

        self.repository
            .update(id, Some(&new_title), None, None, None)
            .await
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
            .update(id, None, Some(system_prompt.as_deref()), None, None)
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
        // Validate content
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
        // Validate content
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
        // Validate content
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

        // Load full aggregate to perform domain logic
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

        // Use domain method to prune (maintains invariants)
        aggregate.prune_to_token_limit(max_tokens)?;

        // Collect IDs of messages that were kept
        let after_ids: std::collections::HashSet<String> =
            aggregate.messages().iter().map(|m| m.id.clone()).collect();

        // Find messages that were removed (in before but not in after)
        let messages_to_delete: Vec<String> = before_ids.difference(&after_ids).cloned().collect();

        if !messages_to_delete.is_empty() {
            // Delete removed messages from database
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
        // Validate content
        if content.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Message content cannot be empty".into(),
            ));
        }

        self.repository
            .add_message_with_status_repo(conversation_id, role, &content, tokens, None, &status)
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
            .update_message_status_repo(message_id, &status)
            .await
    }
}

// ============================================================================
// ConversationServiceTrait Implementation
// ============================================================================

#[async_trait::async_trait]
impl crate::features::conversation::ConversationServiceTrait for ConversationService {
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

    async fn update_message_status(&self, message_id: &str, status: String) -> Result<()> {
        self.update_message_status(message_id, status).await
    }
}

#[async_trait::async_trait]
impl crate::application::ports::ConversationHistoryPort for ConversationService {
    async fn get_conversation(&self, id: &str) -> Result<Option<ConversationAggregate>> {
        self.get_conversation(id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // Test Helpers
    // ============================================================================

    /// Create test database pool with schema
    async fn create_test_pool() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();

        // Run migrations
        sqlx::query(
            r#"
            CREATE TABLE conversations (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                model_name TEXT NOT NULL,
                system_prompt TEXT,
                space_id TEXT NOT NULL DEFAULT 'space_general',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                message_count INTEGER NOT NULL DEFAULT 0,
                total_tokens INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE conversation_spaces (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL
            );

            INSERT INTO conversation_spaces (id, name) VALUES ('space_general', 'General');

            CREATE TABLE conversation_messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                role TEXT NOT NULL CHECK(role IN ('user', 'assistant', 'system')),
                content TEXT NOT NULL,
                tokens INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                metadata TEXT,
                status TEXT NOT NULL DEFAULT 'completed',
                FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            );

            CREATE TABLE conversation_documents (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                conversation_id TEXT NOT NULL,
                document_id TEXT NOT NULL,
                chunk_id TEXT,
                relevance_score REAL,
                added_at TEXT NOT NULL,
                FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE,
                UNIQUE(conversation_id, chunk_id)
            );

            CREATE TABLE document_space_memberships (
                document_id TEXT NOT NULL,
                space_id TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (document_id, space_id)
            );
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    /// Create test service with fresh database
    async fn create_test_service() -> (ConversationService, SqlitePool) {
        let pool = create_test_pool().await;
        let service = ConversationService::new(pool.clone());
        (service, pool)
    }

    /// Count messages for a conversation (for CASCADE DELETE verification)
    async fn count_messages(pool: &SqlitePool, conversation_id: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM conversation_messages WHERE conversation_id = ?",
        )
        .bind(conversation_id)
        .fetch_one(pool)
        .await
        .unwrap_or(0)
    }

    /// Count document references for a conversation (for CASCADE DELETE verification)
    async fn count_document_refs(pool: &SqlitePool, conversation_id: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM conversation_documents WHERE conversation_id = ?",
        )
        .bind(conversation_id)
        .fetch_one(pool)
        .await
        .unwrap_or(0)
    }

    // ============================================================================
    // Category 1: Conversation Lifecycle (P0)
    // ============================================================================

    #[tokio::test]
    async fn test_create_conversation_with_system_prompt() {
        // ARRANGE
        let (service, _pool) = create_test_service().await;

        // ACT
        let result = service
            .create_conversation(
                "Test Chat".to_string(),
                "claude-sonnet-4-5-20250929".to_string(),
                Some("You are a helpful assistant.".to_string()),
            )
            .await;

        // ASSERT
        let conversation = result.expect("create_conversation should succeed");
        assert_eq!(conversation.title, "Test Chat");
        assert_eq!(conversation.model_name, "claude-sonnet-4-5-20250929");
        assert_eq!(
            conversation.system_prompt,
            Some("You are a helpful assistant.".to_string())
        );
        assert_eq!(conversation.message_count, 0);
        assert_eq!(conversation.total_tokens, 0);
        assert!(!conversation.id.to_string().is_empty());
    }

    #[tokio::test]
    async fn test_create_conversation_rejects_empty_title() {
        // ARRANGE
        let (service, _pool) = create_test_service().await;

        // ACT
        let result = service
            .create_conversation(
                "".to_string(),
                "claude-sonnet-4-5-20250929".to_string(),
                None,
            )
            .await;

        // ASSERT
        assert!(result.is_err());
        match result {
            Err(AppError::InvalidInput(msg)) => {
                assert!(msg.contains("title") || msg.contains("empty"));
            }
            _ => panic!("Expected AppError::InvalidInput"),
        }
    }

    #[tokio::test]
    async fn test_get_conversation_loads_aggregate() {
        // ARRANGE
        let (service, _pool) = create_test_service().await;

        let conversation = service
            .create_conversation(
                "Test Conv".to_string(),
                "claude-sonnet-4-5-20250929".to_string(),
                Some("System prompt".to_string()),
            )
            .await
            .expect("create should succeed");

        // Add a message
        service
            .add_user_message(&conversation.id.to_string(), "Hello!".to_string(), 10)
            .await
            .expect("add_user_message should succeed");

        // ACT
        let result = service.get_conversation(&conversation.id.to_string()).await;

        // ASSERT
        let aggregate = result
            .expect("get_conversation should succeed")
            .expect("Conversation should exist");

        assert_eq!(aggregate.conversation().id, conversation.id);
        assert_eq!(aggregate.conversation().title, "Test Conv");
        assert_eq!(aggregate.message_count(), 1);
        assert_eq!(aggregate.messages().len(), 1);
        assert_eq!(aggregate.messages()[0].content, "Hello!");
    }

    #[tokio::test]
    async fn test_get_conversation_returns_none_for_missing() {
        // ARRANGE
        let (service, _pool) = create_test_service().await;

        // ACT
        let result = service.get_conversation("nonexistent-id").await;

        // ASSERT
        let option = result.expect("get_conversation should succeed");
        assert!(option.is_none());
    }

    #[tokio::test]
    async fn test_list_conversations_with_pagination() {
        // ARRANGE
        let (service, _pool) = create_test_service().await;

        // Create 5 conversations
        for i in 0..5 {
            service
                .create_conversation(
                    format!("Conv {}", i),
                    "claude-sonnet-4-5-20250929".to_string(),
                    None,
                )
                .await
                .expect("create should succeed");
        }

        // ACT
        let page1 = service
            .list_conversations(Some(3), Some(0))
            .await
            .expect("list should succeed");
        let page2 = service
            .list_conversations(Some(3), Some(3))
            .await
            .expect("list should succeed");

        // ASSERT
        assert_eq!(page1.len(), 3);
        assert_eq!(page2.len(), 2);

        // Verify no overlap
        let page1_ids: Vec<_> = page1.iter().map(|c| c.id.clone()).collect();
        let page2_ids: Vec<_> = page2.iter().map(|c| c.id.clone()).collect();
        assert!(page1_ids.iter().all(|id| !page2_ids.contains(id)));
    }

    #[tokio::test]
    async fn test_rename_conversation() {
        // ARRANGE
        let (service, _pool) = create_test_service().await;

        let conversation = service
            .create_conversation(
                "Old Title".to_string(),
                "claude-sonnet-4-5-20250929".to_string(),
                None,
            )
            .await
            .expect("create should succeed");

        // ACT
        service
            .rename_conversation(&conversation.id.to_string(), "New Title".to_string())
            .await
            .expect("rename should succeed");

        // ASSERT
        let updated = service
            .get_conversation(&conversation.id.to_string())
            .await
            .expect("get should succeed")
            .expect("Conversation should exist");

        assert_eq!(updated.conversation().title, "New Title");
    }

    // ============================================================================
    // Category 2: Message Management (P0)
    // ============================================================================

    #[tokio::test]
    async fn test_add_user_message() {
        // ARRANGE
        let (service, _pool) = create_test_service().await;

        let conversation = service
            .create_conversation(
                "Test".to_string(),
                "claude-sonnet-4-5-20250929".to_string(),
                None,
            )
            .await
            .expect("create should succeed");

        // ACT
        let result = service
            .add_user_message(&conversation.id.to_string(), "Hello!".to_string(), 10)
            .await;

        // ASSERT
        let message = result.expect("add_user_message should succeed");
        assert_eq!(message.content, "Hello!");
        assert_eq!(message.tokens, 10);
        assert!(matches!(message.role, MessageRole::User));
        assert_eq!(message.conversation_id, conversation.id);
    }

    #[tokio::test]
    async fn test_add_assistant_message() {
        // ARRANGE
        let (service, _pool) = create_test_service().await;

        let conversation = service
            .create_conversation(
                "Test".to_string(),
                "claude-sonnet-4-5-20250929".to_string(),
                None,
            )
            .await
            .expect("create should succeed");

        // ACT
        let result = service
            .add_assistant_message(&conversation.id.to_string(), "Hi there!".to_string(), 20)
            .await;

        // ASSERT
        let message = result.expect("add_assistant_message should succeed");
        assert_eq!(message.content, "Hi there!");
        assert_eq!(message.tokens, 20);
        assert!(matches!(message.role, MessageRole::Assistant));
    }

    #[tokio::test]
    async fn test_add_assistant_message_with_metadata() {
        // ARRANGE
        let (service, _pool) = create_test_service().await;

        let conversation = service
            .create_conversation(
                "Test".to_string(),
                "claude-sonnet-4-5-20250929".to_string(),
                None,
            )
            .await
            .expect("create should succeed");

        let metadata = r#"{"sources": [{"title": "doc1", "score": 0.9}]}"#;

        // ACT
        let result = service
            .add_assistant_message_with_metadata(
                &conversation.id.to_string(),
                "Answer with sources".to_string(),
                50,
                Some(metadata.to_string()),
            )
            .await;

        // ASSERT
        let message = result.expect("add_assistant_message_with_metadata should succeed");
        assert_eq!(message.content, "Answer with sources");
        assert_eq!(message.tokens, 50);
        assert!(matches!(message.role, MessageRole::Assistant));
        assert_eq!(message.metadata, Some(metadata.to_string()));
    }

    #[tokio::test]
    async fn test_add_message_rejects_empty_content() {
        // ARRANGE
        let (service, _pool) = create_test_service().await;

        let conversation = service
            .create_conversation(
                "Test".to_string(),
                "claude-sonnet-4-5-20250929".to_string(),
                None,
            )
            .await
            .expect("create should succeed");

        // ACT
        let result = service
            .add_user_message(&conversation.id.to_string(), "".to_string(), 0)
            .await;

        // ASSERT
        assert!(result.is_err());
        match result {
            Err(AppError::InvalidInput(msg)) => {
                assert!(msg.contains("content") || msg.contains("empty"));
            }
            _ => panic!("Expected AppError::InvalidInput"),
        }
    }

    #[tokio::test]
    async fn test_add_message_to_nonexistent_conversation() {
        // ARRANGE
        let (service, _pool) = create_test_service().await;

        // ACT
        let result = service
            .add_user_message("fake-conversation-id", "Orphan message".to_string(), 10)
            .await;

        // ASSERT
        assert!(result.is_err());
        // Should be Database error or NotFound due to foreign key constraint
        match result {
            Err(AppError::Database(_)) | Err(AppError::NotFound(_)) => {}
            _ => panic!("Expected AppError::Database or AppError::NotFound"),
        }
    }

    #[tokio::test]
    async fn test_prune_conversation_to_token_limit() {
        // ARRANGE
        let (service, _pool) = create_test_service().await;

        let conversation = service
            .create_conversation(
                "Test".to_string(),
                "claude-sonnet-4-5-20250929".to_string(),
                None,
            )
            .await
            .expect("create should succeed");

        // Add 5 messages (100 tokens each)
        for i in 0..5 {
            service
                .add_user_message(&conversation.id.to_string(), format!("Message {}", i), 100)
                .await
                .expect("add message should succeed");
        }

        // ACT
        service
            .prune_conversation_to_limit(&conversation.id.to_string(), 250)
            .await
            .expect("prune should succeed");

        // ASSERT
        let aggregate = service
            .get_conversation(&conversation.id.to_string())
            .await
            .expect("get should succeed")
            .expect("Conversation should exist");

        assert!(aggregate.total_tokens() <= 250);
        assert!(aggregate.message_count() >= 1); // At least 1 message preserved
    }

    #[tokio::test]
    async fn test_prune_rejects_invalid_token_limit() {
        // ARRANGE
        let (service, _pool) = create_test_service().await;

        let conversation = service
            .create_conversation(
                "Test".to_string(),
                "claude-sonnet-4-5-20250929".to_string(),
                None,
            )
            .await
            .expect("create should succeed");

        // ACT
        let result = service
            .prune_conversation_to_limit(&conversation.id.to_string(), 0)
            .await;

        // ASSERT
        assert!(result.is_err());
        match result {
            Err(AppError::InvalidInput(msg)) => {
                assert!(msg.contains("limit") || msg.contains("positive"));
            }
            _ => panic!("Expected AppError::InvalidInput"),
        }
    }

    // ============================================================================
    // Category 3: CASCADE DELETE Testing (P0 - CRITICAL)
    // ============================================================================

    #[tokio::test]
    async fn test_delete_conversation_cascades_to_messages() {
        // ARRANGE
        let (service, pool) = create_test_service().await;

        let conversation = service
            .create_conversation(
                "Test Conv".to_string(),
                "claude-sonnet-4-5-20250929".to_string(),
                None,
            )
            .await
            .expect("create should succeed");

        let conv_id = conversation.id.to_string();

        // Add 3 messages
        service
            .add_user_message(&conv_id, "msg1".to_string(), 10)
            .await
            .expect("add message should succeed");
        service
            .add_user_message(&conv_id, "msg2".to_string(), 10)
            .await
            .expect("add message should succeed");
        service
            .add_user_message(&conv_id, "msg3".to_string(), 10)
            .await
            .expect("add message should succeed");

        // Verify messages exist
        let before_count = count_messages(&pool, &conv_id).await;
        assert_eq!(before_count, 3);

        // ACT
        service
            .delete_conversation(&conv_id)
            .await
            .expect("delete should succeed");

        // ASSERT
        // 1. Conversation deleted
        let conv_result = service
            .get_conversation(&conv_id)
            .await
            .expect("get should succeed");
        assert!(conv_result.is_none(), "Conversation should be deleted");

        // 2. **CRITICAL**: Verify messages cascade deleted
        let after_count = count_messages(&pool, &conv_id).await;
        assert_eq!(after_count, 0, "All messages must cascade delete");
    }

    #[tokio::test]
    async fn test_delete_conversation_cascades_to_document_references() {
        // ARRANGE
        let (service, pool) = create_test_service().await;

        let conversation = service
            .create_conversation(
                "Test Conv".to_string(),
                "claude-sonnet-4-5-20250929".to_string(),
                None,
            )
            .await
            .expect("create should succeed");

        let conv_id = conversation.id.to_string();

        // Add 2 document references
        service
            .add_document_reference(
                &conv_id,
                "doc-123".to_string(),
                Some("chunk-456".to_string()),
                Some(0.95),
            )
            .await
            .expect("add reference should succeed");
        service
            .add_document_reference(
                &conv_id,
                "doc-789".to_string(),
                Some("chunk-012".to_string()),
                Some(0.88),
            )
            .await
            .expect("add reference should succeed");

        // Verify refs exist
        let before_count = count_document_refs(&pool, &conv_id).await;
        assert_eq!(before_count, 2);

        // ACT
        service
            .delete_conversation(&conv_id)
            .await
            .expect("delete should succeed");

        // ASSERT
        // **CRITICAL**: Verify document references cascade deleted
        let after_count = count_document_refs(&pool, &conv_id).await;
        assert_eq!(
            after_count, 0,
            "All document references must cascade delete"
        );
    }

    #[tokio::test]
    async fn test_delete_nonexistent_conversation() {
        // ARRANGE
        let (service, _pool) = create_test_service().await;

        // ACT
        let result = service.delete_conversation("nonexistent-id").await;

        // ASSERT
        assert!(result.is_err());
        match result {
            Err(AppError::NotFound(_)) => {}
            _ => panic!("Expected AppError::NotFound"),
        }
    }

    #[tokio::test]
    async fn test_orphaned_messages_prevented_by_foreign_key() {
        // ARRANGE
        let (service, _pool) = create_test_service().await;

        // ACT
        // Attempt to add message to nonexistent conversation
        let result = service
            .add_user_message(
                "fake-conversation-id",
                "Orphaned message attempt".to_string(),
                10,
            )
            .await;

        // ASSERT
        assert!(result.is_err());
        // Should be Database error due to foreign key constraint
        match result {
            Err(AppError::Database(msg)) | Err(AppError::NotFound(msg)) => {
                let msg_lower = msg.to_lowercase();
                assert!(
                    msg_lower.contains("foreign key")
                        || msg_lower.contains("constraint")
                        || msg_lower.contains("not found")
                );
            }
            _ => {
                panic!("Expected AppError::Database or AppError::NotFound with foreign key message")
            }
        }
    }

    // ============================================================================
    // Category 4: System Prompt and Metadata (P1)
    // ============================================================================

    #[tokio::test]
    async fn test_update_system_prompt() {
        // ARRANGE
        let (service, _pool) = create_test_service().await;

        let conversation = service
            .create_conversation(
                "Test".to_string(),
                "claude-sonnet-4-5-20250929".to_string(),
                Some("Old prompt".to_string()),
            )
            .await
            .expect("create should succeed");

        // ACT
        service
            .update_system_prompt(&conversation.id.to_string(), Some("New prompt".to_string()))
            .await
            .expect("update should succeed");

        // ASSERT
        let updated = service
            .get_conversation(&conversation.id.to_string())
            .await
            .expect("get should succeed")
            .expect("Conversation should exist");

        assert_eq!(
            updated.conversation().system_prompt,
            Some("New prompt".to_string())
        );
    }

    #[tokio::test]
    async fn test_update_system_prompt_to_none() {
        // ARRANGE
        let (service, _pool) = create_test_service().await;

        let conversation = service
            .create_conversation(
                "Test".to_string(),
                "claude-sonnet-4-5-20250929".to_string(),
                Some("Old prompt".to_string()),
            )
            .await
            .expect("create should succeed");

        // ACT
        service
            .update_system_prompt(&conversation.id.to_string(), None)
            .await
            .expect("update should succeed");

        // ASSERT
        let updated = service
            .get_conversation(&conversation.id.to_string())
            .await
            .expect("get should succeed")
            .expect("Conversation should exist");

        assert_eq!(updated.conversation().system_prompt, None);
    }

    // ============================================================================
    // Category 5: Document References (P1)
    // ============================================================================

    #[tokio::test]
    async fn test_add_document_reference() {
        // ARRANGE
        let (service, _pool) = create_test_service().await;

        let conversation = service
            .create_conversation(
                "Test".to_string(),
                "claude-sonnet-4-5-20250929".to_string(),
                None,
            )
            .await
            .expect("create should succeed");

        // ACT
        service
            .add_document_reference(
                &conversation.id.to_string(),
                "doc-123".to_string(),
                Some("chunk-456".to_string()),
                Some(0.95),
            )
            .await
            .expect("add reference should succeed");

        // ASSERT
        let aggregate = service
            .get_conversation(&conversation.id.to_string())
            .await
            .expect("get should succeed")
            .expect("Conversation should exist");

        assert_eq!(aggregate.document_context().len(), 1);
        assert_eq!(
            aggregate.document_context()[0].document_id.as_str(),
            "doc-123"
        );
    }

    #[tokio::test]
    async fn test_get_conversation_includes_document_references() {
        // ARRANGE
        let (service, _pool) = create_test_service().await;

        let conversation = service
            .create_conversation(
                "Test".to_string(),
                "claude-sonnet-4-5-20250929".to_string(),
                None,
            )
            .await
            .expect("create should succeed");

        let conv_id = conversation.id.to_string();

        // Add 2 document references
        service
            .add_document_reference(
                &conv_id,
                "doc-1".to_string(),
                Some("chunk-1".to_string()),
                Some(0.9),
            )
            .await
            .expect("add reference should succeed");
        service
            .add_document_reference(
                &conv_id,
                "doc-2".to_string(),
                Some("chunk-2".to_string()),
                Some(0.8),
            )
            .await
            .expect("add reference should succeed");

        // ACT
        let result = service.get_conversation(&conv_id).await;

        // ASSERT
        let aggregate = result
            .expect("get should succeed")
            .expect("Conversation should exist");

        assert_eq!(aggregate.document_context().len(), 2);
    }

    // ============================================================================
    // Category 6: Two-Phase Commit (P2)
    // ============================================================================

    #[tokio::test]
    async fn test_two_phase_commit_pattern() {
        // ARRANGE
        let (service, _pool) = create_test_service().await;

        let conversation = service
            .create_conversation(
                "Test".to_string(),
                "claude-sonnet-4-5-20250929".to_string(),
                None,
            )
            .await
            .expect("create should succeed");

        let conv_id = conversation.id.to_string();

        // ACT
        // Phase 1: Add message with "pending" status
        let result = service
            .add_message_with_status(
                &conv_id,
                MessageRole::User,
                "Question?".to_string(),
                10,
                "pending".to_string(),
            )
            .await;

        let message = result.expect("add_message_with_status should succeed");
        assert_eq!(message.status, "pending");

        // Simulate LLM processing...

        // Phase 2: Update status to "completed"
        service
            .update_message_status(&message.id.to_string(), "completed".to_string())
            .await
            .expect("update_message_status should succeed");

        // ASSERT
        let aggregate = service
            .get_conversation(&conv_id)
            .await
            .expect("get should succeed")
            .expect("Conversation should exist");

        let updated_msg = &aggregate.messages()[0];
        assert_eq!(updated_msg.status, "completed");
    }

    #[tokio::test]
    async fn test_two_phase_commit_failure() {
        // ARRANGE
        let (service, _pool) = create_test_service().await;

        let conversation = service
            .create_conversation(
                "Test".to_string(),
                "claude-sonnet-4-5-20250929".to_string(),
                None,
            )
            .await
            .expect("create should succeed");

        let conv_id = conversation.id.to_string();

        // Phase 1: Add message with "pending" status
        let message = service
            .add_message_with_status(
                &conv_id,
                MessageRole::User,
                "Question?".to_string(),
                10,
                "pending".to_string(),
            )
            .await
            .expect("add_message_with_status should succeed");

        // Simulate LLM failure...

        // ACT
        // Phase 2: Update status to "failed"
        service
            .update_message_status(&message.id.to_string(), "failed".to_string())
            .await
            .expect("update_message_status should succeed");

        // ASSERT
        let aggregate = service
            .get_conversation(&conv_id)
            .await
            .expect("get should succeed")
            .expect("Conversation should exist");

        let updated_msg = &aggregate.messages()[0];
        assert_eq!(updated_msg.status, "failed");
    }

    #[tokio::test]
    async fn test_update_message_status_nonexistent_message() {
        // ARRANGE
        let (service, _pool) = create_test_service().await;

        // ACT
        let result = service
            .update_message_status("fake-message-id", "completed".to_string())
            .await;

        // ASSERT
        assert!(result.is_err());
        match result {
            Err(AppError::NotFound(_)) => {}
            _ => panic!("Expected AppError::NotFound"),
        }
    }
}
