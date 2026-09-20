//! Conversation messages: append, read and two-phase-commit status changes.

use super::ConversationRepository;
use crate::domain::conversation::{ConversationMessage, MessageRole};
use crate::domain::conversation_memory::compute_digest;
use crate::features::conversation::persistence_mapper::{
    ConversationMessageMapper, ConversationMessageModel,
};
use crate::shared::domain_types::ConversationId;
use crate::shared::error::{AppError, Result};
use chrono::Utc;
use std::str::FromStr;

impl ConversationRepository {
    /// Take the next durable position in a conversation, inside the caller's
    /// transaction.
    ///
    /// Allocated from `conversations.next_message_sequence` rather than derived
    /// from `created_at` or `rowid`: RFC 3339 text ties constantly at second
    /// resolution, message ids are random UUIDs, and rowid does not survive a
    /// vacuum. Numbers are never reused after a delete, so an evidence span can
    /// never be silently re-pointed at a different message.
    ///
    /// Two statements rather than `RETURNING` so the behaviour does not depend
    /// on the bundled SQLite version; inside one transaction they are atomic.
    pub(super) async fn allocate_sequence(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        conversation_id: &str,
    ) -> Result<i64> {
        let updated = sqlx::query(
            "UPDATE conversations SET next_message_sequence = next_message_sequence + 1 WHERE id = ?",
        )
        .bind(conversation_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to allocate message sequence: {}", e)))?;
        if updated.rows_affected() != 1 {
            return Err(AppError::NotFound(format!(
                "Conversation not found: {}",
                conversation_id
            )));
        }
        let next: i64 =
            sqlx::query_scalar("SELECT next_message_sequence FROM conversations WHERE id = ?")
                .bind(conversation_id)
                .fetch_one(&mut **tx)
                .await
                .map_err(|e| {
                    AppError::Database(format!("Failed to read message sequence: {}", e))
                })?;
        Ok(next - 1)
    }
}

impl ConversationRepository {
    pub async fn complete_turn(
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
        let parsed_id = ConversationId::from_str(conversation_id)?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        let mut tx = self.pool.begin().await?;
        let updated = sqlx::query("UPDATE conversation_messages SET status='completed' WHERE id=? AND conversation_id=? AND role='user' AND status='pending'")
            .bind(user_message_id).bind(conversation_id).execute(&mut *tx).await?;
        if updated.rows_affected() != 1 {
            return Err(AppError::InvalidState(
                "Turn is no longer pending in this conversation".into(),
            ));
        }
        let sequence = Self::allocate_sequence(&mut tx, conversation_id).await?;
        sqlx::query("INSERT INTO conversation_messages (id, conversation_id, role, content, tokens, created_at, metadata, status, sequence, content_digest) VALUES (?, ?, 'assistant', ?, ?, ?, ?, 'completed', ?, ?)")
            .bind(&id).bind(conversation_id).bind(&content).bind(tokens).bind(now.to_rfc3339()).bind(&metadata)
            .bind(sequence).bind(compute_digest(&content))
            .execute(&mut *tx).await?;
        sqlx::query("UPDATE conversations SET message_count=message_count+1, total_tokens=total_tokens+?, updated_at=? WHERE id=?")
            .bind(tokens).bind(now.to_rfc3339()).bind(conversation_id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(ConversationMessage {
            id,
            conversation_id: parsed_id,
            role: MessageRole::Assistant,
            content,
            tokens,
            created_at: now,
            metadata,
            status: "completed".into(),
        })
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

        let status = "completed";
        let sequence = Self::allocate_sequence(&mut tx, conversation_id).await?;
        let digest = compute_digest(content);
        sqlx::query!(
            r#"
            INSERT INTO conversation_messages (id, conversation_id, role, content, tokens, created_at, metadata, status, sequence, content_digest)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            message_id,
            conversation_id,
            role_str,
            content,
            tokens,
            now,
            metadata,
            status,
            sequence,
            digest
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to insert message: {}", e)))?;

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

        let sequence = Self::allocate_sequence(&mut tx, conversation_id).await?;
        let digest = compute_digest(content);
        sqlx::query!(
            r#"
            INSERT INTO conversation_messages (id, conversation_id, role, content, tokens, created_at, metadata, status, sequence, content_digest)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            message_id,
            conversation_id,
            role_str,
            content,
            tokens,
            now,
            metadata,
            status,
            sequence,
            digest
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(format!("Failed to insert message: {}", e)))?;

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
            ORDER BY sequence ASC, created_at ASC, rowid ASC
            "#,
        )
        .bind(conversation_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to get messages: {}", e)))?;

        Ok(ConversationMessageMapper::to_entities(&db_models))
    }
}
