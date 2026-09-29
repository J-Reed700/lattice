//! Mirrors notes to markdown files on disk, and puts the mirrored file into
//! the corpus so a journal page is retrievable like any other document.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sqlx::SqlitePool;
#[cfg(test)]
use tokio::sync::mpsc;
use tokio::sync::Notify;

use crate::features::settings::use_cases::GetSettingsUseCase;
use crate::interfaces::di::Container;

/// The note transaction already persisted its outbox entry. A notification
/// merely avoids waiting for the periodic recovery tick.
pub fn spawn_sync_workspace_note(container: &Container) {
    container.vault_writer().wake.notify_one();
}

pub fn spawn_delete_workspace_note(container: &Container) {
    container.vault_writer().wake.notify_one();
}

/// What the corpus should do about a note whose markdown file just changed.
///
/// Journal pages are written through to the vault but were historically never
/// indexed, so "retrieval over your journal" never happened. This is the one
/// decision that closes that gap; it is pure so it can be tested without a
/// container, a pool, or a disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteIndexAction {
    /// Submit the file to the indexing pipeline.
    Index,
    /// Nothing to retrieve — an empty page is not worth a document row.
    Skip,
    /// The file is gone; drop it from the index.
    Remove,
}

/// The note-sync event the action is decided from.
#[derive(Debug, Clone, Copy)]
pub enum NoteSyncOutcome<'a> {
    /// The markdown file was written successfully, with this note body.
    Written { body: &'a str },
    /// The markdown file was removed.
    Deleted,
}

pub fn note_index_action(outcome: NoteSyncOutcome<'_>) -> NoteIndexAction {
    match outcome {
        NoteSyncOutcome::Written { body } if body.trim().is_empty() => NoteIndexAction::Skip,
        NoteSyncOutcome::Written { .. } => NoteIndexAction::Index,
        NoteSyncOutcome::Deleted => NoteIndexAction::Remove,
    }
}

struct WriterLifetime(tokio_util::sync::CancellationToken);

impl Drop for WriterLifetime {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

#[derive(Clone)]
pub struct VaultWriterHandle {
    wake: Arc<Notify>,
    // Only handles own this guard; the worker holds a token clone. Dropping
    // standalone containers therefore stops their worker without an app scope.
    _lifetime: Arc<WriterLifetime>,
    /// Shared cell the worker reads on every error to decide whether
    /// to emit a `vault:write-error` Tauri event. Populated post-
    /// construction via `set_app_handle` because the AppHandle isn't
    /// available until after the container builder runs.
    app_handle: Arc<std::sync::RwLock<Option<tauri::AppHandle>>>,
}

impl VaultWriterHandle {
    pub async fn enqueue_backfill(&self, pool: &SqlitePool) -> crate::shared::error::Result<()> {
        super::repository::enqueue_all(pool).await?;
        self.wake.notify_one();
        Ok(())
    }

