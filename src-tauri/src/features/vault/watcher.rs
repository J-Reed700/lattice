//! Re-imports external `.md` edits into SQLite. Reads settings once at
//! start; toggling `watch_external_changes` requires an app restart.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use notify::{RecursiveMode, Watcher};
use sqlx::SqlitePool;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use tokio::io::AsyncReadExt;
use tokio::sync::Mutex;

use crate::features::daily_notes::repository::{DailyNotesRepository, VaultNoteUpsert};
use crate::features::settings::use_cases::GetSettingsUseCase;

mod rescan;

pub use rescan::{rescan_vault, RescanSummary};

const SUPPRESSION_TTL: Duration = Duration::from_secs(5);
const DEBOUNCE_WINDOW: Duration = Duration::from_millis(800);
static RESCAN_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Keyed by filename, not `PathBuf`: OS-level path normalization differs
/// between writer and watcher (Windows `\\?\` prefixes, macOS case-folding,
/// symlink resolution). All notes live flat in `<vault>/notes/`, so
/// filename uniqueness is sufficient.
#[derive(Clone, Default)]
pub struct WriteSuppressionRegistry {
    inner: Arc<Mutex<HashMap<String, Instant>>>,
}

impl WriteSuppressionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn mark_written(&self, path: PathBuf) {
        let Some(name) = filename_key(&path) else {
            return;
        };
        let mut guard = self.inner.lock().await;
        let now = Instant::now();
        // Prune on write so the read path stays O(1).
        guard.retain(|_, ts| now.duration_since(*ts) < SUPPRESSION_TTL);
        guard.insert(name, now);
    }

    pub async fn was_just_written(&self, path: &Path) -> bool {
        let Some(name) = filename_key(path) else {
            return false;
        };
        let guard = self.inner.lock().await;
        match guard.get(&name) {
            Some(ts) => ts.elapsed() < SUPPRESSION_TTL,
            None => false,
        }
    }
}

fn filename_key(path: &Path) -> Option<String> {
    path.file_name().map(|n| n.to_string_lossy().to_string())
}

pub fn start_vault_watcher(
    settings_uc: Arc<GetSettingsUseCase>,
    db_pool: SqlitePool,
    app_handle: tauri::AppHandle,
    suppression: WriteSuppressionRegistry,
) {
    crate::shared::runtime::background::spawn(async move {
        let settings = match settings_uc.execute().await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "vault watcher: settings read failed — not starting");
                return;
            }
        };
        if !settings.vault.enabled || !settings.vault.watch_external_changes {
            tracing::info!("vault watcher: disabled in settings — not starting");
            return;
        }
        let configured_root = match super::writeback::resolve_vault_root(&settings.vault.vault_path)
        {
            Some(p) => p,
            None => {
                tracing::warn!("vault watcher: vault root could not be resolved — not starting");
                return;
            }
        };

        if let Err(e) = tokio::fs::create_dir_all(&configured_root).await {
            tracing::warn!(
                vault_root = %configured_root.display(),
                error = %e,
                "vault watcher: vault folder doesn't exist and couldn't be created"
            );
            emit_watcher_error(
                &app_handle,
                &format!(
                    "Vault folder '{}' could not be created: {}",
                    configured_root.display(),
                    e
                ),
            );
            return;
        }

        // notify doesn't follow symlinks; canonicalize so a vault
        // pointed at e.g. `~/Dropbox/Lattice` watches the target.
        let vault_root = match tokio::fs::canonicalize(&configured_root).await {
            Ok(canonical) => canonical,
            Err(e) => {
                tracing::warn!(
                    configured_root = %configured_root.display(),
                    error = %e,
                    "vault watcher: failed to canonicalize vault root — using configured path verbatim"
                );
                configured_root
            }
        };

        run_watcher(vault_root, db_pool, app_handle, suppression).await;
    });
}

