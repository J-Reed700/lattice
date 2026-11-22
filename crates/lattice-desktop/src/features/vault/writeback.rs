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
                                super::events::emit_write_error(
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
                                    super::events::emit_write_error(
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
    let mut existing = inspect_existing_markdown(target).await?;
    let mut replaced = false;
    if let Some(md) = existing
        .as_ref()
        .filter(|md| changed_outside_lattice(write, &md.sha256))
    {
        match md
            .parsed
            .as_ref()
            .filter(|parsed| parsed.id == write.note_id)
        {
            Some(parsed) => {
                let file_text = super::repository::note_text_sha256(&parsed.title, &parsed.body);
                if write.mirrored_text_sha256.as_deref() != Some(file_text.as_str()) {
                    return take_outside_edit(database, app_handle, write, md, parsed, target)
                        .await;
                }
            }
            None => replaced = true,
        }
    }
    if replaced {
        set_aside(app_handle, note, target).await?;
        existing = None;
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
    let text_sha256 = super::repository::note_text_sha256(&note.title, &note.content);
    let synced = super::repository::Synced {
        file_sha256: &new_sha256,
        text_sha256: &text_sha256,
    };

    if existing.as_ref().is_some_and(|md| md.sha256 == new_sha256) {
        super::repository::mark_mirrored(database, write, Some(synced))
            .await
            .map_err(|error| error.to_string())?;
        return Ok(None);
    }

    let own_unrecorded = existing
        .as_ref()
        .filter(|md| write.writing_sha256.as_deref() == Some(md.sha256.as_str()))
        .and_then(|md| {
            md.parsed.as_ref().map(|parsed| {
                (
                    md.sha256.as_str(),
                    super::repository::note_text_sha256(&parsed.title, &parsed.body),
                )
            })
        });
    super::repository::begin_mirror(
        database,
        &note.id,
        own_unrecorded
            .as_ref()
            .map(|(file, text)| super::repository::Synced {
                file_sha256: file,
                text_sha256: text,
            }),
        &new_sha256,
    )
    .await
    .map_err(|error| error.to_string())?;
    atomic_write(target, &document)
        .await
        .map_err(|error| error.to_string())?;
    suppression.mark_written(target.to_path_buf()).await;
    super::repository::mark_mirrored(database, write, Some(synced))
        .await
        .map_err(|error| error.to_string())?;
    Ok(None)
}

async fn take_outside_edit(
    database: &Database,
    app_handle: &Arc<std::sync::RwLock<Option<tauri::AppHandle>>>,
    write: &super::repository::PendingWrite,
    md: &ExistingMarkdown,
    parsed: &super::parse::ParsedNote,
    target: &Path,
) -> std::result::Result<Option<String>, String> {
    let expected = write.last_mirrored_sha256.as_deref().unwrap_or_default();
    tracing::warn!(
        note_id = %write.note_id,
        target = %target.display(),
        expected_sha256 = %expected,
        disk_sha256 = %md.sha256,
        "vault worker: target modified externally while writeback was queued; preserving external file"
    );
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
        &md.sha256,
    )
    .await
    .map_err(|error| error.to_string())?;
    if let super::repository::Reconciled::TookFile { kept_copy } = reconciled {
        super::events::emit_note_imported(app_handle, &parsed.id);
        if let Some(copy) = kept_copy {
            super::events::emit_note_conflict(
                app_handle,
                super::events::NoteConflict::KeptCopy {
                    note_id: &parsed.id,
                    note_title: &parsed.title,
                    copy_id: &copy.id,
                    copy_title: &copy.title,
                },
            );
        }
    }
    Ok(Some(parsed.body.clone()))
}

async fn set_aside(
    app_handle: &Arc<std::sync::RwLock<Option<tauri::AppHandle>>>,
    note: &lattice_core::application::contracts::notes::WorkspaceNoteRecord,
    target: &Path,
) -> std::result::Result<(), String> {
    let stem = target
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("note");
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let mut kept = target.with_file_name(format!("{stem}.replaced-{stamp}.md"));
    let mut n = 2u32;
    while tokio::fs::try_exists(&kept)
        .await
        .map_err(|error| error.to_string())?
    {
        kept = target.with_file_name(format!("{stem}.replaced-{stamp}-{n}.md"));
        n += 1;
    }
    tokio::fs::rename(target, &kept)
        .await
        .map_err(|error| error.to_string())?;
    tracing::warn!(
        note_id = %note.id,
        target = %target.display(),
        kept = %kept.display(),
        "vault worker: the note's file was replaced outside Lattice; kept it beside the note"
    );
    super::events::emit_note_conflict(
        app_handle,
        super::events::NoteConflict::SetAside {
            note_id: &note.id,
            note_title: &note.title,
            set_aside_path: &kept,
        },
    );
    Ok(())
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
#[path = "writeback_tests.rs"]
mod tests;
