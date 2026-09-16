//! Re-imports external `.md` edits into SQLite. Reads settings once at
//! start; toggling `watch_external_changes` requires an app restart.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use notify::{RecursiveMode, Watcher};
use notify_debouncer_full::{new_debouncer, DebouncedEvent};
use sqlx::SqlitePool;
use tokio::sync::Mutex;

use crate::features::daily_notes::repository::{
    DailyNotesRepository, NoteTimestampRecord, VaultNoteUpsert,
};
use crate::features::settings::use_cases::GetSettingsUseCase;
use crate::shared::time::parse_db_timestamp;

const SUPPRESSION_TTL: Duration = Duration::from_secs(5);
const DEBOUNCE_WINDOW: Duration = Duration::from_millis(800);

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
    tokio::spawn(async move {
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
    // Debouncer callback runs on its own worker thread, not a tokio
    // worker; bridge into tokio via mpsc.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<DebouncedEvent>>();

    let mut debouncer = match new_debouncer(
        DEBOUNCE_WINDOW,
        None,
        move |result: notify_debouncer_full::DebounceEventResult| match result {
            Ok(events) => {
                if let Err(e) = tx.send(events) {
                    tracing::debug!(error = %e, "vault watcher: failed to forward debounced events");
                }
            }
            Err(errors) => {
                for err in errors {
                    tracing::warn!(error = %err, "vault watcher: notify error");
                }
            }
        },
    ) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "vault watcher: failed to create debouncer — not starting"
            );
            emit_watcher_error(
                &app_handle,
                &format!("Vault watcher failed to start: {}", e),
            );
            return;
        }
    };

    if let Err(e) = debouncer
        .watcher()
        .watch(&vault_root, RecursiveMode::Recursive)
    {
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

    // `debouncer` must stay in scope; dropping it stops the watch.
    while let Some(batch) = rx.recv().await {
        for event in batch {
            handle_event(
                event,
                db_pool.clone(),
                app_handle.clone(),
                suppression.clone(),
            );
        }
    }

    tracing::info!("vault watcher: event channel closed — exiting");
}

/// Spawn per-event so a bulk find-and-replace parallelizes instead of
/// serializing through one worker.
fn handle_event(
    event: DebouncedEvent,
    db_pool: SqlitePool,
    app_handle: tauri::AppHandle,
    suppression: WriteSuppressionRegistry,
) {
    for path in event.event.paths {
        // Filters out our `<id>.md.<uuid>.tmp` staging files (extension == tmp).
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        let pool = db_pool.clone();
        let handle = app_handle.clone();
        let supp = suppression.clone();

        tokio::spawn(async move {
            if supp.was_just_written(&path).await {
                tracing::debug!(
                    path = %path.display(),
                    "vault watcher: skipping our own write"
                );
                return;
            }

            // No `EventKind::Remove` fast-path: atomic saves
            // (rename-away/write-new) emit Remove for a file that's
            // back on disk before we'd read it. import_one's NotFound
            // fallback handles real deletes safely.
            import_one(&path, &pool, &handle).await;
        });
    }
}

/// Pub(crate) so focus-rescan reuses the same parse/UPSERT/emit path.
pub(crate) async fn import_one(path: &Path, db_pool: &SqlitePool, app_handle: &tauri::AppHandle) {
    let contents = match tokio::fs::read_to_string(path).await {
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
                return;
            }
            handle_external_delete(path, db_pool, app_handle).await;
            return;
        }
        Err(e) => {
            tracing::debug!(
                path = %path.display(),
                error = %e,
                "vault watcher: read failed — skipping"
            );
            return;
        }
    };

    let parsed = match super::parse::parse_note(&contents) {
        Ok(p) => p,
        Err(super::parse::ParseError::NotOurs) => {
            tracing::debug!(path = %path.display(), "vault watcher: untracked .md file — skipping");
            return;
        }
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "vault watcher: parse failed — skipping"
            );
            return;
        }
    };

    // Enforce that frontmatter id matches file stem — a copy of
    // `abc.md` to `duplicate.md` still has `id: abc` in frontmatter
    // and would overwrite the original on every rescan.
    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
        if parsed.id != stem {
            tracing::warn!(
                path = %path.display(),
                stem = %stem,
                frontmatter_id = %parsed.id,
                "vault watcher: id mismatch — skipping to prevent data corruption"
            );
            return;
        }
    }

    // ON CONFLICT: only the fields that round-trip through markdown.
    // Lattice-only JSON columns (linked_document_ids, highlights_json,
    // etc.) are preserved.
    let repository = DailyNotesRepository::new(db_pool.clone());
    let result = repository
        .upsert_from_vault(VaultNoteUpsert {
            id: &parsed.id,
            title: &parsed.title,
            content: &parsed.body,
            created_at: &parsed.created_at,
            updated_at: &parsed.updated_at,
        })
        .await;

    match result {
        Ok(()) => {
            tracing::info!(
                id = %parsed.id,
                path = %path.display(),
                "vault watcher: imported external edit"
            );
            emit_note_imported(app_handle, &parsed.id);
        }
        Err(e) => tracing::warn!(
            path = %path.display(),
            error = %e,
            "vault watcher: SQL upsert failed"
        ),
    }
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

    let repository = DailyNotesRepository::new(db_pool.clone());
    let result = repository.delete(id).await;

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

