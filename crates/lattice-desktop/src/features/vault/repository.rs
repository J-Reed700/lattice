//! Durable, coalesced vault write intentions. Bodies remain in the notes table.
use crate::infrastructure::persistence::repositories::workspace_notes::{
    timestamp_from_row, DailyNotesRepository,
};
use lattice_core::application::contracts::notes::{NoteTimestampRecord, VaultNoteUpsert};
use lattice_core::application::ports::WorkspaceNotesPort;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;

/// The notes table the vault mirrors, seen only through its port.
pub(super) fn workspace_notes(db: &Database) -> impl WorkspaceNotesPort {
    DailyNotesRepository::new(db.clone())
}

#[derive(sqlx::FromRow)]
pub(super) struct PendingWrite {
    pub note_id: String,
    pub revision: String,
    pub mirror_complete: bool,
    pub last_mirrored_sha256: Option<String>,
}

pub(super) async fn pending(db: &Database) -> Result<Vec<PendingWrite>> {
    let mut read = db.begin_read().await?;
    let writes = sqlx::query_as(
        "SELECT o.note_id, o.revision, o.mirror_complete, m.last_mirrored_sha256 \
         FROM vault_write_outbox o \
         LEFT JOIN vault_mirror_state m ON m.note_id = o.note_id \
         WHERE o.retry_at <= unixepoch() ORDER BY o.rowid LIMIT 64",
    )
    .fetch_all(&mut *read)
    .await?;
    read.commit().await?;
    Ok(writes)
}

