//! Read models for exporting conversations and journal pages.
//!
//! These read-only projections keep export SQL out of the use case. They are
//! not a second registry for conversations or notes.
//!
//! Runtime-checked `sqlx::query_as` rather than the `query!` macros, so the
//! offline `.sqlx` cache stays valid (see `features/corpus_shape/repository.rs`).

use sqlx::SqlitePool;

use crate::shared::error::AppError;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportConversationRow {
    pub id: String,
    pub title: String,
    pub model_name: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportMessageRow {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportNoteRow {
    pub id: String,
    pub title: String,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
}

pub struct ExportRepository {
    pool: SqlitePool,
}

impl ExportRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// All conversations, newest first.
    pub async fn list_conversations(&self) -> Result<Vec<ExportConversationRow>, AppError> {
        sqlx::query_as::<_, ExportConversationRow>(
            r#"
            SELECT id, title, model_name, created_at, updated_at
              FROM conversations
             ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to read conversations for export: {}", e)))
    }

    /// Every completed message across all conversations, ordered by conversation
    /// then time. One query, not N — a vault with 500 conversations must not fire
    /// 500 round trips.
    pub async fn list_messages(&self) -> Result<Vec<ExportMessageRow>, AppError> {
        sqlx::query_as::<_, ExportMessageRow>(
            r#"
            SELECT id, conversation_id, role, content, created_at
              FROM conversation_messages
             WHERE status = 'completed'
             ORDER BY conversation_id, created_at ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to read messages for export: {}", e)))
    }

    /// Journal pages (workspace notes), newest first.
    pub async fn list_journal_pages(&self) -> Result<Vec<ExportNoteRow>, AppError> {
        sqlx::query_as::<_, ExportNoteRow>(
            r#"
            SELECT id, title, content, created_at, updated_at
              FROM daily_notes_workspace
             ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("Failed to read journal pages for export: {}", e)))
    }
}