/// Catches edits the watcher dropped (sleep/wake, AV interference, etc.).
/// Idempotent. Frontend invokes on window focus.
pub async fn rescan_vault(
    db_pool: SqlitePool,
    vault_root: PathBuf,
    suppression: WriteSuppressionRegistry,
    app_handle: tauri::AppHandle,
) -> Result<RescanSummary, String> {
    let notes_dir = vault_root.join("notes");
    // repository-barrier-allow: rescan reconciles the configured vault resource with its repository.
    if !notes_dir.exists() {
        return Ok(RescanSummary {
            scanned: 0,
            imported: 0,
            deleted: 0,
        });
    }

    // Mutable: disk walk removes each id it sees; whatever remains
    // after the walk = file deleted from disk.
    let repository = DailyNotesRepository::new(db_pool.clone());
    let rows = repository
        .list_timestamps()
        .await
        .map_err(|e| format!("focus-rescan: SQL read failed: {}", e))?;

    let total_rows = rows.len();
    let mut sql_rows: HashMap<String, NoteTimestampRecord> =
        rows.into_iter().map(|r| (r.id.clone(), r)).collect();

    // repository-barrier-allow: rescan enumerates the configured vault resource.
    let mut entries = match tokio::fs::read_dir(&notes_dir).await {
        Ok(e) => e,
        Err(e) => return Err(format!("focus-rescan: read_dir failed: {}", e)),
    };

    let mut scanned = 0usize;
    let mut imported = 0usize;
    let mut deleted = 0usize;

    // A directory walk that ends early leaves rows in `sql_rows` that were
    // never matched against their files. Sweeping on a partial listing would
    // delete live notes, so track whether we actually reached the end.
    let mut walk_complete = false;

    loop {
        let entry = match entries.next_entry().await {
            Ok(Some(entry)) => entry,
            Ok(None) => {
                walk_complete = true;
                break;
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "focus-rescan: directory walk failed part-way; ghost-sweep will be skipped"
                );
                break;
            }
        };

        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        scanned += 1;

        if suppression.was_just_written(&path).await {
            // Still mark as seen so the ghost-sweep doesn't delete it.
            if let Some(id) = path.file_stem().and_then(|s| s.to_str()) {
                sql_rows.remove(id);
            }
            continue;
        }

        let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };

        let sql_row = sql_rows.remove(id);
        let needs_import = match sql_row.as_ref() {
            None => true,
            Some(row) => is_disk_newer_than_sql(&entry, &row.updated_at).await,
        };

        if needs_import {
            import_one(&path, &db_pool, &app_handle).await;
            imported += 1;
        }
    }

    // Ghost-note sweep — anything left in `sql_rows` had its file
    // deleted while Lattice wasn't watching. Skip newly-created rows
    // whose vault writeback hasn't flushed yet.
    // Before deleting anything, demand positive evidence that the vault is
    // actually intact. A present-but-empty `notes/` directory is NOT proof
    // the user deleted their notes — it is the normal appearance of a
    // Dropbox/iCloud folder whose contents haven't materialised, a partially
    // mounted volume, or a sync client mid-move. Treating it as authoritative
    // deletes the entire notes database on one window-focus rescan, with no
    // undo. The database is the SSOT; the filesystem only gets to *suggest*
    // deletions, and only when the evidence is coherent.
    if let Some(reason) =
        ghost_sweep_block_reason(walk_complete, scanned, sql_rows.len(), total_rows)
    {
        tracing::error!(
            scanned,
            candidates = sql_rows.len(),
            total_rows,
            reason,
            "focus-rescan: refusing ghost-sweep; vault looks incomplete rather than edited"
        );
        emit_rescan_blocked(&app_handle, reason, sql_rows.len(), total_rows);
        return Ok(RescanSummary {
            scanned,
            imported,
            deleted: 0,
        });
    }

    let now = chrono::Utc::now();
    let creation_grace = chrono::Duration::seconds(30);

    for (id, row) in sql_rows {
        // Try RFC-3339 first, then fall back to SQLite's native format
        // (`'YYYY-MM-DD HH:MM:SS'`). If we can't parse it, skip deletion
        // to be safe — better to keep a ghost row than delete a live one.
        let created_ok = parse_db_timestamp(&row.created_at);

        match created_ok {
            Ok(dt) => {
                let created_age = now.signed_duration_since(dt);
                if created_age < creation_grace {
                    tracing::debug!(
                        id = %id,
                        created_age_secs = created_age.num_seconds(),
                        "focus-rescan: skipping ghost-sweep for fresh row (writer queue may not have flushed)"
                    );
                    continue;
                }
            }
            Err(_) => {
                tracing::warn!(
                    id = %id,
                    created_at = %row.created_at,
                    "focus-rescan: unparseable created_at, skipping ghost-sweep to be safe"
                );
                continue;
            }
        }

        let result = repository.delete(&id).await;
        match result {
            Ok(true) => {
                tracing::info!(
                    id = %id,
                    "focus-rescan: deleted ghost note (file gone from vault)"
                );
                emit_note_imported(&app_handle, &id);
                deleted += 1;
            }
            Ok(false) => {}
            Err(e) => tracing::warn!(
                id = %id,
                error = %e,
                "focus-rescan: ghost-sweep DELETE failed"
            ),
        }
    }

    if imported > 0 || deleted > 0 {
        tracing::info!(scanned, imported, deleted, "focus-rescan complete");
    } else {
        tracing::debug!(scanned, "focus-rescan: no divergence");
    }

    Ok(RescanSummary {
        scanned,
        imported,
        deleted,
    })
}