    /// Install the Tauri AppHandle so the worker can emit
    /// `vault:write-error` events when an atomic_write fails. Called
    /// from `Container::with_app_handle` once the handle is available.
    /// Safe to call multiple times; later calls overwrite earlier
    /// (non-test code only ever calls it once).
    pub fn set_app_handle(&self, handle: tauri::AppHandle) {
        match self.app_handle.write() {
            Ok(mut guard) => *guard = Some(handle),
            Err(poisoned) => *poisoned.into_inner() = Some(handle),
        }
    }
}

pub fn start_vault_writer(
    settings_uc: Arc<GetSettingsUseCase>,
    suppression: super::watcher::WriteSuppressionRegistry,
    pool: SqlitePool,
) -> VaultWriterHandle {
    let wake = Arc::new(Notify::new());
    let app_handle = Arc::new(std::sync::RwLock::new(None));
    let cancel = crate::shared::background::cancellation_token().child_token();
    let lifetime = Arc::new(WriterLifetime(cancel.clone()));
    crate::shared::background::spawn(run_worker(
        wake.clone(),
        settings_uc,
        app_handle.clone(),
        suppression,
        pool,
        cancel,
    ));
    VaultWriterHandle {
        wake,
        app_handle,
        _lifetime: lifetime,
    }
}

async fn run_worker(
    wake: Arc<Notify>,
    settings_uc: Arc<GetSettingsUseCase>,
    app_handle: Arc<std::sync::RwLock<Option<tauri::AppHandle>>>,
    suppression: super::watcher::WriteSuppressionRegistry,
    pool: SqlitePool,
    cancel: tokio_util::sync::CancellationToken,
) {
    let repository =
        crate::features::daily_notes::repository::DailyNotesRepository::new(pool.clone());
    loop {
        // Intents survive process shutdown. Finish the current atomic write;
        // leave the rest in SQLite for startup replay.
        if cancel.is_cancelled() {
            break;
        }
        let settings = match settings_uc.execute().await {
            Ok(settings) => settings,
            Err(error) => {
                tracing::warn!(%error, "Vault writer could not load settings");
                tokio::select! { _ = cancel.cancelled() => break, _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {} }
                continue;
            }
        };
        if settings.vault.enabled {
            if let Some(root) = resolve_vault_root(&settings.vault.vault_path) {
                match super::repository::pending(&pool).await {
                    Ok(writes) => {
                        let more = writes.len() == 64;
                        let mut failed = false;
                        for write in writes {
                            if cancel.is_cancelled() {
                                break;
                            }
                            // Imported front matter is untrusted: note ids
                            // must never escape the flat notes directory.
                            if !safe_note_id(&write.note_id) {
                                emit_write_error(
                                    &app_handle,
                                    Some(&write.note_id),
                                    &root,
                                    "Invalid note file identity",
                                );
                                let _ = super::repository::defer(&pool, &write).await;
                                failed = true;
                                continue;
                            }
                            let target = root.join("notes").join(format!("{}.md", write.note_id));
                            if !write.mirror_complete {
                                suppression.mark_written(target.clone()).await;
                            }
                            let result = match repository.get(&write.note_id).await {
                                Ok(note) => {
                                    let mirror_result = if write.mirror_complete {
                                        Ok(())
                                    } else {
                                        let frontmatter = build_frontmatter(
                                            &note.id,
                                            &note.title,
                                            &note.created_at,
                                            &note.updated_at,
                                            &[],
                                        );
                                        let document = format!("{frontmatter}\n\n{}", note.content);
                                        match atomic_write(&target, &document).await {
                                            Ok(()) => {
                                                suppression.mark_written(target.clone()).await;
                                                super::repository::mark_mirrored(&pool, &write)
                                                    .await
                                                    .map_err(|error| error.to_string())
                                            }
                                            Err(error) => Err(error.to_string()),
                                        }
                                    };
                                    match mirror_result {
                                        Ok(()) => {
                                            // Keep the markdown write durable even if this index
                                            // action fails; retries skip the file rewrite.
                                            apply_note_index_action(
                                                &app_handle,
                                                &target,
                                                note_index_action(NoteSyncOutcome::Written {
                                                    body: &note.content,
                                                }),
                                            )
                                            .await
                                        }
                                        Err(error) => Err(error),
                                    }
                                }
                                Err(crate::shared::error::AppError::NotFound(_)) => {
                                    let mirror_result: std::result::Result<(), String> = async {
                                        if !write.mirror_complete {
                                            match tokio::fs::remove_file(&target).await {
                                                Ok(()) => {}
                                                Err(error)
                                                    if error.kind()
                                                        == std::io::ErrorKind::NotFound => {}
                                                Err(error) => return Err(error.to_string()),
                                            }
                                            super::repository::mark_mirrored(&pool, &write)
                                                .await
                                                .map_err(|error| error.to_string())?;
                                        }
                                        Ok(())
                                    }
                                    .await;
                                    match mirror_result {
                                        Ok(()) => {
                                            apply_note_index_action(
                                                &app_handle,
                                                &target,
                                                NoteIndexAction::Remove,
                                            )
                                            .await
                                        }
                                        Err(error) => Err(error),
                                    }
                                }
                                Err(error) => Err(error.to_string()),
                            };
                            match settle_write(&pool, &write, result).await {
                                Ok(()) => {}
                                Err(error) => {
                                    emit_write_error(
                                        &app_handle,
                                        Some(&write.note_id),
                                        &target,
                                        &error.to_string(),
                                    );
                                    tracing::warn!(%error, "Vault write remains in the outbox for retry");
                                    failed = true;
                                }
                            }
                        }
                        if more && !failed {
                            continue;
                        }
                    }
                    Err(error) => tracing::warn!(%error, "Vault outbox read failed; retrying"),
                }
            }
        }
        tokio::select! {
            biased;
            _ = cancel.cancelled() => break,
            _ = wake.notified() => {},
            _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {},
        }
    }
}

pub(super) fn safe_note_id(id: &str) -> bool {
    !id.is_empty() && id != "." && id != ".." && !id.contains(['/', '\\', '\0'])
}

async fn settle_write(
    pool: &SqlitePool,
    write: &super::repository::PendingWrite,
    apply_result: std::result::Result<(), String>,
) -> crate::shared::error::Result<()> {
    match apply_result {
        Ok(()) => super::repository::acknowledge(pool, write).await,
        Err(error) => {
            super::repository::defer(pool, write).await?;
            Err(crate::shared::error::AppError::InternalError(error))
        }
    }
}

/// Emit a `vault:write-error` Tauri event so the frontend can surface
/// disk-full / permission-denied / vault-deleted-from-under-us cases
/// as a persistent toast. Without this, the user keeps editing notes
/// believing the vault is mirroring while every write silently fails.
///
/// Best-effort: a missing AppHandle (test fixtures, or a window that
/// has been destroyed) is logged but never crashes the worker.
fn emit_write_error(
    app_handle: &Arc<std::sync::RwLock<Option<tauri::AppHandle>>>,
    note_id: Option<&str>,
    target: &Path,
    error: &str,
) {
    use tauri::Emitter;
    let guard = match app_handle.read() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    let Some(handle) = guard.as_ref() else {
        // No handle yet — boot order or test fixture. Already logged
        // by the caller via tracing::warn; nothing more to do.
        return;
    };
    let payload = serde_json::json!({
        "noteId": note_id,
        "target": target.display().to_string(),
        "error": error,
    });
    if let Err(e) = handle.emit("vault:write-error", payload) {
        tracing::warn!(error = %e, "failed to emit vault:write-error event");
    }
}

/// Puts a mirrored journal page into (or takes it out of) the corpus, so the
/// vault's own notes are retrievable alongside imported documents.
///
/// The writer queue is constructed before the DI container exists, so it cannot
/// hold one — it reaches the container through the Tauri state the app manages
/// at boot, the same way every command does. A missing handle or container
/// (boot order, test fixtures) retains the durable index intent for retry.
///
/// Runs inline in the single worker so the same note is never indexed twice
/// concurrently. Failed index work is deferred without rewriting its mirror.
async fn apply_note_index_action(
    app_handle: &Arc<std::sync::RwLock<Option<tauri::AppHandle>>>,
    target: &Path,
    action: NoteIndexAction,
) -> std::result::Result<(), String> {
    use tauri::Manager;

    if matches!(action, NoteIndexAction::Skip) {
        tracing::debug!(target = %target.display(), "vault worker: empty note — not indexed");
        return Ok(());
    }

    let handle = {
        let guard = match app_handle.read() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        match guard.as_ref() {
            Some(h) => h.clone(),
            None => {
                return Err("Tauri app handle is unavailable; vault index action deferred".into())
            }
        }
    };
    let Some(container) = handle.try_state::<Container>() else {
        return Err("Tauri container is unavailable; vault index action deferred".into());
    };

    let path = target.to_string_lossy().to_string();
    let outcome = match action {
        NoteIndexAction::Index => {
            crate::features::file::plugin::commands::index_file(path.clone(), None, container)
                .await
                .map(|_| ())
        }
        // `Skip` returned above; `Remove` is the only case left.
        _ => {
            crate::features::file::plugin::commands::remove_indexed_file(path.clone(), container)
                .await
        }
    };

    outcome.map_err(|e| {
        tracing::warn!(
            target = %path,
            error = %e.message,
            ?action,
            "vault worker: note index update failed"
        );
        e.message
    })?;
    tracing::debug!(target = %path, ?action, "vault worker: note index updated");
    Ok(())
}

/// Resolve vault path; empty string falls back to `<home>/Lattice`.
pub fn resolve_vault_root(configured: &str) -> Option<PathBuf> {
    let trimmed = configured.trim();
    if !trimmed.is_empty() {
        return Some(PathBuf::from(trimmed));
    }
    dirs::home_dir().map(|h| h.join("Lattice"))
}

fn build_frontmatter(
    id: &str,
    title: &str,
    created_at: &str,
    updated_at: &str,
    tags: &[String],
) -> String {
    let escaped_title = escape_yaml_double_quoted(title);
    let tags_block = if tags.is_empty() {
        "tags: []".to_string()
    } else {
        let escaped: Vec<String> = tags
            .iter()
            .map(|t| format!("\"{}\"", escape_yaml_double_quoted(t)))
            .collect();
        format!("tags: [{}]", escaped.join(", "))
    };
    format!(
        "---\nid: {id}\ntitle: \"{escaped_title}\"\ncreated_at: {created_at}\nupdated_at: {updated_at}\n{tags_block}\n---"
    )
}

fn escape_yaml_double_quoted(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "")
}

