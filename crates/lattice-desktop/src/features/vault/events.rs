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

pub(crate) enum NoteConflict<'a> {
    KeptCopy {
        note_id: &'a str,
        note_title: &'a str,
        copy_id: &'a str,
        copy_title: &'a str,
    },
    SetAside {
        note_id: &'a str,
        note_title: &'a str,
        set_aside_path: &'a Path,
    },
}

pub(crate) fn emit_note_conflict(app_handle: &AppHandleCell, conflict: NoteConflict<'_>) {
    let payload = match conflict {
        NoteConflict::KeptCopy {
            note_id,
            note_title,
            copy_id,
            copy_title,
        } => serde_json::json!({
            "kind": "keptCopy",
            "noteId": note_id,
            "noteTitle": note_title,
            "copyId": copy_id,
            "copyTitle": copy_title,
        }),
        NoteConflict::SetAside {
            note_id,
            note_title,
            set_aside_path,
        } => serde_json::json!({
            "kind": "setAside",
            "noteId": note_id,
            "noteTitle": note_title,
            "setAsidePath": set_aside_path.display().to_string(),
        }),
    };
    emit(app_handle, "vault:note-conflict", payload);
}