async fn run_watcher(
    vault_root: PathBuf,
    db_pool: SqlitePool,
    app_handle: tauri::AppHandle,
    suppression: WriteSuppressionRegistry,
) {
    // Queue paths, never arbitrarily large callback batches. Overflow is a
    // request to reconcile the directory; it cannot silently lose an edit.
    const PATH_CAPACITY: usize = 256;
    let (tx, mut rx) = tokio::sync::mpsc::channel::<PathBuf>(PATH_CAPACITY);
    let overflow = Arc::new(AtomicBool::new(false));
    let wake = Arc::new(tokio::sync::Notify::new());
    let callback_overflow = overflow.clone();
    let callback_wake = wake.clone();
    let mut debouncer =
        match notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
            match result {
                Ok(event) => {
                    for path in event.paths {
                        if path.extension().and_then(|s| s.to_str()) != Some("md") {
                            continue;
                        }
                        if tx.try_send(path).is_err() {
                            callback_overflow.store(true, Ordering::Release);
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "Vault watcher requires reconciliation");
                    callback_overflow.store(true, Ordering::Release);
                }
            }
            callback_wake.notify_one();
        }) {
            Ok(watcher) => watcher,
            Err(error) => {
                emit_watcher_error(
                    &app_handle,
                    &format!("Vault watcher failed to start: {error}"),
                );
                return;
            }
        };

    if let Err(e) = debouncer.watch(&vault_root, RecursiveMode::Recursive) {
        tracing::warn!(
            vault_root = %vault_root.display(),
            error = %e,
            "vault watcher: failed to watch vault root"
        );
        emit_watcher_error(
            &app_handle,
            &format!(
                "Vault watcher couldn't watch '{}': {}",
                vault_root.display(),
                e
            ),
        );
        return;
    }

    tracing::info!(
        vault_root = %vault_root.display(),
        debounce_ms = DEBOUNCE_WINDOW.as_millis() as u64,
        "vault watcher started"
    );

    let cancel = crate::shared::runtime::background::cancellation_token();
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => break,
            _ = wake.notified() => {}
        }
        tokio::select! {
            biased;
            _ = cancel.cancelled() => break,
            _ = tokio::time::sleep(DEBOUNCE_WINDOW) => {}
        }
        let mut paths = std::collections::HashSet::with_capacity(PATH_CAPACITY);
        for _ in 0..PATH_CAPACITY {
            match rx.try_recv() {
                Ok(path) => {
                    paths.insert(path);
                }
                Err(_) => break,
            }
        }
        // A single worker serializes updates/deletes to the same note. There
        // are no detached path tasks and the pending set has a hard bound.
        if overflow.swap(false, Ordering::AcqRel) {
            if let Err(error) = rescan_vault(
                db_pool.clone(),
                vault_root.clone(),
                suppression.clone(),
                app_handle.clone(),
            )
            .await
            {
                emit_watcher_error(&app_handle, &error.to_string());
            }
        }
        for path in paths {
            if cancel.is_cancelled() {
                break;
            }
            if !suppression.was_just_written(&path).await {
                import_one(&path, &db_pool, &app_handle).await;
            }
        }
    }
    tracing::info!("Vault watcher stopped");
}

const MAX_NOTE_BYTES: u64 = 4 * 1024 * 1024;

async fn read_note(path: &Path) -> std::io::Result<String> {
    let file = tokio::fs::File::open(path).await?;
    let mut contents = String::new();
    file.take(MAX_NOTE_BYTES + 1)
        .read_to_string(&mut contents)
        .await?;
    if contents.len() as u64 > MAX_NOTE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Vault note exceeds 4 MiB",
        ));
    }
    Ok(contents)
}

/// Pub(crate) so focus-rescan reuses the same parse/UPSERT/emit path.
///
/// Returns the id of the note it imported, which is the front-matter id and
/// not necessarily the file stem: a note renamed in Finder keeps its id.
pub(crate) async fn import_one(
    path: &Path,
    db_pool: &SqlitePool,
    app_handle: &tauri::AppHandle,
) -> Option<String> {
    let contents = match read_note(path).await {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Atomic-save gap: editor renamed old file away and hasn't
            // written the replacement yet. Sleep briefly and re-check
            // to avoid hard-deleting metadata in the gap.
            tokio::time::sleep(Duration::from_millis(200)).await;
            // repository-barrier-allow: watcher resolves an atomic-save event for the watched file resource.
            if tokio::fs::metadata(path).await.is_ok() {
                tracing::debug!(
                    path = %path.display(),
                    "vault watcher: file reappeared after atomic-save gap — skipping delete"
                );
                return None;
            }
            // A rename in Finder arrives as a Remove for the old name. The
            // note is still on disk under the new one, carrying its id in
            // the front matter; import that file instead of deleting.
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if let Some(renamed) = find_renamed_note(path, stem).await {
                    tracing::info!(
                        from = %path.display(),
                        to = %renamed.display(),
                        "vault watcher: note renamed on disk — keeping it"
                    );
                    return Box::pin(import_one(&renamed, db_pool, app_handle)).await;
                }
            }
            handle_external_delete(path, db_pool, app_handle).await;
            return None;
        }
        Err(e) => {
            tracing::debug!(
                path = %path.display(),
                error = %e,
                "vault watcher: read failed — skipping"
            );
            return None;
        }
    };

    let parsed = match super::parse::parse_note(&contents) {
        Ok(p) => p,
        Err(super::parse::ParseError::NotOurs) => {
            tracing::debug!(path = %path.display(), "vault watcher: untracked .md file — skipping");
            return None;
        }
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "vault watcher: parse failed — skipping"
            );
            return None;
        }
    };

    // A committed application edit/delete takes precedence until its mirror
    // has reached disk. Otherwise a focus scan can re-import the stale file
    // and resurrect a deleted note while its durable writeback is pending.
    if super::repository::has_pending(db_pool, &parsed.id)
        .await
        .unwrap_or(true)
    {
        return Some(parsed.id);
    }

    // A front-matter id that differs from the file stem is either a copy
    // (`abc.md` duplicated to `duplicate.md`, both carrying `id: abc`) or a
    // rename. A copy would overwrite the original on every rescan, so it is
    // skipped; a rename leaves no `abc.md` behind and is the same note.
    if !is_importable_name(path, &parsed.id).await {
        tracing::warn!(
            path = %path.display(),
            frontmatter_id = %parsed.id,
            "vault watcher: id mismatch while the note's own file exists — skipping copy"
        );
        return None;
    }

    // ON CONFLICT: only the fields that round-trip through markdown.
    // Lattice-only JSON columns (linked_document_ids, highlights_json,
    // etc.) are preserved.
    let repository = DailyNotesRepository::new(db_pool.clone());
    let result = repository
        .upsert_from_vault_if_unqueued(VaultNoteUpsert {
            id: &parsed.id,
            title: &parsed.title,
            content: &parsed.body,
            created_at: &parsed.created_at,
            updated_at: &parsed.updated_at,
        })
        .await;

    match result {
        Ok(true) => {
            tracing::info!(
                id = %parsed.id,
                path = %path.display(),
                "vault watcher: imported external edit"
            );
            emit_note_imported(app_handle, &parsed.id);
            Some(parsed.id)
        }
        Ok(false) => Some(parsed.id),
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "vault watcher: SQL upsert failed"
            );
            None
        }
    }
}

