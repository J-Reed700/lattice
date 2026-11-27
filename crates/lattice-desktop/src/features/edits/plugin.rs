//! Edits plugin: unified IPC commands backed by `ContentEdits`.

use crate::interfaces::di::Container;
use lattice_core::shared::edits::{
    ContentEditHistory, ContentEditOutcome, ContentEditPreview, ContentEditQuery,
    ContentEditRequest, ContentEditSummary, ContentRef, EditableContent, RevertContentEditRequest,
};
use lattice_core::shared::ipc::ApiError;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
};

#[tauri::command]
#[specta::specta]
pub async fn open_content_edit(
    content: ContentRef,
    container: State<'_, Container>,
) -> Result<EditableContent, ApiError> {
    Ok(container.content_edits().open(&content).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn preview_content_edit(
    request: ContentEditRequest,
    container: State<'_, Container>,
) -> Result<ContentEditPreview, ApiError> {
    Ok(container.content_edits().preview(&request).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn apply_content_edit(
    request: ContentEditRequest,
    container: State<'_, Container>,
) -> Result<ContentEditOutcome, ApiError> {
    Ok(container.content_edits().apply(&request).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn revert_content_edit(
    request: RevertContentEditRequest,
    container: State<'_, Container>,
) -> Result<ContentEditOutcome, ApiError> {
    Ok(container.content_edits().revert(&request).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn get_content_edit_history(
    content: ContentRef,
    container: State<'_, Container>,
) -> Result<ContentEditHistory, ApiError> {
    Ok(container.content_edits().history(&content).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn list_content_edits(
    query: ContentEditQuery,
    container: State<'_, Container>,
) -> Result<Vec<ContentEditSummary>, ApiError> {
    Ok(container.content_edits().list(&query).await?)
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("edits")
        .invoke_handler(tauri::generate_handler![
            open_content_edit,
            preview_content_edit,
            apply_content_edit,
            revert_content_edit,
            get_content_edit_history,
            list_content_edits,
        ])
        .build()
}
