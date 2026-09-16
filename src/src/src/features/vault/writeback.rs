//! Mirrors notes to markdown files on disk, and puts the mirrored file into
//! the corpus so a journal page is retrievable like any other document.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sqlx::SqlitePool;
use tokio::sync::mpsc;

use crate::features::settings::use_cases::GetSettingsUseCase;
use crate::interfaces::di::Container;

pub fn spawn_sync_workspace_note(
    container: &Container,
    id: String,
    title: String,
    body: String,
    created_at: String,
    updated_at: String,
    tags: Vec<String>,
) {
    let handle = container.vault_writer();
    handle.submit(VaultWriteJob::WorkspaceNote {
        id,
        title,
        body,
        created_at,
        updated_at,
        tags,
    });
}

/// Removes a note's markdown file from the vault.
///
/// Must go through the writer queue rather than deleting inline from the
/// command: the queue serializes against in-flight writes for the same note,
/// and it stamps the suppression registry so the watcher doesn't observe the
/// removal as an external change. Deleting the file directly would race a
/// pending `WorkspaceNote` write, which could re-create the file we just
/// removed.
pub fn spawn_delete_workspace_note(container: &Container, id: String) {
    let handle = container.vault_writer();
    handle.submit(VaultWriteJob::DeleteWorkspaceNote { id });
}

