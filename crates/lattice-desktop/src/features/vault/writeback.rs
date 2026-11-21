//! Mirrors notes to markdown files on disk, and puts the mirrored file into
//! the corpus so a journal page is retrievable like any other document.

use std::path::{Path, PathBuf};
use std::sync::Arc;

#[cfg(test)]
use tokio::sync::mpsc;
use tokio::sync::Notify;

use lattice_core::application::ports::NoteIndexSink;
use lattice_core::application::ports::SettingsReaderPort;
use lattice_core::application::ports::WorkspaceNotesPort;
use lattice_core::shared::path_safety::single_file_name;
use lattice_core::shared::persistence::Database;

/// The note transaction already persisted its outbox entry. A notification
/// merely avoids waiting for the periodic recovery tick.
pub fn spawn_sync_workspace_note(writer: &VaultWriterHandle) {
    writer.wake.notify_one();
}

pub fn spawn_delete_workspace_note(writer: &VaultWriterHandle) {
    writer.wake.notify_one();
}

/// What the corpus should do about a note whose markdown file just changed.
///
/// Journal pages are written through to the vault but were historically never
/// indexed, so "retrieval over your journal" never happened. This is the one
/// decision that closes that gap; it is pure so it can be tested without a
/// container, a database, or a disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteIndexAction {
    /// Submit the file to the indexing pipeline.
    Index,
    /// Nothing to retrieve — an empty page is not worth a document row.
    Skip,
    /// The file is gone (or cleared to whitespace); drop it from the index.
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
        NoteSyncOutcome::Written { body } if body.trim().is_empty() => NoteIndexAction::Remove,
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
    // standalone containers therefore stops their worker without closing the
    // background scope.
    _lifetime: Arc<WriterLifetime>,
    /// Shared cell the worker reads on every error to decide whether
    /// to emit a `vault:write-error` Tauri event. Populated post-
    /// construction via `set_app_handle` because the AppHandle isn't
    /// available until after the container builder runs.
    app_handle: Arc<std::sync::RwLock<Option<tauri::AppHandle>>>,
    /// Corpus sink the worker uses to put a mirrored note into (or out
    /// of) the index. Populated post-construction via
    /// `set_note_index_sink` because the indexing pipeline only exists
    /// once the container is built — the same reason the AppHandle is
    /// installed after the fact.
    note_index_sink: Arc<std::sync::RwLock<Option<Arc<dyn NoteIndexSink>>>>,
}

