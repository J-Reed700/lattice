//! Removing messages: explicit pruning, truncation and taking back a turn.

use super::ConversationRepository;
use crate::shared::error::{AppError, Result};
use chrono::Utc;

impl ConversationRepository {
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

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(format!("Failed to begin pruning: {}", e)))?;
        let deleted = Self::delete_message_ids(&mut tx, conversation_id, message_ids).await?;
        tx.commit()
            .await
            .map_err(|e| AppError::Database(format!("Failed to commit pruning: {}", e)))?;
        Ok(deleted)
    }
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

        let mut ids = Self::load_message_ids_after(&mut tx, conversation_id, &anchor).await?;
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
    /// `(created_at, rowid)` of one message, scoped to its conversation.
    pub(super) async fn load_message_position(
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
            "DELETE FROM conversation_message_bookmarks WHERE conversation_id = ? AND message_id IN ({})",
            placeholders
        );
        let mut bookmark_query = sqlx::query(&bookmark_sql).bind(conversation_id);
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
pub(super) struct MessagePosition {
    pub(super) created_at: String,
    pub(super) rowid: i64,
}
