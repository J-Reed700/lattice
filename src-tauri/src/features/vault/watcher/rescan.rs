//! The full vault rescan, and the guard that stops it deleting notes when the vault looks half-synced.

use std::path::PathBuf;
use std::sync::Arc;

use sqlx::SqlitePool;

use crate::shared::error::{AppError, Result};
use crate::shared::persistence::timestamps::parse_db_timestamp;
use crate::shared::runtime::background::BackgroundTasks;

use super::{
    emit_note_imported, import_one, DailyNotesRepository, RescanLock, WriteSuppressionRegistry,
};

/// Catches edits the watcher dropped (sleep/wake, AV interference, etc.).
/// Idempotent. Frontend invokes on window focus.
pub async fn rescan_vault(
    db_pool: SqlitePool,
    vault_root: PathBuf,
    suppression: WriteSuppressionRegistry,
    rescan_lock: &RescanLock,
    app_handle: tauri::AppHandle,
    background: Arc<BackgroundTasks>,
) -> Result<RescanSummary> {
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
    let _rescan_lock = rescan_lock.0.lock().await;
    let (run_id, total_rows) = super::super::repository::begin_rescan(&db_pool)
        .await
        .map_err(|e| e.with_context("focus-rescan: repository setup failed"))?;
    let mut membership_guard = RescanMembershipGuard {
        pool: db_pool.clone(),
        run_id: run_id.clone(),
        active: true,
        background,
    };

    // repository-barrier-allow: rescan enumerates the configured vault resource.
    let mut entries = match tokio::fs::read_dir(&notes_dir).await {
        Ok(e) => e,
        Err(e) => {
            return Err(AppError::FileSystem(format!(
                "focus-rescan: read_dir failed: {}",
                e
            )))
        }
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
            super::super::repository::mark_rescan_seen(&db_pool, &run_id, id)
                .await
                .map_err(|e| e.with_context("focus-rescan: could not mark note as seen"))?;
            if super::super::repository::has_pending(&db_pool, id)
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
            .map_err(|e| e.with_context("focus-rescan: timestamp lookup failed"))?;
        let needs_import = match sql_row {
            None => true,
            Some(row) => is_disk_newer_than_sql(&entry, &row.updated_at).await,
        };

        if needs_import {
            if let Some(imported_id) = import_one(&path, &db_pool, &app_handle).await {
                super::super::repository::mark_rescan_seen(&db_pool, &run_id, &imported_id)
                    .await
                    .map_err(|e| {
                        e.with_context("focus-rescan: could not mark imported note as seen")
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
        let rows = super::super::repository::rescan_note_page(
            &db_pool,
            &run_id,
            cursor.as_deref(),
            DB_PAGE,
        )
        .await
        .map_err(|e| e.with_context("focus-rescan: ghost candidate page failed"))?;
        if rows.is_empty() {
            break;
        }
        let Some(last) = rows.last() else { break };
        cursor = Some(last.id.clone());
        for row in rows {
            if super::super::repository::has_pending(&db_pool, &row.id)
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
        let _ = super::super::repository::finish_rescan(&db_pool, &run_id).await;
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
        let rows = super::super::repository::rescan_note_page(
            &db_pool,
            &run_id,
            cursor.as_deref(),
            DB_PAGE,
        )
        .await
        .map_err(|e| e.with_context("focus-rescan: ghost deletion page failed"))?;
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
            let Some(note_id) =
                super::super::writeback::safe_note_id(&row.id).then_some(row.id.as_str())
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

    super::super::repository::finish_rescan(&db_pool, &run_id)
        .await
        .map_err(|e| e.with_context("focus-rescan: membership cleanup failed"))?;
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
    /// Where the release runs when the guard drops mid-scan.
    background: Arc<BackgroundTasks>,
}

impl Drop for RescanMembershipGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let pool = self.pool.clone();
        let run_id = self.run_id.clone();
        let _ = self.background.spawn(async move {
            let _ = crate::features::vault::repository::finish_rescan(&pool, &run_id).await;
        });
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
pub(super) fn ghost_sweep_block_reason(
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
