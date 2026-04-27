//! Summary Repository
//!
//! Handles persistence and retrieval of conversation summaries.

use crate::domain::conversation_summary::ConversationSummary;
use crate::shared::error::{AppError, Result};
use sqlx::SqlitePool;

/// Repository for conversation summaries
pub struct SummaryRepository {
    pool: SqlitePool,
}

impl SummaryRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Save summary to database (upsert)
    pub async fn save(&self, summary: &ConversationSummary) -> Result<()> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO conversation_summaries (
                id, conversation_id, summary_text, up_to_message_id,
                original_message_count, original_tokens, summary_tokens,
                compression_ratio, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&summary.id)
        .bind(&summary.conversation_id)
        .bind(&summary.summary_text)
        .bind(&summary.up_to_message_id)
        .bind(summary.original_message_count as i64)
        .bind(summary.original_tokens as i64)
        .bind(summary.summary_tokens as i64)
        .bind(summary.compression_ratio as f64)
        .bind(summary.created_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to save summary: {}", e)))?;

        Ok(())
    }

    /// Load summary for conversation
    pub async fn load(&self, conversation_id: &str) -> Result<Option<ConversationSummary>> {
        #[derive(sqlx::FromRow)]
        struct SummaryRecord {
            id: String,
            conversation_id: String,
            summary_text: String,
            up_to_message_id: String,
            original_message_count: i64,
            original_tokens: i64,
            summary_tokens: i64,
            compression_ratio: f64,
            created_at: String,
        }

        let record: Option<SummaryRecord> = sqlx::query_as(
            r#"
            SELECT id, conversation_id, summary_text, up_to_message_id,
                   original_message_count, original_tokens, summary_tokens,
                   compression_ratio, created_at
            FROM conversation_summaries
            WHERE conversation_id = ?
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(conversation_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to load summary: {}", e)))?;

        match record {
            Some(r) => {
                let created_at = chrono::DateTime::parse_from_rfc3339(&r.created_at)
                    .map_err(|e| AppError::Other(format!("Invalid datetime: {}", e)))?
                    .with_timezone(&chrono::Utc);

                Ok(Some(ConversationSummary {
                    id: r.id,
                    conversation_id: r.conversation_id,
                    summary_text: r.summary_text,
                    up_to_message_id: r.up_to_message_id,
                    original_message_count: r.original_message_count as usize,
                    original_tokens: r.original_tokens as usize,
                    summary_tokens: r.summary_tokens as usize,
                    compression_ratio: r.compression_ratio as f32,
                    created_at,
                }))
            }
            None => Ok(None),
        }
    }

    /// Delete summary for conversation (cache invalidation)
    pub async fn delete(&self, conversation_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM conversation_summaries WHERE conversation_id = ?")
            .bind(conversation_id)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Database(format!("Failed to delete summary: {}", e)))?;

        Ok(())
    }

    /// Check if cached summary is valid for conversation
    pub async fn is_valid(&self, conversation_id: &str, last_message_id: &str) -> Result<bool> {
        let summary = self.load(conversation_id).await?;
        Ok(summary.is_some_and(|s| s.is_valid_for_messages(last_message_id)))
    }
}