/// Strict `>` on whole-second integer timestamps. Read failures
/// conservatively return true (treat as divergent).
async fn is_disk_newer_than_sql(entry: &tokio::fs::DirEntry, sql_updated_at: &str) -> bool {
    let Ok(meta) = entry.metadata().await else {
        return true;
    };
    let Ok(modified) = meta.modified() else {
        return true;
    };
    let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) else {
        return true;
    };

    let Ok(sql_dt) = parse_db_timestamp(sql_updated_at) else {
        return true;
    };
    // Strict `>` so a Lattice write (where SQL and disk land in the
    // same wall-clock second) doesn't re-import on next focus.
    // Same-second external edits are caught by the suppression registry.
    duration.as_secs() as i64 > sql_dt.timestamp()
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct RescanSummary {
    pub scanned: usize,
    pub imported: usize,
    pub deleted: usize,
}

fn emit_note_imported(app_handle: &tauri::AppHandle, note_id: &str) {
    use tauri::Emitter;
    let payload = serde_json::json!({ "noteId": note_id });
    if let Err(e) = app_handle.emit("vault:note-imported", payload) {
        tracing::warn!(error = %e, "failed to emit vault:note-imported event");
    }
}

/// Fraction of the notes table the ghost-sweep may delete in one pass before
/// we treat the vault as suspect rather than edited.
///
/// Deleting half your notes between two window focuses is not a plausible
/// editing session; it is what a half-synced cloud folder looks like.
const GHOST_SWEEP_MAX_DELETE_RATIO: f64 = 0.5;

/// Below this many rows, ratio checks are meaningless — deleting 1 of 2 notes
/// is both 50% and completely ordinary.
const GHOST_SWEEP_RATIO_MIN_ROWS: usize = 4;

/// Returns `Some(reason)` when the ghost-sweep must not run.
///
/// Split out from `rescan_vault` so the policy is unit-testable without a
/// database, a Tauri handle, or a real vault on disk.
fn ghost_sweep_block_reason(
    walk_complete: bool,
    scanned: usize,
    candidates: usize,
    total_rows: usize,
) -> Option<&'static str> {
    if candidates == 0 {
        return None; // Nothing to delete; nothing to guard against.
    }

    if !walk_complete {
        return Some("the vault directory listing did not complete");
    }

    if scanned == 0 {
        return Some("the notes directory is present but contains no files");
    }

    if total_rows >= GHOST_SWEEP_RATIO_MIN_ROWS {
        let ratio = candidates as f64 / total_rows as f64;
        if ratio >= GHOST_SWEEP_MAX_DELETE_RATIO {
            return Some("more than half the notes appear to be missing at once");
        }
    }

    None
}