async fn atomic_write(target: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let tmp = match target.file_name().and_then(|n| n.to_str()) {
        Some(name) => target.with_file_name(format!("{}.{}.tmp", name, uuid::Uuid::new_v4())),
        None => target.with_extension(format!("md.{}.tmp", uuid::Uuid::new_v4())),
    };
    let write_result = tokio::fs::write(&tmp, contents.as_bytes()).await;
    if let Err(e) = write_result {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(e);
    }
    if let Err(e) = tokio::fs::rename(&tmp, target).await {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_empty_write_is_indexed() {
        assert_eq!(
            note_index_action(NoteSyncOutcome::Written {
                body: "Read three papers on retrieval today."
            }),
            NoteIndexAction::Index
        );
    }

    #[test]
    fn empty_write_is_skipped() {
        assert_eq!(
            note_index_action(NoteSyncOutcome::Written { body: "" }),
            NoteIndexAction::Skip
        );
    }

    #[test]
    fn whitespace_only_write_is_skipped() {
        assert_eq!(
            note_index_action(NoteSyncOutcome::Written {
                body: "   \n\t  \r\n "
            }),
            NoteIndexAction::Skip
        );
    }

    #[test]
    fn delete_removes_from_the_index() {
        assert_eq!(
            note_index_action(NoteSyncOutcome::Deleted),
            NoteIndexAction::Remove
        );
    }

    #[test]
    fn frontmatter_with_tags_roundtrips() {
        let fm = build_frontmatter(
            "abc-123",
            "Hello world",
            "2026-05-01T18:00:00Z",
            "2026-05-01T18:05:00Z",
            &["foo".to_string(), "bar".to_string()],
        );
        assert!(fm.starts_with("---\n"));
        assert!(fm.contains("id: abc-123"));
        assert!(fm.contains("title: \"Hello world\""));
        assert!(fm.contains("tags: [\"foo\", \"bar\"]"));
        assert!(fm.ends_with("---"));
    }

    #[test]
    fn frontmatter_escapes_quotes_in_title() {
        let fm = build_frontmatter("id1", "She said \"hi\"", "now", "now", &[]);
        assert!(fm.contains("title: \"She said \\\"hi\\\"\""));
    }

    #[test]
    fn frontmatter_escapes_newline_in_title() {
        let fm = build_frontmatter("id1", "Line one\nLine two", "now", "now", &[]);
        assert!(fm.contains("title: \"Line one\\nLine two\""));
        let title_line = fm.lines().find(|l| l.starts_with("title:")).unwrap();
        assert!(!title_line.contains('\n'));
    }

    #[test]
    fn frontmatter_strips_carriage_returns() {
        let fm = build_frontmatter("id1", "Windows\r\nlineending", "now", "now", &[]);
        assert!(fm.contains("title: \"Windows\\nlineending\""));
    }

    #[test]
    fn empty_tags_render_as_empty_array() {
        let fm = build_frontmatter("id", "title", "now", "now", &[]);
        assert!(fm.contains("tags: []"));
    }

    #[test]
    fn resolve_vault_root_uses_configured_when_present() {
        let root = resolve_vault_root("/explicit/vault").unwrap();
        assert_eq!(root, PathBuf::from("/explicit/vault"));
    }

    #[test]
    fn resolve_vault_root_falls_back_to_default_on_empty_string() {
        let root = resolve_vault_root("");
        if let Some(p) = root {
            assert!(p.ends_with("Lattice"));
        }
    }

    #[test]
    fn resolve_vault_root_treats_whitespace_as_empty() {
        let root = resolve_vault_root("   ");
        if let Some(p) = root {
            assert!(p.ends_with("Lattice"));
        }
    }

    #[tokio::test]
    async fn atomic_write_creates_parent_dirs_and_file() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("nested/dir/file.md");
        atomic_write(&target, "hello").await.unwrap();
        let read = tokio::fs::read_to_string(&target).await.unwrap();
        assert_eq!(read, "hello");
    }

    #[tokio::test]
    async fn atomic_write_handles_concurrent_writers() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("note.md");

        let target_a = target.clone();
        let target_b = target.clone();
        let a = tokio::spawn(async move { atomic_write(&target_a, "version A").await });
        let b = tokio::spawn(async move { atomic_write(&target_b, "version B").await });
        a.await.unwrap().unwrap();
        b.await.unwrap().unwrap();

        let final_contents = tokio::fs::read_to_string(&target).await.unwrap();
        assert!(
            final_contents == "version A" || final_contents == "version B",
            "final file must be one of the inputs verbatim, got: {:?}",
            final_contents
        );

        let mut entries = tokio::fs::read_dir(tmp.path()).await.unwrap();
        let mut count = 0;
        while let Some(entry) = entries.next_entry().await.unwrap() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            assert!(
                !name.contains(".tmp"),
                "unexpected leftover tmp file: {}",
                name
            );
            count += 1;
        }
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn worker_preserves_submit_order_for_same_id() {
        let tmp = tempfile::tempdir().unwrap();
        let vault_root = tmp.path().to_path_buf();

        let (tx, mut rx) = mpsc::unbounded_channel::<(String, String)>();
        let worker_root = vault_root.clone();
        let worker = tokio::spawn(async move {
            while let Some((id, body)) = rx.recv().await {
                let target = worker_root.join("notes").join(format!("{}.md", id));
                atomic_write(&target, &body).await.unwrap();
            }
        });

        let id = "ordering-test".to_string();
        for n in 0..50 {
            tx.send((id.clone(), format!("version-{n}"))).unwrap();
        }
        drop(tx);
        worker.await.unwrap();

        let final_contents =
            tokio::fs::read_to_string(vault_root.join("notes").join("ordering-test.md"))
                .await
                .unwrap();
        assert_eq!(
            final_contents, "version-49",
            "FIFO worker must end at the last submitted version"
        );
    }

    #[tokio::test]
    async fn failed_index_action_keeps_the_durable_write_for_retry() {
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
        sqlx::query("INSERT INTO daily_notes_workspace VALUES ('note', 'Title', 'body')")
            .execute(&pool)
            .await
            .unwrap();
        let write = super::super::repository::pending(&pool)
            .await
            .unwrap()
            .remove(0);

        super::super::repository::mark_mirrored(&pool, &write)
            .await
            .unwrap();
        assert!(
            settle_write(&pool, &write, Err("simulated index failure".into()))
                .await
                .is_err()
        );
        assert!(!super::super::repository::has_pending(&pool, "note")
            .await
            .unwrap());
        assert_eq!(
            super::super::repository::pending(&pool)
                .await
                .unwrap()
                .len(),
            0
        );
        sqlx::query("UPDATE daily_notes_workspace SET content = 'new edit' WHERE id = 'note'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(super::super::repository::has_pending(&pool, "note")
            .await
            .unwrap());
        assert_eq!(
            super::super::repository::pending(&pool)
                .await
                .unwrap()
                .len(),
            1
        );
    }
}
