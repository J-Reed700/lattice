//! Branching a conversation into a sibling thread.

use super::ConversationRepository;
use crate::shared::error::{AppError, Result};
use chrono::Utc;

impl ConversationRepository {
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
}