/// Tell the frontend a rescan declined to delete, so the user finds out from
/// the UI rather than from noticing missing notes later.
fn emit_rescan_blocked(
    app_handle: &tauri::AppHandle,
    reason: &str,
    candidates: usize,
    total_rows: usize,
) {
    use tauri::Emitter;
    let payload = serde_json::json!({
        "reason": reason,
        "candidates": candidates,
        "totalRows": total_rows,
    });
    if let Err(e) = app_handle.emit("vault:rescan-blocked", payload) {
        tracing::warn!(error = %e, "failed to emit vault:rescan-blocked event");
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
mod tests {
    use super::*;

    #[test]
    fn ghost_sweep_allows_ordinary_deletion() {
        // 10 notes on disk, 1 row unmatched — a normal delete.
        assert_eq!(ghost_sweep_block_reason(true, 10, 1, 11), None);
    }

    #[test]
    fn ghost_sweep_allows_everything_when_nothing_to_delete() {
        // Empty vault with an empty table is not suspicious.
        assert_eq!(ghost_sweep_block_reason(true, 0, 0, 0), None);
    }

    #[test]
    fn ghost_sweep_blocks_on_empty_but_present_directory() {
        // The cloud-placeholder case: directory exists, no files materialised,
        // every row looks like a ghost.
        assert!(ghost_sweep_block_reason(true, 0, 25, 25).is_some());
    }

    #[test]
    fn ghost_sweep_blocks_on_incomplete_walk() {
        // A read error part-way through must never authorise deletions.
        assert!(ghost_sweep_block_reason(false, 12, 3, 15).is_some());
    }

    #[test]
    fn ghost_sweep_blocks_on_mass_disappearance() {
        // Half the corpus vanished between two focus events — not an edit.
        assert!(ghost_sweep_block_reason(true, 10, 10, 20).is_some());
    }

    #[test]
    fn ghost_sweep_ratio_does_not_apply_to_tiny_vaults() {
        // 1 of 2 notes deleted is 50%, but entirely ordinary.
        assert_eq!(ghost_sweep_block_reason(true, 1, 1, 2), None);
    }

    #[tokio::test]
    async fn suppression_marks_and_clears_by_filename() {
        let reg = WriteSuppressionRegistry::new();
        let path_a = PathBuf::from("/tmp/abc-123.md");
        assert!(!reg.was_just_written(&path_a).await);
        reg.mark_written(path_a.clone()).await;
        assert!(reg.was_just_written(&path_a).await);
    }

    #[tokio::test]
    async fn suppression_keys_by_filename_across_path_shapes() {
        let reg = WriteSuppressionRegistry::new();
        let written = PathBuf::from("/Users/me/Lattice/notes/abc-123.md");
        let observed = PathBuf::from("/Users/Me/lattice/notes/abc-123.md");
        reg.mark_written(written).await;
        assert!(reg.was_just_written(&observed).await);
    }

    #[tokio::test]
    async fn suppression_is_negative_for_unknown_path() {
        let reg = WriteSuppressionRegistry::new();
        let path = PathBuf::from("/tmp/never-written.md");
        assert!(!reg.was_just_written(&path).await);
    }

    #[tokio::test]
    async fn suppression_marking_prunes_expired_entries() {
        let reg = WriteSuppressionRegistry::new();
        {
            let mut guard = reg.inner.lock().await;
            let stale = Instant::now()
                .checked_sub(SUPPRESSION_TTL + Duration::from_secs(1))
                .expect("clock can roll back this far in test");
            guard.insert("expired.md".to_string(), stale);
        }
        reg.mark_written(PathBuf::from("/tmp/fresh.md")).await;
        let guard = reg.inner.lock().await;
        assert!(!guard.contains_key("expired.md"));
        assert!(guard.contains_key("fresh.md"));
    }

    #[test]
    fn filename_key_extracts_basename() {
        assert_eq!(filename_key(Path::new("/a/b/c.md")).unwrap(), "c.md");
    }
}
