//! Conversation Repository Implementation
//!
//! Infrastructure implementation for conversation persistence using SQLite.
//!
//! # Architecture
//!
//! This repository implements the ConversationRepositoryPort trait,
//! providing CRUD operations for conversations, messages, and document
//! references using SQLite. All database operations go through the
//! mapper layer to maintain clean separation between domain entities
//! and database models.
//!
//! # Features
//!
//! - Full conversation CRUD
//! - Message persistence with automatic token counting
//! - Document reference tracking
//! - Aggregate loading (conversation + messages + references)
//! - Transaction support for consistency

use crate::application::ports::ConversationRepositoryPort;
use crate::domain::conversation::{
    Conversation, ConversationAggregate, ConversationMessage, DocumentReference, MessageRole,
};
use crate::domain_types::ConversationId;
use crate::infrastructure::persistence::mappers::{
    ConversationMapper, ConversationMessageMapper, ConversationMessageModel, ConversationModel,
    DocumentReferenceMapper, DocumentReferenceModel,
};
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use chrono::Utc;
use sqlx::SqlitePool;
use std::str::FromStr;

/// SQLite implementation of conversation repository.
///
/// Handles persistence of conversation metadata, messages, and document references.
/// Uses mappers to convert between domain entities and database models.
///
/// # Thread Safety
///
/// This repository is thread-safe through SQLitePool's internal connection management.
#[derive(Debug, Clone)]
pub struct ConversationRepository {
    pool: SqlitePool,
}

impl ConversationRepository {
    /// Create a new conversation repository.
    ///
    /// # Arguments
    ///
    /// * `pool` - SQLite connection pool
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Alias for create_conversation with no system prompt
    ///
    /// # Arguments
    /// * `title` - Conversation title
    /// * `model_name` - LLM model identifier
    /// * `system_prompt` - Optional system prompt
    ///
    /// # Returns
    /// Newly created conversation entity
    pub async fn create(
        &self,
        title: &str,
        model_name: &str,
        system_prompt: Option<&str>,
    ) -> Result<Conversation> {
        self.create_conversation(title, model_name, system_prompt)
            .await
    }

    /// List all conversations with pagination
    ///
    /// # Arguments
    /// * `limit` - Maximum number of conversations to return
    /// * `offset` - Number of conversations to skip
    ///
    /// # Returns
    /// Vector of conversation entities ordered by updated_at DESC
    pub async fn list(&self, limit: Option<i64>, offset: Option<i64>) -> Result<Vec<Conversation>> {
        self.find_all(limit, offset).await
    }

    /// Update conversation metadata
    ///
    /// # Arguments
    /// * `id` - Conversation ID
    /// * `title` - Optional new title
    /// * `system_prompt` - Optional new system prompt (Some(None) to clear)
    /// * `message_count` - Optional new message count
    /// * `total_tokens` - Optional new total tokens
    ///
    /// # Returns
    /// Updated conversation or error if not found
    pub async fn update(
        &self,
        id: &str,
        title: Option<&str>,
        system_prompt: Option<Option<&str>>,
        _message_count: Option<i64>,
        _total_tokens: Option<i64>,
    ) -> Result<()> {
        if let Some(new_title) = title {
            self.update_title(id, new_title).await?;
        }

        if let Some(new_system_prompt) = system_prompt {
            self.update_system_prompt(id, new_system_prompt).await?;
        }

        Ok(())
    }

    /// Load full conversation aggregate
    ///
    /// # Arguments
    /// * `id` - Conversation ID
    ///
    /// # Returns
    /// Full aggregate with messages and document references
    pub async fn load_aggregate(&self, id: &str) -> Result<Option<ConversationAggregate>> {
        self.find_aggregate_by_id(id).await
    }

