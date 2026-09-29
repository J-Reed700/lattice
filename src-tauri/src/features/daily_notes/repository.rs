//! Persistence owner for `daily_notes_workspace`.

use crate::shared::error::{AppError, Result};
use sqlx::{FromRow, SqlitePool};

const NOTE_COLUMNS: &str = r#"
    id,
    title,
    journal_id,
    content,
    linked_document_ids,
    linked_conversation_ids,
    highlights_json,
    sticky_notes_json,
    conversation_snapshots_json,
    sources_json,
    created_at,
    updated_at
"#;

#[derive(Debug, Clone, FromRow)]
pub struct WorkspaceNoteRecord {
    pub id: String,
    pub title: String,
    /// Owning journal, or `None` for an unfiled page (quick capture, vault import).
    pub journal_id: Option<String>,
    pub content: String,
    pub linked_document_ids: String,
    pub linked_conversation_ids: String,
    pub highlights_json: String,
    pub sticky_notes_json: String,
    pub conversation_snapshots_json: String,
    pub sources_json: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct VaultNoteUpsert<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub content: &'a str,
    pub created_at: &'a str,
    pub updated_at: &'a str,
}

#[derive(Debug, Clone, FromRow)]
pub struct NoteTimestampRecord {
    pub id: String,
    pub updated_at: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct DailyNotesRepository {
    pool: SqlitePool,
}

impl DailyNotesRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn get(&self, note_id: &str) -> Result<WorkspaceNoteRecord> {
        let query = format!("SELECT {NOTE_COLUMNS} FROM daily_notes_workspace WHERE id = ?");
        sqlx::query_as::<_, WorkspaceNoteRecord>(&query)
            .bind(note_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| {
                AppError::Database(format!("Failed to fetch workspace note {note_id}: {error}"))
            })?
            .ok_or_else(|| AppError::NotFound(format!("Workspace note not found: {note_id}")))
    }

