//! Durable, coalesced vault write intentions. Bodies remain in the notes table.
use crate::infrastructure::persistence::repositories::workspace_notes::{
    timestamp_from_row, DailyNotesRepository,
};
use lattice_core::application::contracts::notes::NoteTimestampRecord;
use lattice_core::application::ports::WorkspaceNotesPort;
use lattice_core::shared::error::Result;
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
}

pub(super) async fn pending(db: &Database) -> Result<Vec<PendingWrite>> {
    let mut read = db.begin_read().await?;
    let writes = sqlx::query_as("SELECT note_id, revision, mirror_complete FROM vault_write_outbox WHERE retry_at <= unixepoch() ORDER BY rowid LIMIT 64")
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

pub(super) async fn mark_mirrored(db: &Database, write: &PendingWrite) -> Result<()> {
    let mut tx = db.begin_write().await?;
    sqlx::query(
        "UPDATE vault_write_outbox SET mirror_complete = 1 WHERE note_id = ? AND revision = ?",
    )
    .bind(&write.note_id)
    .bind(&write.revision)
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

pub(super) async fn mark_rescan_seen(db: &Database, run_id: &str, note_id: &str) -> Result<()> {
    let mut tx = db.begin_write().await?;
    sqlx::query("INSERT OR IGNORE INTO vault_rescan_seen(run_id, note_id) VALUES (?, ?)")
        .bind(run_id)
        .bind(note_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
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
        let options = crate::tests::sqlite::file_options(directory.path().join("notes.db"));
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
}
