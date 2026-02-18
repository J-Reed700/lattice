//! Daily Notes Workspace Plugin
//!
//! Provides persistent SQLite-backed CRUD operations for the Daily Notes workspace.

use crate::interfaces::commands::daily_notes_workspace::{
    create_workspace_note_impl, delete_workspace_note_impl, get_daily_notes_range_impl,
    get_next_daily_note_impl, get_previous_daily_note_impl, get_today_note_impl,
    list_workspace_notes_impl, quick_capture_impl, update_daily_note_content_impl,
    update_workspace_note_impl, CreateWorkspaceNoteRequestDto, DailyNoteCompatDto,
    DailyNoteCursorRequestDto, DailyNotesRangeRequestDto, DeleteWorkspaceNoteRequestDto,
    ListWorkspaceNotesResponseDto, UpdateDailyNoteContentRequestDto, WorkspaceNoteDto,
};
use crate::interfaces::di::Container;
use crate::shared::api_result::ApiError;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
};

#[tauri::command]
#[specta::specta]
pub async fn list_workspace_notes(
    container: State<'_, Container>,
) -> Result<ListWorkspaceNotesResponseDto, ApiError> {
    let notes = list_workspace_notes_impl(container.inner())
        .await
        .map_err(ApiError::from)?;
    Ok(ListWorkspaceNotesResponseDto { notes })
}

#[tauri::command]
#[specta::specta]
pub async fn create_workspace_note(
    request: CreateWorkspaceNoteRequestDto,
    container: State<'_, Container>,
) -> Result<WorkspaceNoteDto, ApiError> {
    create_workspace_note_impl(container.inner(), request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn update_workspace_note(
    note: WorkspaceNoteDto,
    container: State<'_, Container>,
) -> Result<WorkspaceNoteDto, ApiError> {
    update_workspace_note_impl(container.inner(), note)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_workspace_note(
    request: DeleteWorkspaceNoteRequestDto,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    delete_workspace_note_impl(container.inner(), request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_today_note(
    container: State<'_, Container>,
) -> Result<DailyNoteCompatDto, ApiError> {
    get_today_note_impl(container.inner())
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn quick_capture(
    content: String,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    quick_capture_impl(container.inner(), content)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_daily_notes_range(
    request: DailyNotesRangeRequestDto,
    container: State<'_, Container>,
) -> Result<Vec<DailyNoteCompatDto>, ApiError> {
    get_daily_notes_range_impl(container.inner(), request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_previous_daily_note(
    request: DailyNoteCursorRequestDto,
    container: State<'_, Container>,
) -> Result<Option<DailyNoteCompatDto>, ApiError> {
    get_previous_daily_note_impl(container.inner(), request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_next_daily_note(
    request: DailyNoteCursorRequestDto,
    container: State<'_, Container>,
) -> Result<Option<DailyNoteCompatDto>, ApiError> {
    get_next_daily_note_impl(container.inner(), request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn update_daily_note_content(
    request: UpdateDailyNoteContentRequestDto,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    update_daily_note_content_impl(container.inner(), request)
        .await
        .map_err(ApiError::from)
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("dailynotes")
        .invoke_handler(tauri::generate_handler![
            get_today_note,
            quick_capture,
            get_daily_notes_range,
            get_previous_daily_note,
            get_next_daily_note,
            update_daily_note_content,
            list_workspace_notes,
            create_workspace_note,
            update_workspace_note,
            delete_workspace_note,
        ])
        .build()
}
