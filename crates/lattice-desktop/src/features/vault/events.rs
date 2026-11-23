//! Every event the vault sends to the window.
use std::path::Path;

use super::watcher::AppHandleCell;

fn emit(app_handle: &AppHandleCell, name: &str, payload: serde_json::Value) {
    use tauri::Emitter;
    let guard = match app_handle.read() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    let Some(handle) = guard.as_ref() else {
        return;
    };
    if let Err(error) = handle.emit(name, payload) {
        tracing::warn!(%error, event = name, "failed to emit a vault event");
    }
}

pub(crate) fn emit_note_imported(app_handle: &AppHandleCell, note_id: &str) {
    emit(
        app_handle,
        "vault:note-imported",
        serde_json::json!({ "noteId": note_id }),
    );
}

/// SQL saved the note but its file could not be written.
pub(crate) fn emit_write_error(
    app_handle: &AppHandleCell,
    note_id: Option<&str>,
    target: &Path,
    error: &str,
) {
    emit(
        app_handle,
        "vault:write-error",
        serde_json::json!({
            "noteId": note_id,
            "target": target.display().to_string(),
            "error": error,
        }),
    );
}

pub(crate) fn emit_watcher_error(app_handle: &AppHandleCell, error: &str) {
    emit(
        app_handle,
        "vault:watcher-error",
        serde_json::json!({ "error": error }),
    );
}

/// Tell the frontend a rescan declined to delete, so the user finds out from
/// the UI rather than from noticing missing notes later.
pub(crate) fn emit_rescan_blocked(
    app_handle: &AppHandleCell,
    reason: &str,
    candidates: usize,
    total_rows: usize,
) {
    emit(
        app_handle,
        "vault:rescan-blocked",
        serde_json::json!({
            "reason": reason,
            "candidates": candidates,
            "totalRows": total_rows,
        }),
    );
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum VaultConflictDto {
    #[serde(rename_all = "camelCase")]
    KeptCopy {
        id: String,
        note_id: String,
        note_title: String,
        copy_id: String,
        copy_title: String,
    },
    #[serde(rename_all = "camelCase")]
    SetAside {
        id: String,
        note_id: String,
        note_title: String,
        set_aside_path: String,
    },
}

pub(crate) fn emit_note_conflict(app_handle: &AppHandleCell, conflict: &VaultConflictDto) {
    match serde_json::to_value(conflict) {
        Ok(payload) => emit(app_handle, "vault:note-conflict", payload),
        Err(error) => tracing::warn!(%error, "failed to serialize vault:note-conflict event"),
    }
}