    /// Persist read-time source enrichment only if the row has not changed
    /// since it was read. Updating this one column preserves concurrent edits,
    /// while matching both the source JSON and row timestamp prevents a stale
    /// hydration from overwriting newer citation or note state.
    pub async fn update_sources_if_unchanged(
        &self,
        note_id: &str,
        expected_sources_json: &str,
        expected_updated_at: &str,
        sources_json: &str,
    ) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE daily_notes_workspace SET sources_json = ? WHERE id = ? AND sources_json = ? AND updated_at = ?",
        )
        .bind(sources_json)
        .bind(note_id)
        .bind(expected_sources_json)
        .bind(expected_updated_at)
        .execute(&self.pool)
        .await
        .map_err(|error| {
            AppError::Database(format!("Failed to persist hydrated note sources {note_id}: {error}"))
        })?;
        Ok(result.rows_affected() > 0)
    }

    /// Every page, whichever journal owns it. The reference inbox, weekly
    /// synthesis and vault sync all work across journals and want this.
    pub async fn list(&self) -> Result<Vec<WorkspaceNoteRecord>> {
        let query =
            format!("SELECT {NOTE_COLUMNS} FROM daily_notes_workspace ORDER BY updated_at DESC");
        sqlx::query_as::<_, WorkspaceNoteRecord>(&query)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| AppError::Database(format!("Failed to list workspace notes: {error}")))
    }

    /// The pages one journal owns. This is what a journal's sidebar lists:
    /// without the filter every journal shows every other journal's pages, and
    /// its own landing page among them.
    pub async fn list_for_journal(&self, journal_id: &str) -> Result<Vec<WorkspaceNoteRecord>> {
        let query = format!(
            "SELECT {NOTE_COLUMNS} FROM daily_notes_workspace \
             WHERE journal_id = ? ORDER BY updated_at DESC"
        );
        sqlx::query_as::<_, WorkspaceNoteRecord>(&query)
            .bind(journal_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| AppError::Database(format!("Failed to list journal pages: {error}")))
    }

    /// The page with this exact title inside one journal (or among the pages no
    /// journal owns), newest first when a title repeats.
    ///
    /// Quick capture asks for today's page by name rather than taking whatever
    /// was written to last, so a capture never lands on an unrelated page. The
    /// journal is part of the question: each journal has its own "today".
    pub async fn find_by_title(
        &self,
        title: &str,
        journal_id: Option<&str>,
    ) -> Result<Option<WorkspaceNoteRecord>> {
        // `IS` rather than `=`, so a bound NULL matches the unowned pages.
        let query = format!(
            "SELECT {NOTE_COLUMNS} FROM daily_notes_workspace \
             WHERE title = ? AND journal_id IS ? ORDER BY updated_at DESC LIMIT 1"
        );
        sqlx::query_as::<_, WorkspaceNoteRecord>(&query)
            .bind(title)
            .bind(journal_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| {
                AppError::Database(format!(
                    "Failed to fetch workspace note titled {title}: {error}"
                ))
            })
    }

    /// The journal a capture with no journal of its own belongs to: the one
    /// written to last, else the first in the sidebar. `None` only when no
    /// journal exists yet.
    ///
    /// The journal screen lists pages per journal and nothing else, so a page
    /// owned by no journal cannot be opened from anywhere. Captures used to be
    /// written to exactly such a page: the toast said "saved" and the text was
    /// unreachable.
    pub async fn capture_journal_id(&self) -> Result<Option<String>> {
        sqlx::query_scalar::<_, String>(
            "SELECT j.id FROM journals j \
             LEFT JOIN daily_notes_workspace n ON n.journal_id = j.id \
             WHERE j.is_archived = 0 \
             GROUP BY j.id \
             ORDER BY MAX(n.updated_at) DESC, j.sort_order, j.created_at, j.id \
             LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| AppError::Database(format!("Failed to choose a capture journal: {error}")))
    }

    pub async fn most_recent(&self) -> Result<Option<WorkspaceNoteRecord>> {
        let query = format!(
            "SELECT {NOTE_COLUMNS} FROM daily_notes_workspace ORDER BY updated_at DESC LIMIT 1"
        );
        sqlx::query_as::<_, WorkspaceNoteRecord>(&query)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| {
                AppError::Database(format!("Failed to fetch latest workspace note: {error}"))
            })
    }

    pub async fn list_in_date_range(
        &self,
        start_date: &str,
        end_date: &str,
    ) -> Result<Vec<WorkspaceNoteRecord>> {
        let query = format!(
            "SELECT {NOTE_COLUMNS} FROM daily_notes_workspace \
             WHERE date(updated_at) BETWEEN date(?) AND date(?) \
             ORDER BY updated_at DESC"
        );
        sqlx::query_as::<_, WorkspaceNoteRecord>(&query)
            .bind(start_date)
            .bind(end_date)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| AppError::Database(format!("Failed to fetch notes by range: {error}")))
    }

    pub async fn previous_before(&self, date: &str) -> Result<Option<WorkspaceNoteRecord>> {
        self.adjacent(date, "<", "DESC").await
    }

    pub async fn next_after(&self, date: &str) -> Result<Option<WorkspaceNoteRecord>> {
        self.adjacent(date, ">", "ASC").await
    }

    async fn adjacent(
        &self,
        date: &str,
        comparison: &'static str,
        direction: &'static str,
    ) -> Result<Option<WorkspaceNoteRecord>> {
        // Both fragments are closed internal enums expressed as literals by
        // the two private callers above; no user text enters the SQL string.
        let query = format!(
            "SELECT {NOTE_COLUMNS} FROM daily_notes_workspace \
             WHERE date(updated_at) {comparison} date(?) \
             ORDER BY updated_at {direction} LIMIT 1"
        );
        sqlx::query_as::<_, WorkspaceNoteRecord>(&query)
            .bind(date)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| {
                AppError::Database(format!("Failed to fetch adjacent workspace note: {error}"))
            })
    }

    pub async fn insert(&self, note: &WorkspaceNoteRecord) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO daily_notes_workspace (
                id, title, journal_id, content, linked_document_ids, linked_conversation_ids,
                highlights_json, sticky_notes_json, conversation_snapshots_json,
                sources_json, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&note.id)
        .bind(&note.title)
        .bind(&note.journal_id)
        .bind(&note.content)
        .bind(&note.linked_document_ids)
        .bind(&note.linked_conversation_ids)
        .bind(&note.highlights_json)
        .bind(&note.sticky_notes_json)
        .bind(&note.conversation_snapshots_json)
        .bind(&note.sources_json)
        .bind(&note.created_at)
        .bind(&note.updated_at)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(|error| AppError::Database(format!("Failed to create workspace note: {error}")))
    }

    pub async fn update(&self, note: &WorkspaceNoteRecord) -> Result<()> {
        let result = sqlx::query(
            r#"
            UPDATE daily_notes_workspace
            SET title = ?, content = ?, linked_document_ids = ?,
                linked_conversation_ids = ?, highlights_json = ?, sticky_notes_json = ?,
                conversation_snapshots_json = ?, sources_json = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&note.title)
        .bind(&note.content)
        .bind(&note.linked_document_ids)
        .bind(&note.linked_conversation_ids)
        .bind(&note.highlights_json)
        .bind(&note.sticky_notes_json)
        .bind(&note.conversation_snapshots_json)
        .bind(&note.sources_json)
        .bind(&note.updated_at)
        .bind(&note.id)
        .execute(&self.pool)
        .await
        .map_err(|error| {
            AppError::Database(format!(
                "Failed to update workspace note {}: {error}",
                note.id
            ))
        })?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Workspace note not found: {}",
                note.id
            )));
        }
        Ok(())
    }

    pub async fn update_content(
        &self,
        note_id: &str,
        content: &str,
        updated_at: &str,
    ) -> Result<()> {
        let result = sqlx::query(
            "UPDATE daily_notes_workspace SET content = ?, updated_at = ? WHERE id = ?",
        )
        .bind(content)
        .bind(updated_at)
        .bind(note_id)
        .execute(&self.pool)
        .await
        .map_err(|error| {
            AppError::Database(format!("Failed to update daily note content: {error}"))
        })?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Workspace note not found: {note_id}"
            )));
        }
        Ok(())
    }

    pub async fn delete(&self, note_id: &str) -> Result<bool> {
        sqlx::query("DELETE FROM daily_notes_workspace WHERE id = ?")
            .bind(note_id)
            .execute(&self.pool)
            .await
            .map(|result| result.rows_affected() > 0)
            .map_err(|error| {
                AppError::Database(format!(
                    "Failed to delete workspace note {note_id}: {error}"
                ))
            })
    }

    /// Delete only when no application write/delete is waiting to reach the
    /// vault. The predicate and delete share one SQLite statement, so an
    /// external filesystem event cannot pass a stale outbox preflight and
    /// erase a newer application edit.
    pub async fn delete_from_vault_if_unqueued(&self, note_id: &str) -> Result<bool> {
        let result = sqlx::query(
            "DELETE FROM daily_notes_workspace WHERE id = ? AND NOT EXISTS (SELECT 1 FROM vault_write_outbox WHERE note_id = ? AND mirror_complete = 0)",
        )
        .bind(note_id)
        .bind(note_id)
        .execute(&self.pool)
        .await
        .map_err(|error| AppError::Database(format!("Failed to import vault deletion {note_id}: {error}")))?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn upsert_from_vault(&self, note: VaultNoteUpsert<'_>) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO daily_notes_workspace (id, title, content, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                content = excluded.content,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(note.id)
        .bind(note.title)
        .bind(note.content)
        .bind(note.created_at)
        .bind(note.updated_at)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(|error| {
            AppError::Database(format!("Failed to import vault note {}: {error}", note.id))
        })
    }

    /// Import external markdown only while no application-owned mirror intent
    /// exists. The INSERT and conflict update are both guarded atomically.
    pub async fn upsert_from_vault_if_unqueued(&self, note: VaultNoteUpsert<'_>) -> Result<bool> {
        let result = sqlx::query(
            r#"
            INSERT INTO daily_notes_workspace (id, title, content, created_at, updated_at)
            SELECT ?, ?, ?, ?, ?
            WHERE NOT EXISTS (SELECT 1 FROM vault_write_outbox WHERE note_id = ? AND mirror_complete = 0)
            ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                content = excluded.content,
                updated_at = excluded.updated_at
            WHERE NOT EXISTS (SELECT 1 FROM vault_write_outbox WHERE note_id = ? AND mirror_complete = 0)
            "#,
        )
        .bind(note.id)
        .bind(note.title)
        .bind(note.content)
        .bind(note.created_at)
        .bind(note.updated_at)
        .bind(note.id)
        .bind(note.id)
        .execute(&self.pool)
        .await
        .map_err(|error| {
            AppError::Database(format!("Failed to import vault note {}: {error}", note.id))
        })?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn get_timestamp(&self, note_id: &str) -> Result<Option<NoteTimestampRecord>> {
        sqlx::query_as::<_, NoteTimestampRecord>(
            "SELECT id, updated_at, created_at FROM daily_notes_workspace WHERE id = ?",
        )
        .bind(note_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| {
            AppError::Database(format!(
                "Failed to fetch workspace note timestamp {note_id}: {error}"
            ))
        })
    }

    pub async fn list_timestamps(&self) -> Result<Vec<NoteTimestampRecord>> {
        sqlx::query_as::<_, NoteTimestampRecord>(
            "SELECT id, updated_at, created_at FROM daily_notes_workspace",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| {
            AppError::Database(format!("Failed to list workspace note timestamps: {error}"))
        })
    }
}

#[cfg(test)]
mod vault_race_tests {
    use super::*;

    #[tokio::test]
    async fn vault_import_and_delete_guards_are_atomic_with_outbox_state() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE daily_notes_workspace(id TEXT PRIMARY KEY, title TEXT, content TEXT, created_at TEXT, updated_at TEXT)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::raw_sql(include_str!(
            "../../../migrations/20260927000000_vault_write_outbox.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        let repository = DailyNotesRepository::new(pool.clone());
        let note = VaultNoteUpsert {
            id: "note",
            title: "external",
            content: "disk body",
            created_at: "2026-01-01T00:00:00Z",
            updated_at: "2026-01-01T00:00:00Z",
        };

        // A successful import is queued by the trigger; another import is
        // blocked until the file mirror itself is recorded complete.
        assert!(repository
            .upsert_from_vault_if_unqueued(note.clone())
            .await
            .unwrap());
        assert!(!repository
            .upsert_from_vault_if_unqueued(note.clone())
            .await
            .unwrap());
        sqlx::query("UPDATE vault_write_outbox SET mirror_complete = 1 WHERE note_id = 'note'")
            .execute(&pool)
            .await
            .unwrap();
        // Once the file mirror is durable, pending vector indexing must not
        // block a newer external edit from being imported.
        assert!(repository
            .upsert_from_vault_if_unqueued(note.clone())
            .await
            .unwrap());

        // Simulate a committed app edit winning immediately before the
        // watcher statement. The in-statement guard preserves it.
        sqlx::query("UPDATE daily_notes_workspace SET content = 'app body' WHERE id = 'note'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(!repository
            .upsert_from_vault_if_unqueued(note)
            .await
            .unwrap());
        let content: String =
            sqlx::query_scalar("SELECT content FROM daily_notes_workspace WHERE id = 'note'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(content, "app body");
        assert!(!repository
            .delete_from_vault_if_unqueued("note")
            .await
            .unwrap());

        sqlx::query("UPDATE vault_write_outbox SET mirror_complete = 1 WHERE note_id = 'note'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(repository
            .delete_from_vault_if_unqueued("note")
            .await
            .unwrap());
    }
}
