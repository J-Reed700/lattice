//! Read models for exporting conversations and journal pages.
//!
//! These read-only projections keep export SQL out of the use case. They are
//! not a second registry for conversations or notes.
//!
//! Runtime-checked `sqlx::query_as` rather than the `query!` macros, so the
//! offline `.sqlx` cache stays valid (see `features/corpus_shape/repository.rs`).

use lattice_core::shared::error::AppError;
use lattice_core::shared::persistence::Database;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportConversationRow {
    pub id: String,
    pub title: String,
    /// The chat's model choice in `ChatModelChoice`'s stored form.
    pub model_choice: Option<String>,
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
    /// Holds the turn record, whose `model` says which model answered.
    pub metadata: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportNoteRow {
    pub id: String,
    pub title: String,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A picture an exported answer shows, with what its source line needs.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExportImageRow {
    pub message_id: String,
    /// The `ref` column: `img-N` as the answer wrote it, or `att-N` for a
    /// picture the reader attached to their message.
    pub image_ref: String,
    pub id: String,
    pub ext: String,
    /// `None` for an attached picture.
    pub source_url: Option<String>,
    pub source_title: Option<String>,
    pub license: Option<String>,
    /// The attached file's name; `None` for a found picture.
    pub original_name: Option<String>,
}

/// Internal sentinel row holding standalone web snapshots; never exported as a chat.
const INTERNAL_LIBRARY_SNAPSHOTS_ID: &str = "__library_snapshots__";

pub struct ExportRepository {
    db: Database,
}

impl ExportRepository {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// All conversations, newest first.
    pub async fn list_conversations(&self) -> Result<Vec<ExportConversationRow>, AppError> {
        let mut read = self.db.begin_read().await?;
        let rows = sqlx::query_as::<_, ExportConversationRow>(
            r#"
            SELECT id, title, model_choice, created_at, updated_at
              FROM conversations
             WHERE id != ?
             ORDER BY updated_at DESC
            "#,
        )
        .bind(INTERNAL_LIBRARY_SNAPSHOTS_ID)
        .fetch_all(&mut *read)
        .await
        .map_err(|e| {
            AppError::Database(format!("Failed to read conversations for export: {}", e))
        })?;
        read.commit().await?;
        Ok(rows)
    }

    /// Every completed message across all conversations, ordered by conversation
    /// then time. One query, not N — a vault with 500 conversations must not fire
    /// 500 round trips.
    pub async fn list_messages(&self) -> Result<Vec<ExportMessageRow>, AppError> {
        let mut read = self.db.begin_read().await?;
        let rows = sqlx::query_as::<_, ExportMessageRow>(
            r#"
            SELECT id, conversation_id, role, content, created_at, metadata
              FROM conversation_messages
             WHERE status = 'completed'
             ORDER BY conversation_id, created_at ASC
            "#,
        )
        .fetch_all(&mut *read)
        .await
        .map_err(|e| AppError::Database(format!("Failed to read messages for export: {}", e)))?;
        read.commit().await?;
        Ok(rows)
    }

    /// Every picture an answer holds, in one query like the messages.
    pub async fn list_chat_images(&self) -> Result<Vec<ExportImageRow>, AppError> {
        let mut read = self.db.begin_read().await?;
        let rows = sqlx::query_as::<_, ExportImageRow>(
            r#"
            SELECT message_id, ref AS image_ref, id, ext, source_url, source_title, license,
                   original_name
              FROM chat_images
             WHERE message_id IS NOT NULL
            "#,
        )
        .fetch_all(&mut *read)
        .await
        .map_err(|e| AppError::Database(format!("Failed to read chat images for export: {}", e)))?;
        read.commit().await?;
        Ok(rows)
    }

    /// Journal pages (workspace notes), newest first.
    pub async fn list_journal_pages(&self) -> Result<Vec<ExportNoteRow>, AppError> {
        let mut read = self.db.begin_read().await?;
        let rows = sqlx::query_as::<_, ExportNoteRow>(
            r#"
            SELECT id, title, content, created_at, updated_at
              FROM daily_notes_workspace
             ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&mut *read)
        .await
        .map_err(|e| {
            AppError::Database(format!("Failed to read journal pages for export: {}", e))
        })?;
        read.commit().await?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn list_conversations_excludes_internal_library_snapshots() {
        let pool = lattice_core::shared::testing::sqlite::migrated_in_memory()
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO conversations (id, title, space_id, created_at, updated_at) VALUES \
             ('__library_snapshots__', 'Saved Web Snapshots', 'space_general', '2026-10-13T00:00:00Z', '2026-10-13T00:00:00Z'), \
             ('conv-real', 'Real Chat', 'space_general', '2026-10-13T00:00:00Z', '2026-10-13T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let repo = ExportRepository::new(Database::new(pool));
        let exported = repo.list_conversations().await.unwrap();
        assert_eq!(exported.len(), 1);
        assert_eq!(exported[0].id, "conv-real");
    }
}
