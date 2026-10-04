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
use crate::shared::persistence::timestamps::parse_db_timestamp;

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
                emit_watcher_error(&app_handle, &error);
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

    let repository = DailyNotesRepository::new(db_pool.clone());
    const DB_PAGE: i64 = 128;
    let _rescan_lock = RESCAN_LOCK.get_or_init(|| Mutex::new(())).lock().await;
    let (run_id, total_rows) = super::repository::begin_rescan(&db_pool)
        .await
        .map_err(|e| format!("focus-rescan: repository setup failed: {e}"))?;
    let mut membership_guard = RescanMembershipGuard {
        pool: db_pool.clone(),
        run_id: run_id.clone(),
        active: true,
    };

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

        let stem = path.file_stem().and_then(|s| s.to_str());
        if let Some(id) = stem {
            super::repository::mark_rescan_seen(&db_pool, &run_id, id)
                .await
                .map_err(|e| format!("focus-rescan: could not mark note as seen: {e}"))?;
            if super::repository::has_pending(&db_pool, id)
                .await
                .unwrap_or(true)
            {
                continue;
            }
        }

        if suppression.was_just_written(&path).await {
            // Still mark as seen so the ghost-sweep doesn't delete it.
            continue;
        }

        let Some(id) = stem else {
            continue;
        };

        let sql_row = repository
            .get_timestamp(id)
            .await
            .map_err(|e| format!("focus-rescan: timestamp lookup failed: {e}"))?;
        let needs_import = match sql_row {
            None => true,
            Some(row) => is_disk_newer_than_sql(&entry, &row.updated_at).await,
        };

        if needs_import {
            if let Some(imported_id) = import_one(&path, &db_pool, &app_handle).await {
                super::repository::mark_rescan_seen(&db_pool, &run_id, &imported_id)
                    .await
                    .map_err(|e| {
                        format!("focus-rescan: could not mark imported note as seen: {e}")
                    })?;
            }
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
    // Count eligible missing rows in keyset pages before deleting any. This
    // preserves the mass-disappearance guard while keeping memory bounded.
    let mut candidates = 0usize;
    let mut cursor: Option<String> = None;
    loop {
        let rows =
            super::repository::rescan_note_page(&db_pool, &run_id, cursor.as_deref(), DB_PAGE)
                .await
                .map_err(|e| format!("focus-rescan: ghost candidate page failed: {e}"))?;
        if rows.is_empty() {
            break;
        }
        let Some(last) = rows.last() else { break };
        cursor = Some(last.id.clone());
        for row in rows {
            if super::repository::has_pending(&db_pool, &row.id)
                .await
                .unwrap_or(true)
            {
                continue;
            }
            if parse_db_timestamp(&row.created_at).is_ok_and(|dt| {
                chrono::Utc::now().signed_duration_since(dt) >= chrono::Duration::seconds(30)
            }) {
                candidates += 1;
            }
        }
    }
    if let Some(reason) =
        ghost_sweep_block_reason(walk_complete, scanned, candidates, total_rows as usize)
    {
        tracing::error!(
            scanned,
            candidates,
            total_rows,
            reason,
            "focus-rescan: refusing ghost-sweep; vault looks incomplete rather than edited"
        );
        emit_rescan_blocked(&app_handle, reason, candidates, total_rows as usize);
        let _ = super::repository::finish_rescan(&db_pool, &run_id).await;
        membership_guard.active = false;
        return Ok(RescanSummary {
            scanned,
            imported,
            deleted: 0,
        });
    }

    let now = chrono::Utc::now();
    let creation_grace = chrono::Duration::seconds(30);

    cursor = None;
    loop {
        let rows =
            super::repository::rescan_note_page(&db_pool, &run_id, cursor.as_deref(), DB_PAGE)
                .await
                .map_err(|e| format!("focus-rescan: ghost deletion page failed: {e}"))?;
        if rows.is_empty() {
            break;
        }
        let Some(last) = rows.last() else { break };
        cursor = Some(last.id.clone());
        for row in rows {
            if !parse_db_timestamp(&row.created_at)
                .is_ok_and(|dt| now.signed_duration_since(dt) >= creation_grace)
            {
                continue;
            }
            let Some(note_id) = super::writeback::safe_note_id(&row.id).then_some(row.id.as_str())
            else {
                continue;
            };
            // repository-barrier-allow: focus rescan checks the mirrored note file before ghost deletion.
            if tokio::fs::try_exists(notes_dir.join(format!("{note_id}.md")))
                .await
                .unwrap_or(true)
            {
                continue;
            }
            match repository.delete_from_vault_if_unqueued(&row.id).await {
                Ok(true) => {
                    tracing::info!(id = %row.id, "focus-rescan: deleted ghost note (file gone from vault)");
                    emit_note_imported(&app_handle, &row.id);
                    deleted += 1;
                }
                Ok(false) => {}
                Err(e) => {
                    tracing::warn!(id = %row.id, error = %e, "focus-rescan ghost DELETE failed")
                }
            }
        }
    }

    super::repository::finish_rescan(&db_pool, &run_id)
        .await
        .map_err(|e| format!("focus-rescan: membership cleanup failed: {e}"))?;
    membership_guard.active = false;

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

struct RescanMembershipGuard {
    pool: SqlitePool,
    run_id: String,
    active: bool,
}

impl Drop for RescanMembershipGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let pool = self.pool.clone();
        let run_id = self.run_id.clone();
        let _ = crate::shared::runtime::background::spawn(async move {
            let _ = crate::features::vault::repository::finish_rescan(&pool, &run_id).await;
        });
    }
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

    #[tokio::test]
    async fn oversized_vault_note_is_rejected_without_reading_the_whole_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("oversized.md");
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_NOTE_BYTES + 1024).unwrap();
        assert_eq!(
            read_note(&path).await.unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
    }

    fn note(id: &str) -> String {
        format!("---\nid: {id}\ntitle: \"t\"\ncreated_at: t\nupdated_at: t\ntags: []\n---\n\nbody")
    }

    #[tokio::test]
    async fn a_note_renamed_in_finder_is_found_under_its_new_name() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("other.md"), note("other")).unwrap();
        std::fs::write(dir.path().join("My title.md"), note("abc")).unwrap();
        let removed = dir.path().join("abc.md");

        assert_eq!(
            find_renamed_note(&removed, "abc").await,
            Some(dir.path().join("My title.md"))
        );
        assert_eq!(find_renamed_note(&removed, "gone").await, None);
    }

    #[tokio::test]
    async fn a_renamed_file_is_importable_but_a_copy_is_not() {
        let dir = tempfile::tempdir().unwrap();
        let renamed = dir.path().join("My title.md");
        std::fs::write(&renamed, note("abc")).unwrap();
        assert!(is_importable_name(&renamed, "abc").await);
        assert!(is_importable_name(&dir.path().join("abc.md"), "abc").await);

        // The original is still there: this is a copy, not a rename.
        std::fs::write(dir.path().join("abc.md"), note("abc")).unwrap();
        assert!(!is_importable_name(&renamed, "abc").await);
    }

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