/// Whether the file at `path` may be imported as note `id`. Its own name is
/// always fine; another name is a rename only while `<id>.md` is absent from
/// the same folder, and otherwise a copy.
async fn is_importable_name(path: &Path, id: &str) -> bool {
    if path.file_stem().and_then(|s| s.to_str()) == Some(id) {
        return true;
    }
    let own_file = path.with_file_name(format!("{id}.md"));
    // repository-barrier-allow: watcher checks the watched vault folder for the note's own file.
    !matches!(tokio::fs::try_exists(&own_file).await, Ok(true) | Err(_))
}

/// Another `.md` file in the removed note's folder whose front-matter id is
/// `id`: where a note renamed in Finder now lives.
async fn find_renamed_note(removed: &Path, id: &str) -> Option<PathBuf> {
    let dir = removed.parent()?;
    // repository-barrier-allow: watcher searches the watched vault folder for a renamed note.
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(error) => {
            tracing::warn!(dir = %dir.display(), %error, "vault watcher: could not list notes to look for a rename");
            return None;
        }
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let candidate = entry.path();
        if candidate == removed || candidate.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        // repository-barrier-allow: watcher reads a candidate note in the watched vault folder.
        let Ok(contents) = read_note(&candidate).await else {
            continue;
        };
        if super::parse::parse_note(&contents).is_ok_and(|note| note.id == id) {
            return Some(candidate);
        }
    }
    None
}

/// Trusts the writer's `<id>.md` naming invariant — id is the file stem.
async fn handle_external_delete(path: &Path, db_pool: &SqlitePool, app_handle: &tauri::AppHandle) {
    let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
        tracing::debug!(
            path = %path.display(),
            "vault watcher: delete event has no file stem — skipping"
        );
        return;
    };

    if super::repository::has_pending(db_pool, id)
        .await
        .unwrap_or(true)
    {
        return;
    }

    let repository = DailyNotesRepository::new(db_pool.clone());
    let result = repository.delete_from_vault_if_unqueued(id).await;

    match result {
        Ok(true) => {
            tracing::info!(
                id = %id,
                path = %path.display(),
                "vault watcher: deleted note (external rm)"
            );
            emit_note_imported(app_handle, id);
        }
        Ok(false) => {
            tracing::debug!(
                id = %id,
                path = %path.display(),
                "vault watcher: delete event for unknown id — no-op"
            );
        }
        Err(e) => tracing::warn!(
            id = %id,
            error = %e,
            "vault watcher: SQL delete failed"
        ),
    }
}

fn emit_note_imported(app_handle: &tauri::AppHandle, note_id: &str) {
    use tauri::Emitter;
    let payload = serde_json::json!({ "noteId": note_id });
    if let Err(e) = app_handle.emit("vault:note-imported", payload) {
        tracing::warn!(error = %e, "failed to emit vault:note-imported event");
    }
}

fn emit_watcher_error(app_handle: &tauri::AppHandle, error: &str) {
    use tauri::Emitter;
    let payload = serde_json::json!({ "error": error });
    if let Err(e) = app_handle.emit("vault:watcher-error", payload) {
        tracing::warn!(error = %e, "failed to emit vault:watcher-error event");
    }
}

#[cfg(test)]
mod tests;
