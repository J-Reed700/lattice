//! Conversation Repository Implementation
//!
//! SQLite-backed CRUD for conversations, messages, and document
//! references. All DB ops go through the mapper layer to keep domain
//! entities decoupled from database models.
//!
//! # Architecture note
//!
//! This used to implement a `ConversationRepositoryPort` trait. The
//! trait had one implementor (this struct) and zero `dyn` consumers —
//! a Java/C# 'Header Interface' anti-pattern in Rust. Methods are now
//! plain inherent methods. If polymorphism is ever needed, extract
//! the trait at that point.

use crate::domain::conversation::{
    Conversation, ConversationAggregate, ConversationMessage, DocumentReference, MessageRole,
};
use crate::domain_types::ConversationId;
use crate::infrastructure::persistence::mappers::{
    ConversationMessageMapper, ConversationMessageModel, ConversationModel, ConversationRowMapper,
    DocumentReferenceMapper, DocumentReferenceModel,
};
use crate::shared::error::{AppError, Result};
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

        let placeholders = vec!["?"; message_ids.len()].join(",");
        let sql = format!(
            "DELETE FROM conversation_messages WHERE id IN ({})",
            placeholders
        );

        let mut query = sqlx::query(&sql);
        for message_id in message_ids {
            query = query.bind(message_id);
        }

        let total_deleted = query
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("Failed to delete messages: {}", e)))?
            .rows_affected();

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

    // ---------------------------------------------------------------
    // Methods below were previously the body of
    // `impl ConversationRepositoryPort for ConversationRepository`.
    // The port trait had no `dyn` consumers, so it was deleted and the
    // methods promoted to inherent methods directly on the repository.
    // ---------------------------------------------------------------

    pub async fn create_conversation(
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

    pub async fn find_by_id(&self, id: &str) -> Result<Option<Conversation>> {
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
            Some(model) => Ok(Some(ConversationRowMapper::to_entity(&model)?)),
            None => Ok(None),
        }
    }

    pub async fn find_aggregate_by_id(&self, id: &str) -> Result<Option<ConversationAggregate>> {
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

    pub async fn find_all(
        &self,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Conversation>> {
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

        Ok(ConversationRowMapper::to_entities(&db_models))
    }

    pub async fn update_title(&self, id: &str, new_title: &str) -> Result<()> {
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

    pub async fn update_system_prompt(&self, id: &str, system_prompt: Option<&str>) -> Result<()> {
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

    pub async fn delete(&self, id: &str) -> Result<()> {
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

    pub async fn add_message(
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
    pub async fn add_message_with_status(
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
    pub async fn update_message_status(&self, message_id: &str, status: &str) -> Result<()> {
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

    pub async fn get_messages(&self, conversation_id: &str) -> Result<Vec<ConversationMessage>> {
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

    pub async fn add_document_reference(
        &self,
        conversation_id: &str,
        document_id: &str,
        chunk_id: Option<&str>,
        relevance_score: Option<f32>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();

        // Both inserts must commit together. If the membership insert
        // fails after the conversation_documents insert, we'd otherwise
        // have a doc linked to the conversation that is invisible in
        // the conversation's space — breaks the invariant that linked
        // chat docs are also space members. Drop-on-error rolls back.
        let mut tx = self.pool.begin().await.map_err(|e| {
            AppError::Database(format!(
                "Failed to open tx for add_document_reference: {}",
                e
            ))
        })?;

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
        .execute(&mut *tx)
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
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to add document-space membership for reference: {}",
                e
            ))
        })?;

        tx.commit().await.map_err(|e| {
            AppError::Database(format!("Failed to commit add_document_reference: {}", e))
        })?;

        Ok(())
    }

    pub async fn get_document_references(
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

    pub async fn save_aggregate(&self, aggregate: &ConversationAggregate) -> Result<()> {
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

    pub async fn count(&self) -> Result<usize> {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM conversations")
            .fetch_one(&self.pool)
            .await
            .map(|c| c as usize)
            .map_err(|e| AppError::Database(format!("Failed to count conversations: {}", e)))
    }

    pub async fn exists(&self, id: &str) -> Result<bool> {
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM conversations WHERE id = ?)")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                AppError::Database(format!("Failed to check conversation existence: {}", e))
            })
    }

    // ---------------------------------------------------------------
    // Branching: truncate / take-last-turn / fork
    // (BRIEF rank 4, contract §4.2). No schema change — a branch is a
    // sibling conversation with copied messages, not a parent pointer.
    //
    // Every ordering here is `(created_at, rowid)`. `created_at` is an
    // RFC 3339 TEXT column and ids are random UUIDs, so two messages
    // written inside the same second would otherwise order arbitrarily
    // and a truncate could delete the wrong turn.
    // ---------------------------------------------------------------

    /// Delete every message created after `message_id` in `conversation_id`,
    /// and `message_id` itself when `inclusive`.
    ///
    /// Also deletes `conversation_message_bookmarks` rows for the removed
    /// messages in the same transaction: a bookmark that outlived its message
    /// is a reference the ReferenceInbox cannot open.
    ///
    /// Returns the number of messages deleted.
    pub async fn truncate_after(
        &self,
        conversation_id: &str,
        message_id: &str,
        inclusive: bool,
    ) -> Result<u64> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin truncate: {}", e)))?;

        let anchor = Self::load_message_position(&mut tx, conversation_id, message_id).await?;
        let anchor = match anchor {
            Some(anchor) => anchor,
            None => {
                return Err(AppError::NotFound(format!(
                    "Message {} is not in conversation {}",
                    message_id, conversation_id
                )))
            }
        };

        let mut ids =
            Self::load_message_ids_after(&mut tx, conversation_id, &anchor).await?;
        if inclusive {
            ids.push(message_id.to_string());
        }

        let deleted = Self::delete_message_ids(&mut tx, conversation_id, &ids).await?;

        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit truncate: {}", e)))?;

        Ok(deleted)
    }

    /// Delete the trailing assistant/system messages that follow the last user
    /// message, plus that user message, and return its content and token count.
    ///
    /// This is what makes regenerate not duplicate the question: the chat flow
    /// re-persists the returned content through its normal pending → completed
    /// path, so the thread ends up with exactly one copy of the user turn.
    ///
    /// Returns `None` when the conversation has no user message; nothing is
    /// deleted in that case.
    pub async fn take_last_user_turn(
        &self,
        conversation_id: &str,
    ) -> Result<Option<(String, i64)>> {
        let mut tx = self.pool.begin().await.map_err(|e| {
            AppError::Database(format!("Failed to begin take_last_user_turn: {}", e))
        })?;

        #[derive(sqlx::FromRow)]
        struct LastUserRow {
            id: String,
            content: String,
            tokens: i64,
            created_at: String,
            rowid: i64,
        }

        let row = sqlx::query_as::<_, LastUserRow>(
            r#"
            SELECT id, content, tokens, created_at, rowid
            FROM conversation_messages
            WHERE conversation_id = ? AND role = 'user'
            ORDER BY created_at DESC, rowid DESC
            LIMIT 1
            "#,
        )
        .bind(conversation_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to load last user message: {}", e)))?;

        let row = match row {
            Some(row) => row,
            None => return Ok(None),
        };

        let anchor = MessagePosition {
            created_at: row.created_at.clone(),
            rowid: row.rowid,
        };
        let mut ids = Self::load_message_ids_after(&mut tx, conversation_id, &anchor).await?;
        ids.push(row.id.clone());

        Self::delete_message_ids(&mut tx, conversation_id, &ids).await?;

        tx.commit().await.map_err(|e| {
            AppError::Database(format!("Failed to commit take_last_user_turn: {}", e))
        })?;

        Ok(Some((row.content, row.tokens)))
    }

    /// Create a sibling conversation in the same space and copy messages up to
    /// and including `up_to_message_id` (all when `None`), preserving role,
    /// content, tokens, metadata, status and relative ordering with fresh ids.
    /// Also copies `conversation_documents` and `conversation_web_sources`.
    ///
    /// Deliberately does **not** copy `conversation_summaries` (it points at
    /// message ids that mean something else in the new thread) or
    /// `conversation_memory_vectors` (keyed `UNIQUE(message_id)`, regenerated
    /// on the next turn).
    ///
    /// An `up_to_message_id` that is not in the conversation copies **zero**
    /// messages — an unknown anchor must never be read as "copy everything".
    ///
    /// Returns `(new conversation id, copied message count)`.
    pub async fn fork(
        &self,
        conversation_id: &str,
        up_to_message_id: Option<&str>,
        new_id: &str,
        new_title: &str,
    ) -> Result<(String, u32)> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin fork: {}", e)))?;

        #[derive(sqlx::FromRow)]
        struct SourceConversationRow {
            model_name: String,
            system_prompt: Option<String>,
            space_id: String,
        }

        let source = sqlx::query_as::<_, SourceConversationRow>(
            r#"
            SELECT model_name, system_prompt, space_id
            FROM conversations
            WHERE id = ?
            "#,
        )
        .bind(conversation_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to load conversation to fork: {}", e)))?
        .ok_or_else(|| AppError::NotFound(format!("Conversation {} not found", conversation_id)))?;

        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO conversations
                (id, title, model_name, system_prompt, space_id,
                 created_at, updated_at, message_count, total_tokens)
            VALUES (?, ?, ?, ?, ?, ?, ?, 0, 0)
            "#,
        )
        .bind(new_id)
        .bind(new_title)
        .bind(&source.model_name)
        .bind(&source.system_prompt)
        .bind(&source.space_id)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to create branch conversation: {}", e)))?;

        #[derive(sqlx::FromRow)]
        struct CopyRow {
            role: String,
            content: String,
            tokens: i64,
            created_at: String,
            metadata: Option<String>,
            status: String,
        }

        let rows: Vec<CopyRow> = match up_to_message_id {
            Some(anchor_id) => {
                match Self::load_message_position(&mut tx, conversation_id, anchor_id).await? {
                    Some(anchor) => sqlx::query_as::<_, CopyRow>(
                        r#"
                        SELECT role, content, tokens, created_at, metadata, status
                        FROM conversation_messages
                        WHERE conversation_id = ?
                          AND (created_at < ? OR (created_at = ? AND rowid <= ?))
                        ORDER BY created_at ASC, rowid ASC
                        "#,
                    )
                    .bind(conversation_id)
                    .bind(&anchor.created_at)
                    .bind(&anchor.created_at)
                    .bind(anchor.rowid)
                    .fetch_all(&mut *tx)
                    .await
                    .map_err(|e| {
                        AppError::Database(format!("Failed to read messages to copy: {}", e))
                    })?,
                    // Unknown anchor: copy nothing rather than everything.
                    None => Vec::new(),
                }
            }
            None => sqlx::query_as::<_, CopyRow>(
                r#"
                SELECT role, content, tokens, created_at, metadata, status
                FROM conversation_messages
                WHERE conversation_id = ?
                ORDER BY created_at ASC, rowid ASC
                "#,
            )
            .bind(conversation_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| AppError::Database(format!("Failed to read messages to copy: {}", e)))?,
        };

        let mut copied: u32 = 0;
        let mut total_tokens: i64 = 0;
        for row in &rows {
            let message_id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                r#"
                INSERT INTO conversation_messages
                    (id, conversation_id, role, content, tokens, created_at, metadata, status)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&message_id)
            .bind(new_id)
            .bind(&row.role)
            .bind(&row.content)
            .bind(row.tokens)
            .bind(&row.created_at)
            .bind(&row.metadata)
            .bind(&row.status)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Database(format!("Failed to copy message: {}", e)))?;
            copied += 1;
            total_tokens += row.tokens;
        }

        sqlx::query(
            r#"
            INSERT OR IGNORE INTO conversation_documents
                (conversation_id, document_id, chunk_id, relevance_score, added_at)
            SELECT ?, document_id, chunk_id, relevance_score, added_at
            FROM conversation_documents
            WHERE conversation_id = ?
            "#,
        )
        .bind(new_id)
        .bind(conversation_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to copy linked documents: {}", e)))?;

        #[derive(sqlx::FromRow)]
        struct WebSourceRow {
            url: String,
            normalized_url: String,
            title: Option<String>,
            excerpt: Option<String>,
            relevance_score: Option<f64>,
            added_at: String,
        }

        let web_sources = sqlx::query_as::<_, WebSourceRow>(
            r#"
            SELECT url, normalized_url, title, excerpt, relevance_score, added_at
            FROM conversation_web_sources
            WHERE conversation_id = ?
            "#,
        )
        .bind(conversation_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to read web sources to copy: {}", e)))?;

        for source in &web_sources {
            sqlx::query(
                r#"
                INSERT OR IGNORE INTO conversation_web_sources
                    (id, conversation_id, url, normalized_url, title, excerpt,
                     relevance_score, added_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(new_id)
            .bind(&source.url)
            .bind(&source.normalized_url)
            .bind(&source.title)
            .bind(&source.excerpt)
            .bind(source.relevance_score)
            .bind(&source.added_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Database(format!("Failed to copy web source: {}", e)))?;
        }

        sqlx::query(
            r#"
            UPDATE conversations
            SET message_count = ?, total_tokens = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(copied as i64)
        .bind(total_tokens)
        .bind(&now)
        .bind(new_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to set branch counts: {}", e)))?;

        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit fork: {}", e)))?;

        Ok((new_id.to_string(), copied))
    }

    /// `(created_at, rowid)` of one message, scoped to its conversation.
    async fn load_message_position(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        conversation_id: &str,
        message_id: &str,
    ) -> Result<Option<MessagePosition>> {
        sqlx::query_as::<_, MessagePosition>(
            r#"
            SELECT created_at, rowid
            FROM conversation_messages
            WHERE id = ? AND conversation_id = ?
            "#,
        )
        .bind(message_id)
        .bind(conversation_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to locate message: {}", e)))
    }

    /// Ids of every message strictly after `anchor`, oldest first.
    async fn load_message_ids_after(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        conversation_id: &str,
        anchor: &MessagePosition,
    ) -> Result<Vec<String>> {
        sqlx::query_scalar::<_, String>(
            r#"
            SELECT id
            FROM conversation_messages
            WHERE conversation_id = ?
              AND (created_at > ? OR (created_at = ? AND rowid > ?))
            ORDER BY created_at ASC, rowid ASC
            "#,
        )
        .bind(conversation_id)
        .bind(&anchor.created_at)
        .bind(&anchor.created_at)
        .bind(anchor.rowid)
        .fetch_all(&mut **tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to list messages to remove: {}", e)))
    }

    /// Delete messages (and their bookmarks) by id, then recount the
    /// conversation. Runs inside the caller's transaction so a partial
    /// truncate can never be observed.
    async fn delete_message_ids(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        conversation_id: &str,
        message_ids: &[String],
    ) -> Result<u64> {
        if message_ids.is_empty() {
            return Ok(0);
        }

        let placeholders = vec!["?"; message_ids.len()].join(",");

        // Bookmarks first: `conversation_message_bookmarks` only cascades when
        // SQLite foreign keys are enabled, and a dangling bookmark is worse
        // than a redundant delete.
        let bookmark_sql = format!(
            "DELETE FROM conversation_message_bookmarks WHERE message_id IN ({})",
            placeholders
        );
        let mut bookmark_query = sqlx::query(&bookmark_sql);
        for id in message_ids {
            bookmark_query = bookmark_query.bind(id);
        }
        bookmark_query
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Database(format!("Failed to delete bookmarks: {}", e)))?;

        let sql = format!(
            "DELETE FROM conversation_messages WHERE conversation_id = ? AND id IN ({})",
            placeholders
        );
        let mut query = sqlx::query(&sql).bind(conversation_id);
        for id in message_ids {
            query = query.bind(id);
        }
        let deleted = query
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Database(format!("Failed to delete messages: {}", e)))?
            .rows_affected();

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
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to recount messages: {}", e)))?;

        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            UPDATE conversations
            SET message_count = ?, total_tokens = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(stats.count)
        .bind(stats.total)
        .bind(&now)
        .bind(conversation_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to update conversation stats: {}", e)))?;

        Ok(deleted)
    }
}

/// Position of a message in its conversation: `(created_at, rowid)`.
///
/// `created_at` alone is not a key — RFC 3339 TEXT at second resolution ties
/// constantly, and message ids are random UUIDs, so `rowid` is what makes the
/// order deterministic.
#[derive(Debug, Clone, sqlx::FromRow)]
struct MessagePosition {
    created_at: String,
    rowid: i64,
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
                status TEXT NOT NULL DEFAULT 'completed',
                FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            );

            CREATE TABLE conversation_message_bookmarks (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                message_id TEXT NOT NULL,
                title TEXT,
                note TEXT,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE (conversation_id, message_id)
            );

            CREATE TABLE conversation_web_sources (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                url TEXT NOT NULL,
                normalized_url TEXT NOT NULL,
                title TEXT,
                excerpt TEXT,
                relevance_score REAL,
                added_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE (conversation_id, normalized_url)
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

    // ---------------------------------------------------------------
    // Branching (BRIEF rank 4, contract §4.2)
    // ---------------------------------------------------------------

    /// Insert a message at a controlled `created_at` so ordering is testable.
    async fn seed_message(
        pool: &SqlitePool,
        conversation_id: &str,
        id: &str,
        role: &str,
        content: &str,
        tokens: i64,
        created_at: &str,
    ) {
        sqlx::query(
            r#"
            INSERT INTO conversation_messages
                (id, conversation_id, role, content, tokens, created_at, metadata, status)
            VALUES (?, ?, ?, ?, ?, ?, NULL, 'completed')
            "#,
        )
        .bind(id)
        .bind(conversation_id)
        .bind(role)
        .bind(content)
        .bind(tokens)
        .bind(created_at)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            r#"
            UPDATE conversations
            SET message_count = message_count + 1, total_tokens = total_tokens + ?
            WHERE id = ?
            "#,
        )
        .bind(tokens)
        .bind(conversation_id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn message_ids(pool: &SqlitePool, conversation_id: &str) -> Vec<String> {
        sqlx::query_scalar::<_, String>(
            "SELECT id FROM conversation_messages WHERE conversation_id = ? \
             ORDER BY created_at ASC, rowid ASC",
        )
        .bind(conversation_id)
        .fetch_all(pool)
        .await
        .unwrap()
    }

    async fn conversation_counts(pool: &SqlitePool, conversation_id: &str) -> (i64, i64) {
        sqlx::query_as::<_, (i64, i64)>(
            "SELECT message_count, total_tokens FROM conversations WHERE id = ?",
        )
        .bind(conversation_id)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// A four-message thread: user → assistant → user → assistant.
    async fn seed_thread(pool: &SqlitePool) -> String {
        sqlx::query(
            "INSERT INTO conversations (id, title, model_name, space_id, created_at, updated_at) \
             VALUES ('conv-1', 'Thread', 'model', 'space_general', '2026-09-01T10:00:00Z', '2026-09-01T10:00:00Z')",
        )
        .execute(pool)
        .await
        .unwrap();

        seed_message(pool, "conv-1", "m1", "user", "first question", 1, "2026-09-01T10:00:00Z").await;
        seed_message(pool, "conv-1", "m2", "assistant", "first answer", 2, "2026-09-01T10:00:01Z").await;
        seed_message(pool, "conv-1", "m3", "user", "second question", 4, "2026-09-01T10:00:02Z").await;
        seed_message(pool, "conv-1", "m4", "assistant", "second answer", 8, "2026-09-01T10:00:03Z").await;

        "conv-1".to_string()
    }

    #[tokio::test]
    async fn test_truncate_after_exclusive() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let conversation_id = seed_thread(&pool).await;
        let repo = ConversationRepository::new(pool.clone());

        let deleted = repo
            .truncate_after(&conversation_id, "m2", false)
            .await
            .unwrap();

        assert_eq!(deleted, 2);
        assert_eq!(message_ids(&pool, &conversation_id).await, vec!["m1", "m2"]);
        assert_eq!(conversation_counts(&pool, &conversation_id).await, (2, 3));
    }

    #[tokio::test]
    async fn test_truncate_after_inclusive() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let conversation_id = seed_thread(&pool).await;
        let repo = ConversationRepository::new(pool.clone());

        let deleted = repo
            .truncate_after(&conversation_id, "m2", true)
            .await
            .unwrap();

        assert_eq!(deleted, 3);
        assert_eq!(message_ids(&pool, &conversation_id).await, vec!["m1"]);
        assert_eq!(conversation_counts(&pool, &conversation_id).await, (1, 1));
    }

    #[tokio::test]
    async fn test_truncate_after_deletes_orphaned_bookmarks() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let conversation_id = seed_thread(&pool).await;

        for (bookmark_id, message_id) in [("b1", "m2"), ("b2", "m4")] {
            sqlx::query(
                "INSERT INTO conversation_message_bookmarks (id, conversation_id, message_id, created_at) \
                 VALUES (?, ?, ?, '2026-09-01T11:00:00Z')",
            )
            .bind(bookmark_id)
            .bind(&conversation_id)
            .bind(message_id)
            .execute(&pool)
            .await
            .unwrap();
        }

        let repo = ConversationRepository::new(pool.clone());
        repo.truncate_after(&conversation_id, "m2", false)
            .await
            .unwrap();

        let surviving = sqlx::query_scalar::<_, String>(
            "SELECT message_id FROM conversation_message_bookmarks",
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        assert_eq!(surviving, vec!["m2"], "only the surviving message keeps its bookmark");
    }

    #[tokio::test]
    async fn test_truncate_after_same_second_ordering() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;

        sqlx::query(
            "INSERT INTO conversations (id, title, model_name, space_id, created_at, updated_at) \
             VALUES ('conv-2', 'Tied', 'model', 'space_general', '2026-09-01T10:00:00Z', '2026-09-01T10:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();

        // All three share an identical timestamp; only rowid separates them.
        seed_message(&pool, "conv-2", "t1", "user", "a", 1, "2026-09-01T10:00:00Z").await;
        seed_message(&pool, "conv-2", "t2", "assistant", "b", 1, "2026-09-01T10:00:00Z").await;
        seed_message(&pool, "conv-2", "t3", "user", "c", 1, "2026-09-01T10:00:00Z").await;

        let repo = ConversationRepository::new(pool.clone());
        let deleted = repo.truncate_after("conv-2", "t1", false).await.unwrap();

        assert_eq!(deleted, 2);
        assert_eq!(message_ids(&pool, "conv-2").await, vec!["t1"]);
    }

    #[tokio::test]
    async fn test_take_last_user_turn_removes_trailing_assistants_only() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let conversation_id = seed_thread(&pool).await;
        let repo = ConversationRepository::new(pool.clone());

        let taken = repo
            .take_last_user_turn(&conversation_id)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(taken.0, "second question");
        assert_eq!(taken.1, 4);
        assert_eq!(
            message_ids(&pool, &conversation_id).await,
            vec!["m1", "m2"],
            "the earlier user→assistant prefix is untouched"
        );
        assert_eq!(conversation_counts(&pool, &conversation_id).await, (2, 3));
    }

    #[tokio::test]
    async fn test_take_last_user_turn_none_when_no_user_message() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;

        sqlx::query(
            "INSERT INTO conversations (id, title, model_name, space_id, created_at, updated_at) \
             VALUES ('conv-3', 'System only', 'model', 'space_general', '2026-09-01T10:00:00Z', '2026-09-01T10:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();
        seed_message(&pool, "conv-3", "s1", "system", "be helpful", 1, "2026-09-01T10:00:00Z").await;

        let repo = ConversationRepository::new(pool.clone());
        assert!(repo.take_last_user_turn("conv-3").await.unwrap().is_none());
        assert_eq!(message_ids(&pool, "conv-3").await, vec!["s1"]);
    }

    #[tokio::test]
    async fn test_fork_copies_messages_up_to() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let conversation_id = seed_thread(&pool).await;
        let repo = ConversationRepository::new(pool.clone());

        let (new_id, copied) = repo
            .fork(&conversation_id, Some("m2"), "conv-branch", "Thread · branch")
            .await
            .unwrap();

        assert_eq!(new_id, "conv-branch");
        assert_eq!(copied, 2);

        #[derive(sqlx::FromRow)]
        struct Row {
            id: String,
            role: String,
            content: String,
            tokens: i64,
            status: String,
        }

        let rows = sqlx::query_as::<_, Row>(
            "SELECT id, role, content, tokens, status FROM conversation_messages \
             WHERE conversation_id = ? ORDER BY created_at ASC, rowid ASC",
        )
        .bind("conv-branch")
        .fetch_all(&pool)
        .await
        .unwrap();

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].role, "user");
        assert_eq!(rows[0].content, "first question");
        assert_eq!(rows[0].tokens, 1);
        assert_eq!(rows[0].status, "completed");
        assert_eq!(rows[1].content, "first answer");
        assert!(rows.iter().all(|r| r.id != "m1" && r.id != "m2"), "copies get fresh ids");

        let (title, space_id) = sqlx::query_as::<_, (String, String)>(
            "SELECT title, space_id FROM conversations WHERE id = ?",
        )
        .bind("conv-branch")
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(title.ends_with("· branch"));
        assert_eq!(space_id, "space_general");
        assert_eq!(conversation_counts(&pool, "conv-branch").await, (2, 3));
    }

    #[tokio::test]
    async fn test_fork_copies_all_when_none() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let conversation_id = seed_thread(&pool).await;
        let repo = ConversationRepository::new(pool.clone());

        let (_, copied) = repo
            .fork(&conversation_id, None, "conv-branch-all", "Thread · branch")
            .await
            .unwrap();

        assert_eq!(copied, 4);
        assert_eq!(message_ids(&pool, "conv-branch-all").await.len(), 4);
    }

    #[tokio::test]
    async fn test_fork_unknown_anchor_copies_nothing() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let conversation_id = seed_thread(&pool).await;
        let repo = ConversationRepository::new(pool.clone());

        let (_, copied) = repo
            .fork(
                &conversation_id,
                Some("does-not-exist"),
                "conv-branch-empty",
                "Thread · branch",
            )
            .await
            .unwrap();

        assert_eq!(copied, 0, "an unknown anchor must not be read as 'copy everything'");
    }

    #[tokio::test]
    async fn test_fork_copies_linked_documents_and_web_sources() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let conversation_id = seed_thread(&pool).await;

        sqlx::query(
            "INSERT INTO conversation_documents (conversation_id, document_id, chunk_id, relevance_score, added_at) \
             VALUES (?, 'doc-1', 'chunk-1', 0.9, '2026-09-01T10:00:00Z')",
        )
        .bind(&conversation_id)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO conversation_web_sources (id, conversation_id, url, normalized_url, title, excerpt, relevance_score, added_at) \
             VALUES ('ws-1', ?, 'https://example.com/a', 'example.com/a', 'A', 'excerpt', 0.5, '2026-09-01T10:00:00Z')",
        )
        .bind(&conversation_id)
        .execute(&pool)
        .await
        .unwrap();

        let repo = ConversationRepository::new(pool.clone());
        repo.fork(&conversation_id, None, "conv-branch-links", "Thread · branch")
            .await
            .unwrap();

        let doc_ids = sqlx::query_scalar::<_, String>(
            "SELECT document_id FROM conversation_documents WHERE conversation_id = ?",
        )
        .bind("conv-branch-links")
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(doc_ids, vec!["doc-1"]);

        let web_ids = sqlx::query_scalar::<_, String>(
            "SELECT id FROM conversation_web_sources WHERE conversation_id = ?",
        )
        .bind("conv-branch-links")
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(web_ids.len(), 1);
        assert_ne!(web_ids[0], "ws-1", "the copy gets a fresh primary key");
    }

    #[tokio::test]
    async fn test_fork_does_not_touch_original() {
        let pool = create_test_pool().await;
        setup_schema(&pool).await;
        let conversation_id = seed_thread(&pool).await;
        let before = conversation_counts(&pool, &conversation_id).await;

        let repo = ConversationRepository::new(pool.clone());
        repo.fork(&conversation_id, Some("m2"), "conv-branch-2", "Thread · branch")
            .await
            .unwrap();

        assert_eq!(conversation_counts(&pool, &conversation_id).await, before);
        assert_eq!(
            message_ids(&pool, &conversation_id).await,
            vec!["m1", "m2", "m3", "m4"]
        );
    }
}