impl VaultWriterHandle {
    pub async fn enqueue_backfill(
        &self,
        database: &Database,
    ) -> lattice_core::shared::error::Result<()> {
        super::repository::enqueue_all(database).await?;
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

    /// Install the corpus sink so the worker can index or remove a
    /// mirrored note. Called from `Container::with_app_handle` once the
    /// indexing pipeline exists. Safe to call multiple times; later calls
    /// overwrite earlier (non-test code only ever calls it once).
    pub fn set_note_index_sink(&self, sink: Arc<dyn NoteIndexSink>) {
        match self.note_index_sink.write() {
            Ok(mut guard) => *guard = Some(sink),
            Err(poisoned) => *poisoned.into_inner() = Some(sink),
        }
    }
}

pub fn start_vault_writer(
    settings_uc: Arc<impl SettingsReaderPort + 'static>,
    suppression: super::watcher::WriteSuppressionRegistry,
    database: Database,
    background: &lattice_core::shared::runtime::background::BackgroundTasks,
) -> VaultWriterHandle {
    let wake = Arc::new(Notify::new());
    let app_handle = Arc::new(std::sync::RwLock::new(None));
    let note_index_sink = Arc::new(std::sync::RwLock::new(None));
    let cancel = background.token().child_token();
    let lifetime = Arc::new(WriterLifetime(cancel.clone()));
    background.spawn(run_worker(
        wake.clone(),
        settings_uc as Arc<dyn SettingsReaderPort>,
        app_handle.clone(),
        note_index_sink.clone(),
        suppression,
        database,
        cancel,
    ));
    VaultWriterHandle {
        wake,
        app_handle,
        note_index_sink,
        _lifetime: lifetime,
    }
}

async fn run_worker(
    wake: Arc<Notify>,
    settings_uc: Arc<dyn SettingsReaderPort>,
    app_handle: Arc<std::sync::RwLock<Option<tauri::AppHandle>>>,
    note_index_sink: Arc<std::sync::RwLock<Option<Arc<dyn NoteIndexSink>>>>,
    suppression: super::watcher::WriteSuppressionRegistry,
    database: Database,
    cancel: tokio_util::sync::CancellationToken,
) {
    let repository = super::repository::workspace_notes(&database);
    loop {
        // Intents survive process shutdown. Finish the current atomic write;
        // leave the rest in SQLite for startup replay.
        if cancel.is_cancelled() {
            break;
        }
        let settings = match settings_uc.current_settings().await {
            Ok(settings) => settings,
            Err(error) => {
                tracing::warn!(%error, "Vault writer could not load settings");
                tokio::select! { _ = cancel.cancelled() => break, _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {} }
                continue;
            }
        };
        if settings.vault.enabled {
            if let Some(root) =
                lattice_core::shared::fs::roots::resolve_vault_root(&settings.vault.vault_path)
            {
                match super::repository::pending(&database).await {
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
                                let _ = super::repository::defer(&database, &write).await;
                                failed = true;
                                continue;
                            }
                            let target = root.join("notes").join(format!("{}.md", write.note_id));
                            if !write.mirror_complete {
                                suppression.mark_written(target.clone()).await;
                            }
                            let result = match repository.get(&write.note_id).await {
                                Ok(note) => {
                                    let mirror_outcome = if write.mirror_complete {
                                        Ok(None)
                                    } else {
                                        mirror_note_to_disk(
                                            &database,
                                            &suppression,
                                            &app_handle,
                                            &write,
                                            &note,
                                            &target,
                                        )
                                        .await
                                    };
                                    match mirror_outcome {
                                        Ok(preserved_external_body) => {
                                            // Keep the markdown write durable even if this index
                                            // action fails; retries skip the file rewrite.
                                            let body_for_index = preserved_external_body
                                                .as_deref()
                                                .unwrap_or(&note.content);
                                            apply_note_index_action(
                                                &note_index_sink,
                                                &target,
                                                note_index_action(NoteSyncOutcome::Written {
                                                    body: body_for_index,
                                                }),
                                            )
                                            .await
                                        }
                                        Err(error) => Err(error),
                                    }
                                }
                                Err(lattice_core::shared::error::AppError::NotFound(_)) => {
                                    let mirror_result: std::result::Result<(), String> = async {
                                        if !write.mirror_complete {
                                            match tokio::fs::remove_file(&target).await {
                                                Ok(()) => {}
                                                Err(error)
                                                    if error.kind()
                                                        == std::io::ErrorKind::NotFound => {}
                                                Err(error) => return Err(error.to_string()),
                                            }
                                            super::repository::mark_mirrored(
                                                &database, &write, None,
                                            )
                                            .await
                                            .map_err(|error| error.to_string())?;
                                        }
                                        Ok(())
                                    }
                                    .await;
                                    match mirror_result {
                                        Ok(()) => {
                                            apply_note_index_action(
                                                &note_index_sink,
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
                            match settle_write(&database, &write, result).await {
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
    single_file_name(id).is_ok()
}

async fn settle_write(
    database: &Database,
    write: &super::repository::PendingWrite,
    apply_result: std::result::Result<(), String>,
) -> lattice_core::shared::error::Result<()> {
    match apply_result {
        Ok(()) => super::repository::acknowledge(database, write).await,
        Err(error) => {
            super::repository::defer(database, write).await?;
            Err(lattice_core::shared::error::AppError::InternalError(error))
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
/// hold one. It reaches the indexing pipeline through a `NoteIndexSink` the
/// composition root installs once the container is built. A missing sink (boot
/// order, test fixtures) retains the durable index intent for retry.
///
/// Runs inline in the single worker so the same note is never indexed twice
/// concurrently. Failed index work is deferred without rewriting its mirror.
async fn apply_note_index_action(
    note_index_sink: &Arc<std::sync::RwLock<Option<Arc<dyn NoteIndexSink>>>>,
    target: &Path,
    action: NoteIndexAction,
) -> std::result::Result<(), String> {
    if matches!(action, NoteIndexAction::Skip) {
        tracing::debug!(target = %target.display(), "vault worker: empty note — not indexed");
        return Ok(());
    }

    let sink = {
        let guard = match note_index_sink.read() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        match guard.as_ref() {
            Some(s) => Arc::clone(s),
            None => {
                return Err("Note index sink is unavailable; vault index action deferred".into())
            }
        }
    };

    let path = target.to_string_lossy().to_string();
    let outcome = match action {
        NoteIndexAction::Index => sink.index(&path).await,
        // `Skip` returned above; `Remove` is the only case left.
        _ => sink.remove(&path).await,
    };

    outcome.map_err(|e| {
        tracing::warn!(
            target = %path,
            error = %e,
            ?action,
            "vault worker: note index update failed"
        );
        e.to_string()
    })?;
    tracing::debug!(target = %path, ?action, "vault worker: note index updated");
    Ok(())
}

/// Resolve vault path; empty string falls back to `<home>/Lattice`.
///
/// Delegates to [`lattice_core::shared::fs::roots::resolve_vault_root`]. Kept
/// at this path for the settings side effects, which name it here.
pub fn resolve_vault_root(configured: &str) -> Option<PathBuf> {
    lattice_core::shared::fs::roots::resolve_vault_root(configured)
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

fn emit_note_imported(
    app_handle: &Arc<std::sync::RwLock<Option<tauri::AppHandle>>>,
    note_id: &str,
) {
    use tauri::Emitter;
    let guard = match app_handle.read() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    let Some(handle) = guard.as_ref() else {
        return;
    };
    let payload = serde_json::json!({ "noteId": note_id });
    if let Err(e) = handle.emit("vault:note-imported", payload) {
        tracing::warn!(error = %e, "failed to emit vault:note-imported event");
    }
}

struct ExistingMarkdown {
    sha256: String,
    parsed: Option<super::parse::ParsedNote>,
}

async fn inspect_existing_markdown(
    target: &Path,
) -> std::result::Result<Option<ExistingMarkdown>, String> {
    let bytes = match tokio::fs::read(target).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let sha256 = lattice_core::shared::hash::sha256_hex(&bytes);
    let parsed = std::str::from_utf8(&bytes)
        .ok()
        .and_then(|text| super::parse::parse_note(text).ok());
    Ok(Some(ExistingMarkdown { sha256, parsed }))
}

fn changed_outside_lattice(write: &super::repository::PendingWrite, disk_sha256: &str) -> bool {
    write
        .last_mirrored_sha256
        .as_deref()
        .is_some_and(|baseline| baseline != disk_sha256)
        && write.writing_sha256.as_deref() != Some(disk_sha256)
}

/// Mirror `note` to `target` unless the on-disk file was externally modified
/// since `write.last_mirrored_sha256`. When an external modification is detected,
/// SQLite `daily_notes_workspace` is reconciled to match the preserved disk file
/// so disk and database converge without a split-brain.
async fn mirror_note_to_disk(
    database: &Database,
    suppression: &super::watcher::WriteSuppressionRegistry,
    app_handle: &Arc<std::sync::RwLock<Option<tauri::AppHandle>>>,
    write: &super::repository::PendingWrite,
    note: &lattice_core::application::contracts::notes::WorkspaceNoteRecord,
    target: &Path,
) -> std::result::Result<Option<String>, String> {
    let existing = inspect_existing_markdown(target).await?;

    if let Some(existing_md) = existing.as_ref() {
        if changed_outside_lattice(write, &existing_md.sha256) {
            let expected = write.last_mirrored_sha256.as_deref().unwrap_or_default();
            tracing::warn!(
                note_id = %write.note_id,
                target = %target.display(),
                expected_sha256 = %expected,
                disk_sha256 = %existing_md.sha256,
                "vault worker: target modified externally while writeback was queued; preserving external file"
            );
            if let Some(parsed) = existing_md
                .parsed
                .as_ref()
                .filter(|parsed| parsed.id == write.note_id)
            {
                let reconciled = super::repository::reconcile_external_note_and_mark_mirrored(
                    database,
                    write,
                    lattice_core::application::contracts::notes::VaultNoteUpsert {
                        id: &parsed.id,
                        title: &parsed.title,
                        content: &parsed.body,
                        created_at: &parsed.created_at,
                        updated_at: &parsed.updated_at,
                    },
                    &existing_md.sha256,
                )
                .await
                .map_err(|error| error.to_string())?;
                if reconciled {
                    emit_note_imported(app_handle, &parsed.id);
                }
                return Ok(Some(parsed.body.clone()));
            }
            super::repository::mark_mirrored(database, write, Some(&existing_md.sha256))
                .await
                .map_err(|error| error.to_string())?;
            return Ok(existing_md.parsed.as_ref().map(|p| p.body.clone()));
        }
    }

    let existing_tags = existing
        .as_ref()
        .and_then(|md| md.parsed.as_ref())
        .map(|parsed| parsed.tags.as_slice())
        .unwrap_or(&[]);
    let frontmatter = build_frontmatter(
        &note.id,
        &note.title,
        &note.created_at,
        &note.updated_at,
        existing_tags,
    );
    let document = format!("{frontmatter}\n\n{}", note.content);
    let new_sha256 = lattice_core::shared::hash::sha256_hex(document.as_bytes());

    if existing.as_ref().is_some_and(|md| md.sha256 == new_sha256) {
        super::repository::mark_mirrored(database, write, Some(&new_sha256))
            .await
            .map_err(|error| error.to_string())?;
        return Ok(None);
    }

    super::repository::begin_mirror(database, &note.id, &new_sha256)
        .await
        .map_err(|error| error.to_string())?;
    atomic_write(target, &document)
        .await
        .map_err(|error| error.to_string())?;
    suppression.mark_written(target.to_path_buf()).await;
    super::repository::mark_mirrored(database, write, Some(&new_sha256))
        .await
        .map_err(|error| error.to_string())?;
    Ok(None)
}

async fn atomic_write(target: &Path, contents: &str) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let tmp = match target.file_name().and_then(|n| n.to_str()) {
        Some(name) => target.with_file_name(format!("{}.{}.tmp", name, uuid::Uuid::new_v4())),
        None => target.with_extension(format!("md.{}.tmp", uuid::Uuid::new_v4())),
    };
    let write_result: std::io::Result<()> = async {
        let mut file = tokio::fs::File::create(&tmp).await?;
        file.write_all(contents.as_bytes()).await?;
        file.sync_all().await?;
        Ok(())
    }
    .await;
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
    use sqlx::SqlitePool;

    #[cfg(windows)]
    #[test]
    fn a_note_id_windows_reads_as_a_parent_or_a_drive_is_not_safe() {
        for id in ["...", ".. ", "C:x"] {
            assert!(!safe_note_id(id), "{id:?}");
        }
    }

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
    fn empty_write_removes_from_index() {
        assert_eq!(
            note_index_action(NoteSyncOutcome::Written { body: "" }),
            NoteIndexAction::Remove
        );
    }

    #[test]
    fn whitespace_only_write_removes_from_index() {
        assert_eq!(
            note_index_action(NoteSyncOutcome::Written {
                body: "   \n\t  \r\n "
            }),
            NoteIndexAction::Remove
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
        let db = Database::new(pool.clone());
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
        sqlx::raw_sql(include_str!(
            "../../../migrations/20261121010000_vault_mirror_writing.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO daily_notes_workspace VALUES ('note', 'Title', 'body')")
            .execute(&pool)
            .await
            .unwrap();
        let write = super::super::repository::pending(&db)
            .await
            .unwrap()
            .remove(0);

        super::super::repository::mark_mirrored(&db, &write, Some("sha"))
            .await
            .unwrap();
        assert!(
            settle_write(&db, &write, Err("simulated index failure".into()))
                .await
                .is_err()
        );
        assert!(!super::super::repository::has_pending(&db, "note")
            .await
            .unwrap());
        assert_eq!(
            super::super::repository::pending(&db).await.unwrap().len(),
            0
        );
        sqlx::query("UPDATE daily_notes_workspace SET content = 'new edit' WHERE id = 'note'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(super::super::repository::has_pending(&db, "note")
            .await
            .unwrap());
        assert_eq!(
            super::super::repository::pending(&db).await.unwrap().len(),
            1
        );
    }

    async fn own_write_fixture() -> (
        SqlitePool,
        Database,
        tempfile::TempDir,
        std::path::PathBuf,
        super::super::watcher::WriteSuppressionRegistry,
        Arc<std::sync::RwLock<Option<tauri::AppHandle>>>,
    ) {
        let pool = lattice_core::shared::testing::sqlite::migrated_in_memory()
            .await
            .unwrap();
        let db = Database::new(pool.clone());
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("notes/n1.md");
        let suppression = super::super::watcher::WriteSuppressionRegistry::new();
        let app_handle = Arc::new(std::sync::RwLock::new(None));
        sqlx::query(
            "INSERT INTO daily_notes_workspace (id, title, content, created_at, updated_at) \
             VALUES ('n1', 'T1', 'first', '2026-11-19T00:00:00Z', '2026-11-19T00:01:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();
        (pool, db, dir, target, suppression, app_handle)
    }

    async fn pass_over_n1(
        db: &Database,
        suppression: &super::super::watcher::WriteSuppressionRegistry,
        app_handle: &Arc<std::sync::RwLock<Option<tauri::AppHandle>>>,
        target: &Path,
    ) -> Option<String> {
        let write = super::super::repository::pending(db)
            .await
            .unwrap()
            .remove(0);
        let note = super::super::repository::workspace_notes(db)
            .get("n1")
            .await
            .unwrap();
        let outside = mirror_note_to_disk(db, suppression, app_handle, &write, &note, target)
            .await
            .unwrap();
        settle_write(db, &write, Ok(())).await.unwrap();
        outside
    }

    #[tokio::test]
    async fn a_save_during_a_write_keeps_the_newer_text() {
        let (pool, db, _dir, target, suppression, app_handle) = own_write_fixture().await;
        pass_over_n1(&db, &suppression, &app_handle, &target).await;

        sqlx::query("UPDATE daily_notes_workspace SET content = 'second' WHERE id = 'n1'")
            .execute(&pool)
            .await
            .unwrap();
        let w2 = super::super::repository::pending(&db)
            .await
            .unwrap()
            .remove(0);
        let n2 = super::super::repository::workspace_notes(&db)
            .get("n1")
            .await
            .unwrap();
        sqlx::query("UPDATE daily_notes_workspace SET content = 'third' WHERE id = 'n1'")
            .execute(&pool)
            .await
            .unwrap();
        mirror_note_to_disk(&db, &suppression, &app_handle, &w2, &n2, &target)
            .await
            .unwrap();
        settle_write(&db, &w2, Ok(())).await.unwrap();

        let outside = pass_over_n1(&db, &suppression, &app_handle, &target).await;

        let note = super::super::repository::workspace_notes(&db)
            .get("n1")
            .await
            .unwrap();
        assert_eq!(note.content, "third", "the page must keep the newer save");
        let on_disk = tokio::fs::read_to_string(&target).await.unwrap();
        assert!(on_disk.contains("third"));
        assert_eq!(outside, None);
        assert!(super::super::repository::pending(&db)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn a_write_whose_baseline_did_not_commit_is_still_lattices_own() {
        let (pool, db, _dir, target, suppression, app_handle) = own_write_fixture().await;
        pass_over_n1(&db, &suppression, &app_handle, &target).await;

        sqlx::query("UPDATE daily_notes_workspace SET content = 'second' WHERE id = 'n1'")
            .execute(&pool)
            .await
            .unwrap();
        let w2 = super::super::repository::pending(&db)
            .await
            .unwrap()
            .remove(0);
        let n2 = super::super::repository::workspace_notes(&db)
            .get("n1")
            .await
            .unwrap();
        sqlx::query(
            "CREATE TRIGGER fail_mirror_record BEFORE UPDATE ON vault_write_outbox \
             BEGIN SELECT RAISE(ABORT, 'injected'); END",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(
            mirror_note_to_disk(&db, &suppression, &app_handle, &w2, &n2, &target)
                .await
                .is_err()
        );
        let on_disk = tokio::fs::read_to_string(&target).await.unwrap();
        assert!(on_disk.contains("second"));
        sqlx::query("DROP TRIGGER fail_mirror_record")
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query("UPDATE daily_notes_workspace SET content = 'third' WHERE id = 'n1'")
            .execute(&pool)
            .await
            .unwrap();
        pass_over_n1(&db, &suppression, &app_handle, &target).await;

        let note = super::super::repository::workspace_notes(&db)
            .get("n1")
            .await
            .unwrap();
        assert_eq!(note.content, "third");
        let on_disk = tokio::fs::read_to_string(&target).await.unwrap();
        assert!(on_disk.contains("third"));
    }

    #[tokio::test]
    async fn mirror_preserves_existing_frontmatter_tags_and_skips_clobbering_external_edits() {
        let pool = lattice_core::shared::testing::sqlite::migrated_in_memory()
            .await
            .unwrap();
        let db = Database::new(pool.clone());
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("notes/n1.md");
        tokio::fs::create_dir_all(target.parent().unwrap())
            .await
            .unwrap();

        let initial_disk = "---\nid: n1\ntitle: \"T1\"\ncreated_at: 2026-11-19T00:00:00Z\nupdated_at: 2026-11-19T00:00:00Z\ntags: [\"research\", \"ai\"]\n---\n\nold body";
        tokio::fs::write(&target, initial_disk).await.unwrap();
        let initial_sha = lattice_core::shared::hash::sha256_hex(initial_disk.as_bytes());

        sqlx::query(
            "INSERT INTO daily_notes_workspace (id, title, content, created_at, updated_at) \
             VALUES ('n1', 'T1', 'new body from app', '2026-11-19T00:00:00Z', '2026-11-19T00:01:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO vault_mirror_state (note_id, last_mirrored_sha256) VALUES ('n1', ?)",
        )
        .bind(&initial_sha)
        .execute(&pool)
        .await
        .unwrap();

        let write = super::super::repository::pending(&db)
            .await
            .unwrap()
            .remove(0);
        let note = super::super::repository::workspace_notes(&db)
            .get("n1")
            .await
            .unwrap();
        let suppression = super::super::watcher::WriteSuppressionRegistry::new();
        let app_handle = Arc::new(std::sync::RwLock::new(None));

        // Matching SHA-256: mirrors new body AND preserves external tags!
        let preserved = mirror_note_to_disk(&db, &suppression, &app_handle, &write, &note, &target)
            .await
            .unwrap();
        assert_eq!(preserved, None);
        let on_disk = tokio::fs::read_to_string(&target).await.unwrap();
        assert!(on_disk.contains("tags: [\"research\", \"ai\"]"));
        assert!(on_disk.contains("new body from app"));

        // Settle the first write (deleting the outbox row) to match production
        // flow: the persisted SHA-256 baseline must survive acknowledge().
        settle_write(&db, &write, Ok(())).await.unwrap();
        assert!(super::super::repository::pending(&db)
            .await
            .unwrap()
            .is_empty());

        // Now simulate external edit on disk while a new outbox write is queued with stale expected sha.
        let external_disk = "---\nid: n1\ntitle: \"T1\"\ncreated_at: 2026-11-19T00:00:00Z\nupdated_at: 2026-11-19T00:05:00Z\ntags: [\"research\", \"ai\"]\n---\n\nobsidian edit v2";
        tokio::fs::write(&target, external_disk).await.unwrap();
        sqlx::query("UPDATE daily_notes_workspace SET content = 'stale app edit' WHERE id = 'n1'")
            .execute(&pool)
            .await
            .unwrap();
        let stale_write = super::super::repository::pending(&db)
            .await
            .unwrap()
            .remove(0);
        let stale_note = super::super::repository::workspace_notes(&db)
            .get("n1")
            .await
            .unwrap();

        let preserved = mirror_note_to_disk(
            &db,
            &suppression,
            &app_handle,
            &stale_write,
            &stale_note,
            &target,
        )
        .await
        .unwrap();
        assert_eq!(preserved.as_deref(), Some("obsidian edit v2"));
        let disk_after = tokio::fs::read_to_string(&target).await.unwrap();
        assert!(
            disk_after.contains("obsidian edit v2"),
            "external edit on disk must not be overwritten"
        );
        // SQLite must also converge to the preserved external content and settle cleanly.
        let converged_note = super::super::repository::workspace_notes(&db)
            .get("n1")
            .await
            .unwrap();
        assert_eq!(converged_note.content, "obsidian edit v2");
        settle_write(&db, &stale_write, Ok(())).await.unwrap();
        assert!(super::super::repository::pending(&db)
            .await
            .unwrap()
            .is_empty());
    }
}
