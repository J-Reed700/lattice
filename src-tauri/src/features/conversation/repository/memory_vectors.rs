//! Persistence for derived vectors over conversation messages.
use super::ConversationRepository;
use crate::shared::error::{AppError, Result};

impl ConversationRepository {
    /// Store an embedding only while its source message still matches the
    /// snapshot that was embedded. The single `INSERT ... SELECT` makes the
    /// existence check and write atomic with conversation/message deletion.
    /// A deleted, moved, or edited source selects no row and returns `0`; real
    /// database failures remain errors for the caller to log.
    #[allow(clippy::too_many_arguments)]
    pub async fn persist_memory_vector(
        &self,
        conversation_id: &str,
        vector_id: &str,
        message_id: &str,
        role: &str,
        content: &str,
        embedding: Vec<u8>,
        dimension: i64,
        embedding_model: &str,
        created_at: &str,
    ) -> Result<u64> {
        let result = sqlx::query(
            r#"
            INSERT INTO conversation_memory_vectors (
                id, conversation_id, message_id, role, content,
                embedding, dimension, embedding_model, created_at
            )
            SELECT ?, c.id, m.id, ?, ?, ?, ?, ?, ?
            FROM conversations c
            JOIN conversation_messages m ON m.conversation_id = c.id
            WHERE c.id = ? AND m.id = ? AND m.role = ? AND m.content = ?
            ON CONFLICT(message_id) DO UPDATE SET
                role = excluded.role,
                content = excluded.content,
                embedding = excluded.embedding,
                dimension = excluded.dimension,
                embedding_model = excluded.embedding_model,
                created_at = excluded.created_at
            "#,
        )
        .bind(vector_id)
        .bind(role)
        .bind(content)
        .bind(embedding)
        .bind(dimension)
        .bind(embedding_model)
        .bind(created_at)
        .bind(conversation_id)
        .bind(message_id)
        .bind(role)
        .bind(content)
        .execute(&self.pool)
        .await
        .map_err(|error| {
            AppError::Database(format!(
                "Failed to persist conversation memory vector: {error}"
            ))
        })?;

        Ok(result.rows_affected())
    }
}