    /// Delete specific messages from a conversation
    ///
    /// # Arguments
    /// * `conversation_id` - Conversation ID
    /// * `message_ids` - Array of message IDs to delete
    ///
    /// # Returns
    /// Number of messages deleted
    pub async fn delete_messages(
        &self,
        conversation_id: &str,
        message_ids: &[String],
    ) -> Result<u64> {
        if message_ids.is_empty() {
            return Ok(0);
        }

        let mut total_deleted = 0;

        for message_id in message_ids {
            let result = sqlx::query!("DELETE FROM conversation_messages WHERE id = ?", message_id)
                .execute(&self.pool)
                .await
                .map_err(|e| AppError::Database(format!("Failed to delete message: {}", e)))?;

            total_deleted += result.rows_affected();
        }

        // Recount messages and tokens for the conversation
        #[derive(sqlx::FromRow)]
        struct MessageStats {
            count: i64,
            total: i64,
        }

        let stats = sqlx::query_as::<_, MessageStats>(
            r#"
            SELECT COUNT(*) as count, COALESCE(SUM(tokens), 0) as total
            FROM conversation_messages
            WHERE conversation_id = ?
            "#,
        )
        .bind(conversation_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to recount messages: {}", e)))?;

        let now = Utc::now().to_rfc3339();
        sqlx::query!(
            r#"
            UPDATE conversations
            SET message_count = ?,
                total_tokens = ?,
                updated_at = ?
            WHERE id = ?
            "#,
            stats.count,
            stats.total,
            now,
            conversation_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to update conversation stats: {}", e)))?;

        Ok(total_deleted)
    }

    /// Delete a conversation by ID (delegates to trait method)
    ///
    /// # Arguments
    /// * `id` - Conversation ID
    ///
    /// # Errors
    /// - `AppError::NotFound` if conversation doesn't exist
    /// - `AppError::Database` if deletion fails
    pub async fn delete(&self, id: &str) -> Result<()> {
        ConversationRepositoryPort::delete(self, id).await
    }

    /// Add a message to a conversation (delegates to trait method)
    ///
    /// # Arguments
    /// * `conversation_id` - Conversation ID
    /// * `role` - Message role (User, Assistant, System)
    /// * `content` - Message content
    /// * `tokens` - Token count
    /// * `metadata` - Optional metadata JSON string
    ///
    /// # Returns
    /// Created message entity
    pub async fn add_message(
        &self,
        conversation_id: &str,
        role: MessageRole,
        content: &str,
        tokens: i64,
        metadata: Option<&str>,
    ) -> Result<ConversationMessage> {
        ConversationRepositoryPort::add_message(
            self,
            conversation_id,
            role,
            content,
            tokens,
            metadata,
        )
        .await
    }

    /// Add a message with a specific status (for two-phase commit)
    ///
    /// # Arguments
    /// * `conversation_id` - Conversation ID
    /// * `role` - Message role (User, Assistant, System)
    /// * `content` - Message content
    /// * `tokens` - Token count
    /// * `metadata` - Optional metadata JSON string
    /// * `status` - Message status ('pending', 'completed', 'failed')
    ///
    /// # Returns
    /// Created message entity with specified status
    pub async fn add_message_with_status_repo(
        &self,
        conversation_id: &str,
        role: MessageRole,
        content: &str,
        tokens: i64,
        metadata: Option<&str>,
        status: &str,
    ) -> Result<ConversationMessage> {
        self.add_message_with_status(conversation_id, role, content, tokens, metadata, status)
            .await
    }

    /// Update the status of a message (for two-phase commit)
    ///
    /// # Arguments
    /// * `message_id` - ID of the message to update
    /// * `status` - New status ('pending', 'completed', 'failed')
    ///
    /// # Returns
    /// Unit result or error if message not found
    pub async fn update_message_status_repo(&self, message_id: &str, status: &str) -> Result<()> {
        self.update_message_status(message_id, status).await
    }

    /// Add a document reference to a conversation (delegates to trait method)
    ///
    /// # Arguments
    /// * `conversation_id` - Conversation ID
    /// * `document_id` - Document ID
    /// * `chunk_id` - Optional chunk ID
    /// * `relevance_score` - Optional relevance score (0.0 to 1.0)
    pub async fn add_document_reference(
        &self,
        conversation_id: &str,
        document_id: &str,
        chunk_id: Option<&str>,
        relevance_score: Option<f32>,
    ) -> Result<()> {
        ConversationRepositoryPort::add_document_reference(
            self,
            conversation_id,
            document_id,
            chunk_id,
            relevance_score,
        )
        .await
    }
}

#[async_trait]
impl ConversationRepositoryPort for ConversationRepository {
    async fn create_conversation(
        &self,
        title: &str,
        model_name: &str,
        system_prompt: Option<&str>,
    ) -> Result<Conversation> {
        let id = ConversationId::new();
        let now = Utc::now().to_rfc3339();

        // Store value to avoid E0716 temporary value dropped error
        let id_str = id.to_string();

        sqlx::query!(
            r#"
            INSERT INTO conversations (id, title, model_name, system_prompt, created_at, updated_at, message_count, total_tokens)
            VALUES (?, ?, ?, ?, ?, ?, 0, 0)
            "#,
            id_str,
            title,
            model_name,
            system_prompt,
            now,
            now
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to create conversation: {}", e)))?;

        Ok(Conversation {
            id,
            title: title.to_string(),
            model_name: model_name.to_string(),
            system_prompt: system_prompt.map(|s| s.to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            message_count: 0,
            total_tokens: 0,
        })
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<Conversation>> {
        let db_model = sqlx::query_as::<_, ConversationModel>(
            r#"
            SELECT id, title, model_name, system_prompt, created_at, updated_at,
                   message_count, total_tokens
            FROM conversations
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to find conversation: {}", e)))?;

        match db_model {
            Some(model) => Ok(Some(ConversationMapper::to_entity(&model)?)),
            None => Ok(None),
        }
    }

    async fn find_aggregate_by_id(&self, id: &str) -> Result<Option<ConversationAggregate>> {
        let conversation = match self.find_by_id(id).await? {
            Some(conv) => conv,
            None => return Ok(None),
        };

        // Load messages
        let messages = self.get_messages(id).await?;

        // Load document references
        let document_refs = self.get_document_references(id).await?;

        // Reconstruct aggregate
        let aggregate =
            ConversationAggregate::from_persistence(conversation, messages, document_refs);

        Ok(Some(aggregate))
    }

    async fn find_all(&self, limit: Option<i64>, offset: Option<i64>) -> Result<Vec<Conversation>> {
        let limit = limit.unwrap_or(100);
        let offset = offset.unwrap_or(0);

        let db_models = sqlx::query_as::<_, ConversationModel>(
            r#"
            SELECT id, title, model_name, system_prompt, created_at, updated_at,
                   message_count, total_tokens
            FROM conversations
            ORDER BY updated_at DESC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to list conversations: {}", e)))?;

        Ok(ConversationMapper::to_entities(&db_models))
    }

    async fn update_title(&self, id: &str, new_title: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();

        let result = sqlx::query!(
            r#"
            UPDATE conversations
            SET title = ?, updated_at = ?
            WHERE id = ?
            "#,
            new_title,
            now,
            id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to update conversation title: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Conversation not found: {}",
                id
            )));
        }

        Ok(())
    }

    async fn update_system_prompt(&self, id: &str, system_prompt: Option<&str>) -> Result<()> {
        let now = Utc::now().to_rfc3339();

        let result = sqlx::query!(
            r#"
            UPDATE conversations
            SET system_prompt = ?, updated_at = ?
            WHERE id = ?
            "#,
            system_prompt,
            now,
            id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to update conversation system prompt: {}",
                e
            ))
        })?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Conversation not found: {}",
                id
            )));
        }

        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;

        let result = sqlx::query("DELETE FROM conversations WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Database(format!("Failed to delete conversation: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Conversation not found: {}",
                id
            )));
        }

        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    async fn add_message(
        &self,
        conversation_id: &str,
        role: MessageRole,
        content: &str,
        tokens: i64,
        metadata: Option<&str>,
    ) -> Result<ConversationMessage> {
        let message_id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let role_str = role.to_string();

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;

        // Insert message with status defaulting to 'completed'
        let status = "completed";
        sqlx::query!(
            r#"
            INSERT INTO conversation_messages (id, conversation_id, role, content, tokens, created_at, metadata, status)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            message_id,
            conversation_id,
            role_str,
            content,
            tokens,
            now,
            metadata,
            status
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to insert message: {}", e)))?;

        // Update conversation counts
        sqlx::query!(
            r#"
            UPDATE conversations
            SET message_count = message_count + 1,
                total_tokens = total_tokens + ?,
                updated_at = ?
            WHERE id = ?
            "#,
            tokens,
            now,
            conversation_id
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to update conversation counts: {}", e)))?;

        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit transaction: {}", e)))?;

        Ok(ConversationMessage {
            id: message_id,
            conversation_id: ConversationId::from_str(conversation_id)?,
            role,
            content: content.to_string(),
            tokens,
            created_at: Utc::now(),
            metadata: metadata.map(|s| s.to_string()),
            status: status.to_string(),
        })
    }

    /// Add a message with a specific status (for two-phase commit).
    ///
    /// This method supports the two-phase commit pattern for chat operations:
    /// 1. Create message with 'pending' status
    /// 2. Attempt operation (e.g., LLM generation)
    /// 3. Update status to 'completed' or 'failed' based on outcome
    ///
    /// # Arguments
    /// * `conversation_id` - Conversation ID
    /// * `role` - Message role (User, Assistant, System)
    /// * `content` - Message content
    /// * `tokens` - Token count
    /// * `metadata` - Optional JSON metadata
    /// * `status` - Message status ('pending', 'completed', 'failed')
    ///
    /// # Returns
    /// Created message entity with specified status
    async fn add_message_with_status(
        &self,
        conversation_id: &str,
        role: MessageRole,
        content: &str,
        tokens: i64,
        metadata: Option<&str>,
        status: &str,
    ) -> Result<ConversationMessage> {
        let message_id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let role_str = role.to_string();

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin transaction: {}", e)))?;

        // Insert message with specified status
        sqlx::query!(
            r#"
            INSERT INTO conversation_messages (id, conversation_id, role, content, tokens, created_at, metadata, status)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            message_id,
            conversation_id,
            role_str,
            content,
            tokens,
            now,
            metadata,
            status
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to insert message: {}", e)))?;

        // Update conversation counts
        sqlx::query!(
            r#"
            UPDATE conversations
            SET message_count = message_count + 1,
                total_tokens = total_tokens + ?,
                updated_at = ?
            WHERE id = ?
            "#,
            tokens,
            now,
            conversation_id
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to update conversation counts: {}", e)))?;

        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit transaction: {}", e)))?;

        Ok(ConversationMessage {
            id: message_id,
            conversation_id: ConversationId::from_str(conversation_id)?,
            role,
            content: content.to_string(),
            tokens,
            created_at: Utc::now(),
            metadata: metadata.map(|s| s.to_string()),
            status: status.to_string(),
        })
    }

    /// Update the status of a message.
    ///
    /// Used in two-phase commit to mark messages as 'completed' or 'failed'
    /// after the operation completes.
    ///
    /// # Arguments
    /// * `message_id` - ID of the message to update
    /// * `status` - New status ('pending', 'completed', 'failed')
    ///
    /// # Errors
    /// - `AppError::NotFound` if message doesn't exist
    /// - `AppError::Database` if update fails
    async fn update_message_status(&self, message_id: &str, status: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();

        let result = sqlx::query!(
            r#"
            UPDATE conversation_messages
            SET status = ?
            WHERE id = ?
            "#,
            status,
            message_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to update message status: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Message not found: {}",
                message_id
            )));
        }

        // Also update conversation's updated_at timestamp
        // Get conversation_id from message first
        let conversation_id: Option<String> =
            sqlx::query_scalar("SELECT conversation_id FROM conversation_messages WHERE id = ?")
                .bind(message_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| AppError::Database(format!("Failed to get conversation_id: {}", e)))?;

        if let Some(conv_id) = conversation_id {
            sqlx::query!(
                "UPDATE conversations SET updated_at = ? WHERE id = ?",
                now,
                conv_id
            )
            .execute(&self.pool)
            .await
            .map_err(|e| {
                AppError::Database(format!("Failed to update conversation timestamp: {}", e))
            })?;
        }

        Ok(())
    }

    async fn get_messages(&self, conversation_id: &str) -> Result<Vec<ConversationMessage>> {
        let db_models = sqlx::query_as::<_, ConversationMessageModel>(
            r#"
            SELECT id, conversation_id, role, content, tokens, created_at,
                   metadata, status
            FROM conversation_messages
            WHERE conversation_id = ?
            ORDER BY created_at ASC
            "#,
        )
        .bind(conversation_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to get messages: {}", e)))?;

        Ok(ConversationMessageMapper::to_entities(&db_models))
    }

    async fn add_document_reference(
        &self,
        conversation_id: &str,
        document_id: &str,
        chunk_id: Option<&str>,
        relevance_score: Option<f32>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            INSERT OR IGNORE INTO conversation_documents (conversation_id, document_id, chunk_id, relevance_score, added_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(conversation_id)
        .bind(document_id)
        .bind(chunk_id)
        .bind(relevance_score)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to add document reference: {}", e)))?;

        sqlx::query(
            r#"
            INSERT OR IGNORE INTO document_space_memberships (document_id, space_id, created_at)
            SELECT ?, c.space_id, ?
            FROM conversations c
            WHERE c.id = ?
              AND c.space_id IS NOT NULL
              AND TRIM(c.space_id) <> ''
            "#,
        )
        .bind(document_id)
        .bind(&now)
        .bind(conversation_id)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to add document-space membership for reference: {}",
                e
            ))
        })?;

        Ok(())
    }

    async fn get_document_references(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<DocumentReference>> {
        let db_models = sqlx::query_as::<_, DocumentReferenceModel>(
            r#"
            SELECT document_id, chunk_id, relevance_score, added_at
            FROM conversation_documents
            WHERE conversation_id = ?
            ORDER BY added_at ASC
            "#,
        )
        .bind(conversation_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to get document references: {}", e)))?;

        Ok(DocumentReferenceMapper::to_entities(&db_models))
    }

    async fn save_aggregate(&self, aggregate: &ConversationAggregate) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conversation = aggregate.conversation();

        // Store value to avoid E0716 temporary value dropped error
        let id_str = conversation.id.to_string();

        let result = sqlx::query!(
            r#"
            UPDATE conversations
            SET title = ?,
                system_prompt = ?,
                message_count = ?,
                total_tokens = ?,
                updated_at = ?
            WHERE id = ?
            "#,
            conversation.title,
            conversation.system_prompt,
            conversation.message_count,
            conversation.total_tokens,
            now,
            id_str
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to save aggregate: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Conversation not found: {}",
                conversation.id
            )));
        }

        Ok(())
    }

    async fn count(&self) -> Result<usize> {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM conversations")
            .fetch_one(&self.pool)
            .await
            .map(|c| c as usize)
            .map_err(|e| AppError::Database(format!("Failed to count conversations: {}", e)))
    }

    async fn exists(&self, id: &str) -> Result<bool> {
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM conversations WHERE id = ?)")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                AppError::Database(format!("Failed to check conversation existence: {}", e))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn create_test_pool() -> SqlitePool {
        SqlitePoolOptions::new().connect(":memory:").await.unwrap()
    }

    async fn setup_schema(pool: &SqlitePool) {
        sqlx::query(
            r#"
            CREATE TABLE conversations (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                model_name TEXT NOT NULL,
                system_prompt TEXT,
                space_id TEXT NOT NULL DEFAULT 'space_general',
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
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
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                metadata TEXT,
                FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            );

            CREATE TABLE conversation_documents (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                conversation_id TEXT NOT NULL,
                document_id TEXT NOT NULL,
                chunk_id TEXT,
                relevance_score REAL,
                added_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
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
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_create_conversation() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = ConversationRepository::new(pool);

        let conversation = repo
            .create_conversation(
                "Test Chat",
                "claude-sonnet-4-5-20250929",
                Some("Be helpful"),
            )
            .await
            .unwrap();

        assert_eq!(conversation.title, "Test Chat");
        assert_eq!(conversation.model_name, "claude-sonnet-4-5-20250929");
        assert_eq!(conversation.system_prompt, Some("Be helpful".to_string()));
        assert_eq!(conversation.message_count, 0);
        assert_eq!(conversation.total_tokens, 0);
    }

    #[tokio::test]
    async fn test_find_by_id() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = ConversationRepository::new(pool);

        let created = repo
            .create_conversation("Test", "model", None)
            .await
            .unwrap();

        let found = repo
            .find_by_id(&created.id.to_string())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(found.id, created.id);
        assert_eq!(found.title, "Test");
    }

    #[ignore] // TODO: Fix in Quality Phase - Oracle Phase 3 quarantine
    #[tokio::test]

    async fn test_add_message() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = ConversationRepository::new(pool);

        let conversation = repo
            .create_conversation("Test", "model", None)
            .await
            .unwrap();

        let message = repo
            .add_message(
                &conversation.id.to_string(),
                MessageRole::User,
                "Hello!",
                10,
                None,
            )
            .await
            .unwrap();

        assert_eq!(message.content, "Hello!");
        assert_eq!(message.tokens, 10);

        // Verify conversation was updated
        let updated = repo
            .find_by_id(&conversation.id.to_string())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(updated.message_count, 1);
        assert_eq!(updated.total_tokens, 10);
    }

    #[ignore] // TODO: Fix in Quality Phase - Oracle Phase 3 quarantine
    #[tokio::test]

    async fn test_get_messages() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = ConversationRepository::new(pool);

        let conversation = repo
            .create_conversation("Test", "model", None)
            .await
            .unwrap();

        repo.add_message(
            &conversation.id.to_string(),
            MessageRole::User,
            "First",
            5,
            None,
        )
        .await
        .unwrap();

        repo.add_message(
            &conversation.id.to_string(),
            MessageRole::Assistant,
            "Second",
            10,
            None,
        )
        .await
        .unwrap();

        let messages = repo
            .get_messages(&conversation.id.to_string())
            .await
            .unwrap();

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "First");
        assert_eq!(messages[1].content, "Second");
    }

    #[ignore] // TODO: Fix in Quality Phase - Oracle Phase 3 quarantine
    #[tokio::test]

    async fn test_find_aggregate_by_id() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = ConversationRepository::new(pool);

        let conversation = repo
            .create_conversation("Test", "model", None)
            .await
            .unwrap();

        repo.add_message(
            &conversation.id.to_string(),
            MessageRole::User,
            "Hello",
            5,
            None,
        )
        .await
        .unwrap();

        let aggregate = repo
            .find_aggregate_by_id(&conversation.id.to_string())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(aggregate.title(), "Test");
        assert_eq!(aggregate.messages().len(), 1);
        assert_eq!(aggregate.messages()[0].content, "Hello");
    }

    #[tokio::test]
    async fn test_update_title() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = ConversationRepository::new(pool);

        let conversation = repo
            .create_conversation("Old Title", "model", None)
            .await
            .unwrap();

        repo.update_title(&conversation.id.to_string(), "New Title")
            .await
            .unwrap();

        let updated = repo
            .find_by_id(&conversation.id.to_string())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(updated.title, "New Title");
    }

    #[tokio::test]
    async fn test_delete_conversation() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = ConversationRepository::new(pool);

        let conversation = repo
            .create_conversation("Test", "model", None)
            .await
            .unwrap();

        repo.delete(&conversation.id.to_string()).await.unwrap();

        let found = repo.find_by_id(&conversation.id.to_string()).await.unwrap();

        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_delete_conversation_preserves_space_memberships() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = ConversationRepository::new(pool.clone());

        let conversation = repo
            .create_conversation("Test", "model", None)
            .await
            .unwrap();

        repo.add_document_reference(&conversation.id.to_string(), "doc-1", None, Some(0.9))
            .await
            .unwrap();

        let before_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM document_space_memberships WHERE document_id = 'doc-1' AND space_id = 'space_general'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(before_count, 1);

        repo.delete(&conversation.id.to_string()).await.unwrap();

        let after_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM document_space_memberships WHERE document_id = 'doc-1' AND space_id = 'space_general'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(after_count, 1);
    }

    #[tokio::test]
    async fn test_delete_conversation_keeps_membership_when_other_space_conversation_uses_doc() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = ConversationRepository::new(pool.clone());

        let conversation_a = repo
            .create_conversation("Conversation A", "model", None)
            .await
            .unwrap();
        let conversation_b = repo
            .create_conversation("Conversation B", "model", None)
            .await
            .unwrap();

        repo.add_document_reference(
            &conversation_a.id.to_string(),
            "doc-shared",
            None,
            Some(0.7),
        )
        .await
        .unwrap();
        repo.add_document_reference(
            &conversation_b.id.to_string(),
            "doc-shared",
            None,
            Some(0.8),
        )
        .await
        .unwrap();

        repo.delete(&conversation_a.id.to_string()).await.unwrap();

        let membership_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM document_space_memberships WHERE document_id = 'doc-shared' AND space_id = 'space_general'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(membership_count, 1);
    }

    #[tokio::test]
    async fn test_find_all() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = ConversationRepository::new(pool);

        repo.create_conversation("Conv 1", "model", None)
            .await
            .unwrap();
        repo.create_conversation("Conv 2", "model", None)
            .await
            .unwrap();
        repo.create_conversation("Conv 3", "model", None)
            .await
            .unwrap();

        let all = repo.find_all(None, None).await.unwrap();
        assert_eq!(all.len(), 3);

        let limited = repo.find_all(Some(2), None).await.unwrap();
        assert_eq!(limited.len(), 2);
    }

    #[tokio::test]
    async fn test_count_and_exists() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = ConversationRepository::new(pool);

        assert_eq!(repo.count().await.unwrap(), 0);

        let conversation = repo
            .create_conversation("Test", "model", None)
            .await
            .unwrap();

        assert_eq!(repo.count().await.unwrap(), 1);
        assert!(repo.exists(&conversation.id.to_string()).await.unwrap());
        assert!(!repo.exists("non-existent").await.unwrap());
    }

    #[tokio::test]
    async fn test_add_document_reference() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let repo = ConversationRepository::new(pool);

        let conversation = repo
            .create_conversation("Test", "model", None)
            .await
            .unwrap();

        repo.add_document_reference(
            &conversation.id.to_string(),
            "doc-123",
            Some("chunk-456"),
            Some(0.95),
        )
        .await
        .unwrap();

        let refs = repo
            .get_document_references(&conversation.id.to_string())
            .await
            .unwrap();

        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].document_id, "doc-123");
        assert_eq!(refs[0].chunk_id, Some("chunk-456".to_string()));
        assert_eq!(refs[0].relevance_score, Some(0.95));
    }
}
