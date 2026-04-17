//! Download Plugin - Download lifecycle management commands
//!
//! Thin plugin wrapper over `interfaces::commands::downloads`.

use crate::features::download::commands::{
    self, DownloadCommandState, DownloadStatusResponse, StartDownloadRequest,
};
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
};

#[tauri::command]
#[specta::specta]
pub async fn start_model_download(
    state: State<'_, DownloadCommandState>,
    request: StartDownloadRequest,
) -> Result<String, String> {
    commands::start_model_download(state, request).await
}

#[tauri::command]
#[specta::specta]
pub async fn pause_download(
    state: State<'_, DownloadCommandState>,
    id: String,
) -> Result<(), String> {
    commands::pause_download(state, id).await
}

#[tauri::command]
#[specta::specta]
pub async fn resume_download(
    state: State<'_, DownloadCommandState>,
    id: String,
) -> Result<(), String> {
    commands::resume_download(state, id).await
}

#[tauri::command]
#[specta::specta]
pub async fn download_cancel(
    state: State<'_, DownloadCommandState>,
    id: String,
) -> Result<(), String> {
    commands::cancel_download(state, id).await
}

#[tauri::command]
#[specta::specta]
pub async fn retry_download(
    state: State<'_, DownloadCommandState>,
    id: String,
) -> Result<(), String> {
    commands::retry_download(state, id).await
}

#[tauri::command]
#[specta::specta]
pub async fn remove_download(
    state: State<'_, DownloadCommandState>,
    id: String,
) -> Result<(), String> {
    commands::remove_download(state, id).await
}

#[tauri::command]
#[specta::specta]
pub async fn clear_completed_downloads(
    state: State<'_, DownloadCommandState>,
) -> Result<usize, String> {
    commands::clear_completed_downloads(state).await
}

#[tauri::command]
#[specta::specta]
pub async fn download_get_status(
    state: State<'_, DownloadCommandState>,
    id: String,
) -> Result<Option<DownloadStatusResponse>, String> {
    commands::get_download_status(state, id).await
}

#[tauri::command]
#[specta::specta]
pub async fn list_downloads(
    state: State<'_, DownloadCommandState>,
) -> Result<Vec<DownloadStatusResponse>, String> {
    commands::list_downloads(state).await
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("download")
        .invoke_handler(tauri::generate_handler![
            start_model_download,
            pause_download,
            resume_download,
            download_cancel,
            retry_download,
            remove_download,
            clear_completed_downloads,
            download_get_status,
            list_downloads,
        ])
        .build()
}