pub(super) async fn acknowledge(db: &Database, write: &PendingWrite) -> Result<()> {
    let mut tx = db.begin_write().await?;
    sqlx::query("DELETE FROM vault_write_outbox WHERE note_id = ? AND revision = ?")
        .bind(&write.note_id)
        .bind(&write.revision)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub(super) async fn enqueue_all(db: &Database) -> Result<()> {
    let mut tx = db.begin_write().await?;
    sqlx::query("INSERT INTO vault_write_outbox(note_id, revision) SELECT id, lower(hex(randomblob(16))) FROM daily_notes_workspace WHERE true ON CONFLICT(note_id) DO UPDATE SET revision = excluded.revision, mirror_complete = 0, retry_at = 0")
        .execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

pub(super) async fn mark_mirrored(
    db: &Database,
    write: &PendingWrite,
    sha256_hex: Option<&str>,
) -> Result<()> {
    let mut tx = db.begin_write().await?;
    let result = sqlx::query(
        "UPDATE vault_write_outbox SET mirror_complete = 1 \
         WHERE note_id = ? AND revision = ?",
    )
    .bind(&write.note_id)
    .bind(&write.revision)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() > 0 {
        match sha256_hex {
            Some(sha) => {
                sqlx::query(
                    "INSERT INTO vault_mirror_state (note_id, last_mirrored_sha256) VALUES (?, ?) \
                     ON CONFLICT(note_id) DO UPDATE SET last_mirrored_sha256 = excluded.last_mirrored_sha256",
                )
                .bind(&write.note_id)
                .bind(sha)
                .execute(&mut *tx)
                .await?;
            }
            None => {
                sqlx::query("DELETE FROM vault_mirror_state WHERE note_id = ?")
                    .bind(&write.note_id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }
    tx.commit().await?;
    Ok(())
}

/// Import external markdown only while no application-owned mirror intent
/// exists, and clear the trigger-generated `mirror_complete = 0` flag inside
/// the same transaction so subsequent external edits are not dropped and the
/// writer indexes the imported note without echoing back to disk.
pub(super) async fn upsert_from_vault_if_unqueued(
    db: &Database,
    note: VaultNoteUpsert<'_>,
    sha256_hex: &str,
) -> Result<bool> {
    let mut tx = db.begin_write().await?;
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
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        AppError::Database(format!("Failed to import vault note {}: {error}", note.id))
    })?;
    let imported = result.rows_affected() > 0;
    if imported {
        sqlx::query(
            "UPDATE vault_write_outbox \
             SET mirror_complete = 1, retry_at = 0 \
             WHERE note_id = ?",
        )
        .bind(note.id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO vault_mirror_state (note_id, last_mirrored_sha256) VALUES (?, ?) \
             ON CONFLICT(note_id) DO UPDATE SET last_mirrored_sha256 = excluded.last_mirrored_sha256",
        )
        .bind(note.id)
        .bind(sha256_hex)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(imported)
}

/// Reconcile SQLite `daily_notes_workspace` to match an externally edited
/// markdown file preserved during a queued writeback conflict, and mark the
/// outbox entry mirrored while preserving `write.revision` so `acknowledge`
/// settles cleanly. Returns `true` when SQLite was updated.
pub(super) async fn reconcile_external_note_and_mark_mirrored(
    db: &Database,
    write: &PendingWrite,
    note: VaultNoteUpsert<'_>,
    sha256_hex: &str,
) -> Result<bool> {
    let mut tx = db.begin_write().await?;
    let result = sqlx::query(
        r#"
        INSERT INTO daily_notes_workspace (id, title, content, created_at, updated_at)
        SELECT ?, ?, ?, ?, ?
        WHERE EXISTS (SELECT 1 FROM vault_write_outbox WHERE note_id = ? AND revision = ?)
        ON CONFLICT(id) DO UPDATE SET
            title = excluded.title,
            content = excluded.content,
            updated_at = excluded.updated_at
        WHERE EXISTS (SELECT 1 FROM vault_write_outbox WHERE note_id = ? AND revision = ?)
        "#,
    )
    .bind(note.id)
    .bind(note.title)
    .bind(note.content)
    .bind(note.created_at)
    .bind(note.updated_at)
    .bind(note.id)
    .bind(&write.revision)
    .bind(note.id)
    .bind(&write.revision)
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        AppError::Database(format!(
            "Failed to reconcile vault note {}: {error}",
            note.id
        ))
    })?;
    let reconciled = result.rows_affected() > 0;
    if reconciled {
        sqlx::query(
            "UPDATE vault_write_outbox \
             SET mirror_complete = 1, revision = ?, retry_at = 0 \
             WHERE note_id = ?",
        )
        .bind(&write.revision)
        .bind(note.id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO vault_mirror_state (note_id, last_mirrored_sha256) VALUES (?, ?) \
             ON CONFLICT(note_id) DO UPDATE SET last_mirrored_sha256 = excluded.last_mirrored_sha256",
        )
        .bind(note.id)
        .bind(sha256_hex)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(reconciled)
}

pub(crate) async fn find_note_document_id(db: &Database, note_id: &str) -> Result<Option<String>> {
    let mut read = db.begin_read().await?;
    let doc_id = sqlx::query_scalar::<_, String>(
        "SELECT document_id FROM vault_note_documents WHERE note_id = ?",
    )
    .bind(note_id)
    .fetch_optional(&mut *read)
    .await?;
    read.commit().await?;
    Ok(doc_id)
}

pub(crate) async fn upsert_note_document_id(
    db: &Database,
    note_id: &str,
    document_id: &str,
) -> Result<()> {
    let mut tx = db.begin_write().await?;
    sqlx::query(
        "INSERT INTO vault_note_documents (note_id, document_id) VALUES (?, ?) \
         ON CONFLICT(note_id) DO UPDATE SET document_id = excluded.document_id",
    )
    .bind(note_id)
    .bind(document_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub(crate) async fn delete_note_document_mapping(db: &Database, note_id: &str) -> Result<()> {
    let mut tx = db.begin_write().await?;
    sqlx::query("DELETE FROM vault_note_documents WHERE note_id = ?")
        .bind(note_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub(super) async fn defer(db: &Database, write: &PendingWrite) -> Result<()> {
    let mut tx = db.begin_write().await?;
    sqlx::query("UPDATE vault_write_outbox SET retry_at = unixepoch() + 30 WHERE note_id = ? AND revision = ?")
        .bind(&write.note_id).bind(&write.revision).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

pub(super) async fn has_pending(db: &Database, id: &str) -> Result<bool> {
    let mut read = db.begin_read().await?;
    let pending = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM vault_write_outbox WHERE note_id = ? AND mirror_complete = 0)",
    )
    .bind(id)
    .fetch_one(&mut *read)
    .await?;
    read.commit().await?;
    Ok(pending)
}

pub(super) async fn begin_rescan(db: &Database) -> Result<(String, i64)> {
    // Rescans are serialized in the watcher. This clears spill records left
    // by a previous process before allocating a new isolated run id.
    let mut tx = db.begin_write().await?;
    sqlx::query("DELETE FROM vault_rescan_seen")
        .execute(&mut *tx)
        .await?;
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM daily_notes_workspace")
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((uuid::Uuid::new_v4().to_string(), total))
}

const RESCAN_SEEN_BATCH_SIZE: usize = 250;

pub(super) async fn mark_rescan_seen_batch(
    db: &Database,
    run_id: &str,
    note_ids: &[String],
) -> Result<()> {
    if note_ids.is_empty() {
        return Ok(());
    }
    for chunk in note_ids.chunks(RESCAN_SEEN_BATCH_SIZE) {
        let mut tx = db.begin_write().await?;
        for note_id in chunk {
            sqlx::query("INSERT OR IGNORE INTO vault_rescan_seen(run_id, note_id) VALUES (?, ?)")
                .bind(run_id)
                .bind(note_id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
    }
    Ok(())
}

pub(super) async fn rescan_note_page(
    db: &Database,
    run_id: &str,
    after_id: Option<&str>,
    limit: i64,
) -> Result<Vec<NoteTimestampRecord>> {
    let mut read = db.begin_read().await?;
    let page = sqlx::query(
        "SELECT n.id, n.updated_at, n.created_at FROM daily_notes_workspace n WHERE (? IS NULL OR n.id > ?) AND NOT EXISTS (SELECT 1 FROM vault_rescan_seen s WHERE s.run_id = ? AND s.note_id = n.id) ORDER BY n.id LIMIT ?",
    )
    .bind(after_id)
    .bind(after_id)
    .bind(run_id)
    .bind(limit)
    .try_map(timestamp_from_row)
    .fetch_all(&mut *read)
    .await?;
    read.commit().await?;
    Ok(page)
}

pub(super) async fn finish_rescan(db: &Database, run_id: &str) -> Result<()> {
    let mut tx = db.begin_write().await?;
    sqlx::query("DELETE FROM vault_rescan_seen WHERE run_id = ?")
        .bind(run_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    #[tokio::test]
    async fn outbox_replays_after_reopen_and_rolls_back_with_the_note() {
        use sqlx::sqlite::SqlitePoolOptions;
        let directory = tempfile::tempdir().unwrap();
        let options =
            lattice_core::shared::testing::sqlite::file_options(directory.path().join("notes.db"));
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options.clone())
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE daily_notes_workspace(id TEXT PRIMARY KEY, title TEXT, content TEXT)",
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
        sqlx::raw_sql(include_str!(
            "../../../migrations/20261119000000_vault_note_index_and_mirror_state.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        let mut transaction = pool.begin().await.unwrap();
        sqlx::query("INSERT INTO daily_notes_workspace VALUES ('rolled-back', 'Title', 'body')")
            .execute(&mut *transaction)
            .await
            .unwrap();
        transaction.rollback().await.unwrap();
        let db = Database::new(pool.clone());
        assert!(pending(&db).await.unwrap().is_empty());
        sqlx::query("INSERT INTO daily_notes_workspace VALUES ('saved', 'Title', 'body')")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
        let reopened = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        let db = Database::new(reopened.clone());
        assert!(has_pending(&db, "saved").await.unwrap());
        let write = pending(&db).await.unwrap().remove(0);
        assert_eq!(write.note_id, "saved");
        defer(&db, &write).await.unwrap();
        assert!(pending(&db).await.unwrap().is_empty());
        // A newer committed edit resets retry delay and survives an old ack.
        sqlx::query("UPDATE daily_notes_workspace SET content = 'new' WHERE id = 'saved'")
            .execute(&reopened)
            .await
            .unwrap();
        acknowledge(&db, &write).await.unwrap();
        assert_eq!(pending(&db).await.unwrap().len(), 1);
        reopened.close().await;
    }

    #[tokio::test]
    async fn outbox_coalesces_and_does_not_acknowledge_a_newer_edit() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE daily_notes_workspace(id TEXT PRIMARY KEY, title TEXT, content TEXT)",
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
        sqlx::raw_sql(include_str!(
            "../../../migrations/20261119000000_vault_note_index_and_mirror_state.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO daily_notes_workspace VALUES ('note', 'Title', 'original')")
            .execute(&pool)
            .await
            .unwrap();
        let db = Database::new(pool.clone());
        let before = pending(&db).await.unwrap().remove(0);
        for _ in 0..1000 {
            sqlx::query("UPDATE daily_notes_workspace SET content = 'latest' WHERE id = 'note'")
                .execute(&pool)
                .await
                .unwrap();
        }
        acknowledge(&db, &before).await.unwrap();
        let writes = pending(&db).await.unwrap();
        assert_eq!(writes.len(), 1);
        sqlx::query("DELETE FROM daily_notes_workspace WHERE id = 'note'")
            .execute(&pool)
            .await
            .unwrap();
        acknowledge(&db, writes.first().unwrap()).await.unwrap();
        let deleted = pending(&db).await.unwrap().remove(0);
        acknowledge(&db, &deleted).await.unwrap();
        assert!(pending(&db).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn watcher_import_clears_outbox_mirror_flag_and_records_sha256() {
        let pool = lattice_core::shared::testing::sqlite::migrated_in_memory()
            .await
            .unwrap();
        let db = Database::new(pool.clone());

        let imported = upsert_from_vault_if_unqueued(
            &db,
            VaultNoteUpsert {
                id: "note-1",
                title: "Draft 1",
                content: "v1",
                created_at: "2026-11-19T00:00:00Z",
                updated_at: "2026-11-19T00:01:00Z",
            },
            "sha-v1",
        )
        .await
        .unwrap();
        assert!(imported);
        // Subsequent external saves must NOT see mirror_complete = 0 from the v1 import.
        assert!(!has_pending(&db, "note-1").await.unwrap());

        let writes = pending(&db).await.unwrap();
        assert_eq!(writes.len(), 1);
        assert!(writes[0].mirror_complete);
        assert_eq!(writes[0].last_mirrored_sha256.as_deref(), Some("sha-v1"));

        // Saving v2 from disk immediately succeeds without waiting for writer ack.
        let imported_v2 = upsert_from_vault_if_unqueued(
            &db,
            VaultNoteUpsert {
                id: "note-1",
                title: "Draft 2",
                content: "v2",
                created_at: "2026-11-19T00:00:00Z",
                updated_at: "2026-11-19T00:02:00Z",
            },
            "sha-v2",
        )
        .await
        .unwrap();
        assert!(imported_v2);
        assert!(!has_pending(&db, "note-1").await.unwrap());
        let writes_v2 = pending(&db).await.unwrap();
        assert_eq!(writes_v2.len(), 1);
        assert_eq!(writes_v2[0].last_mirrored_sha256.as_deref(), Some("sha-v2"));
    }

    #[tokio::test]
    async fn mark_rescan_seen_batch_handles_multiple_chunks() {
        let pool = lattice_core::shared::testing::sqlite::migrated_in_memory()
            .await
            .unwrap();
        let db = Database::new(pool.clone());
        let (run_id, _) = begin_rescan(&db).await.unwrap();
        let ids: Vec<String> = (0..520).map(|i| format!("note-{i}")).collect();
        mark_rescan_seen_batch(&db, &run_id, &ids).await.unwrap();
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM vault_rescan_seen WHERE run_id = ?")
                .bind(&run_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 520);
        finish_rescan(&db, &run_id).await.unwrap();
    }
}