pub enum VaultWriteJob {
    WorkspaceNote {
        id: String,
        title: String,
        body: String,
        created_at: String,
        updated_at: String,
        tags: Vec<String>,
    },
    DeleteWorkspaceNote {
        id: String,
    },
    Backfill {
        pool: SqlitePool,
        vault_root: PathBuf,
    },
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

#[derive(Clone)]
pub struct VaultWriterHandle {
    tx: mpsc::UnboundedSender<VaultWriteJob>,
    /// Shared cell the worker reads on every error to decide whether
    /// to emit a `vault:write-error` Tauri event. Populated post-
    /// construction via `set_app_handle` because the AppHandle isn't
    /// available until after the container builder runs.
    app_handle: Arc<std::sync::RwLock<Option<tauri::AppHandle>>>,
}

impl VaultWriterHandle {
    pub fn submit(&self, job: VaultWriteJob) {
        if let Err(e) = self.tx.send(job) {
            tracing::warn!(error = %e, "vault writer queue dropped a job (worker shut down)");
        }
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
) -> VaultWriterHandle {
    let (tx, rx) = mpsc::unbounded_channel();
    let app_handle = Arc::new(std::sync::RwLock::new(None));
    tokio::spawn(run_worker(
        rx,
        settings_uc,
        Arc::clone(&app_handle),
        suppression,
    ));
    VaultWriterHandle { tx, app_handle }
}

async fn run_worker(
    mut rx: mpsc::UnboundedReceiver<VaultWriteJob>,
    settings_uc: Arc<GetSettingsUseCase>,
    app_handle: Arc<std::sync::RwLock<Option<tauri::AppHandle>>>,
    suppression: super::watcher::WriteSuppressionRegistry,
) {
    tracing::info!("vault writer worker started");
    while let Some(job) = rx.recv().await {
        let settings = match settings_uc.execute().await {
            Ok(s) => s,
            Err(e) => {
                tracing::debug!(error = %e, "vault worker: settings read failed — skipping job");
                continue;
            }
        };
        if !settings.vault.enabled {
            continue;
        }
        let vault_root = match resolve_vault_root(&settings.vault.vault_path) {
            Some(p) => p,
            None => {
                tracing::warn!("vault worker: could not resolve vault root — skipping job");
                continue;
            }
        };

        match job {
            VaultWriteJob::WorkspaceNote {
                id,
                title,
                body,
                created_at,
                updated_at,
                tags,
            } => {
                let target = vault_root.join("notes").join(format!("{}.md", id));
                let frontmatter = build_frontmatter(&id, &title, &created_at, &updated_at, &tags);
                let document = format!("{}\n\n{}", frontmatter, body);
                if let Err(e) = atomic_write(&target, &document).await {
                    tracing::warn!(
                        target = %target.display(),
                        error = %e,
                        "vault worker: workspace note write failed"
                    );
                    emit_write_error(&app_handle, Some(&id), &target, &e.to_string());
                } else {
                    // Stamp the suppression registry so the watcher
                    // doesn't treat our own write as an external edit
                    // and bounce it back through SQL → vault → ...
                    suppression.mark_written(target.clone()).await;
                    tracing::debug!(target = %target.display(), "vault worker: workspace note synced");
                    apply_note_index_action(
                        &app_handle,
                        &target,
                        note_index_action(NoteSyncOutcome::Written { body: &body }),
                    )
                    .await;
                }
            }
            VaultWriteJob::DeleteWorkspaceNote { id } => {
                let target = vault_root.join("notes").join(format!("{}.md", id));

                // Mark before unlinking. The watcher keys suppression on the
                // path, and a delete event can reach it before this await
                // returns; marking first closes that window.
                suppression.mark_written(target.clone()).await;

                match tokio::fs::remove_file(&target).await {
                    Ok(()) => {
                        tracing::debug!(target = %target.display(), "vault worker: workspace note removed");
                        apply_note_index_action(
                            &app_handle,
                            &target,
                            note_index_action(NoteSyncOutcome::Deleted),
                        )
                        .await;
                    }
                    // Already gone is the desired end state, not a failure —
                    // the user may have deleted it from the vault side first.
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        tracing::debug!(target = %target.display(), "vault worker: note file already absent");
                        apply_note_index_action(
                            &app_handle,
                            &target,
                            note_index_action(NoteSyncOutcome::Deleted),
                        )
                        .await;
                    }
                    Err(e) => {
                        tracing::warn!(
                            target = %target.display(),
                            error = %e,
                            "vault worker: workspace note delete failed"
                        );
                        emit_write_error(&app_handle, Some(&id), &target, &e.to_string());
                    }
                }
            }
            VaultWriteJob::Backfill {
                pool,
                vault_root: backfill_root,
            } => {
                run_backfill(pool, backfill_root, &app_handle, &suppression).await;
            }
        }
    }
    tracing::info!("vault writer worker exited (all senders dropped)");
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
/// (boot order, test fixtures) is a silent no-op: the markdown mirror is durable
/// either way, and the next edit retries the indexing.
///
/// Runs inline in the worker on purpose. The queue is FIFO, so indexing the same
/// note twice concurrently is impossible; the cost is that the next note's mirror
/// waits, which is latency, not data loss.
async fn apply_note_index_action(
    app_handle: &Arc<std::sync::RwLock<Option<tauri::AppHandle>>>,
    target: &Path,
    action: NoteIndexAction,
) {
    use tauri::Manager;

    if matches!(action, NoteIndexAction::Skip) {
        tracing::debug!(target = %target.display(), "vault worker: empty note — not indexed");
        return;
    }

    let handle = {
        let guard = match app_handle.read() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        match guard.as_ref() {
            Some(h) => h.clone(),
            None => return,
        }
    };
    let Some(container) = handle.try_state::<Container>() else {
        tracing::debug!("vault worker: container not in Tauri state yet — note not indexed");
        return;
    };

    let path = target.to_string_lossy().to_string();
    let outcome = match action {
        NoteIndexAction::Index => {
            crate::features::file::plugin::commands::index_file(path.clone(), None, container)
                .await
                .map(|_| ())
        }
        // `Skip` returned above; `Remove` is the only case left.
        _ => crate::features::file::plugin::commands::remove_indexed_file(path.clone(), container)
            .await,
    };

    match outcome {
        Ok(()) => tracing::debug!(target = %path, ?action, "vault worker: note index updated"),
        Err(e) => tracing::warn!(
            target = %path,
            error = %e.message,
            ?action,
            "vault worker: note index update failed"
        ),
    }
}

async fn run_backfill(
    pool: SqlitePool,
    vault_root: PathBuf,
    app_handle: &Arc<std::sync::RwLock<Option<tauri::AppHandle>>>,
    suppression: &super::watcher::WriteSuppressionRegistry,
) {
    let rows = match sqlx::query_as::<_, BackfillRow>(
        r#"
        SELECT id, title, content, created_at, updated_at
        FROM daily_notes_workspace
        "#,
    )
    .fetch_all(&pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "vault backfill: failed to enumerate notes");
            return;
        }
    };

    let total = rows.len();
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    for row in rows {
        let target = vault_root.join("notes").join(format!("{}.md", row.id));
        let frontmatter =
            build_frontmatter(&row.id, &row.title, &row.created_at, &row.updated_at, &[]);
        let document = format!("{}\n\n{}", frontmatter, row.content);
        match atomic_write(&target, &document).await {
            Ok(()) => {
                suppression.mark_written(target.clone()).await;
                apply_note_index_action(
                    app_handle,
                    &target,
                    note_index_action(NoteSyncOutcome::Written { body: &row.content }),
                )
                .await;
                succeeded += 1;
            }
            Err(e) => {
                tracing::warn!(
                    target = %target.display(),
                    error = %e,
                    "vault backfill: write failed for one note"
                );
                failed += 1;
                // Don't spam an event per row — surface a single
                // representative error after the walk completes.
                // (See post-loop emit below.)
                if failed == 1 {
                    emit_write_error(app_handle, Some(&row.id), &target, &e.to_string());
                }
            }
        }
    }
    tracing::info!(succeeded, failed, total, "vault backfill complete");
}

#[derive(sqlx::FromRow)]
struct BackfillRow {
    id: String,
    title: String,
    content: String,
    created_at: String,
    updated_at: String,
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
}
