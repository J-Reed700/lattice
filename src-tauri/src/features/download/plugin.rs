//! Download Plugin - Download lifecycle management commands
//!
//! Thin plugin wrapper over `interfaces::commands::downloads`.

use crate::features::download::commands::{
    self, DownloadCommandState, DownloadStatusResponse, StartDownloadRequest,
};
use crate::features::download::vision_projector::{
    start_vision_projector_download_impl, VisionProjectorDownloadDto,
};
use crate::interfaces::di::Container;
use lattice_core::shared::ipc::ApiError;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
};

#[tauri::command]
#[specta::specta]
pub async fn start_model_download(
    state: State<'_, DownloadCommandState>,
    request: StartDownloadRequest,
) -> Result<String, ApiError> {
    commands::start_model_download(state, request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn pause_download(
    state: State<'_, DownloadCommandState>,
    id: String,
) -> Result<(), ApiError> {
    commands::pause_download(state, id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn resume_download(
    state: State<'_, DownloadCommandState>,
    id: String,
) -> Result<(), ApiError> {
    commands::resume_download(state, id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_download(
    state: State<'_, DownloadCommandState>,
    id: String,
) -> Result<(), ApiError> {
    commands::cancel_download(state, id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn retry_download(
    state: State<'_, DownloadCommandState>,
    id: String,
) -> Result<(), ApiError> {
    commands::retry_download(state, id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn remove_download(
    state: State<'_, DownloadCommandState>,
    id: String,
) -> Result<(), ApiError> {
    commands::remove_download(state, id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn clear_completed_downloads(
    state: State<'_, DownloadCommandState>,
) -> Result<usize, ApiError> {
    commands::clear_completed_downloads(state)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_download_status(
    state: State<'_, DownloadCommandState>,
    id: String,
) -> Result<Option<DownloadStatusResponse>, ApiError> {
    commands::get_download_status(state, id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn list_downloads(
    state: State<'_, DownloadCommandState>,
) -> Result<Vec<DownloadStatusResponse>, ApiError> {
    commands::list_downloads(state)
        .await
        .map_err(ApiError::from)
}

/// Start downloading the catalog vision projector for the downloaded model
/// `id`, beside its weights. Follow it like any download, by the returned id.
#[tauri::command]
#[specta::specta]
pub async fn start_vision_projector_download(
    container: State<'_, Container>,
    id: String,
) -> Result<VisionProjectorDownloadDto, ApiError> {
    let manager = container.download_manager().map_err(ApiError::from)?;
    let repository = container.downloaded_model_repository();
    start_vision_projector_download_impl(manager.as_ref(), &repository, &id)
        .await
        .map_err(ApiError::from)
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("download")
        .invoke_handler(tauri::generate_handler![
            start_model_download,
            pause_download,
            resume_download,
            cancel_download,
            retry_download,
            remove_download,
            clear_completed_downloads,
            get_download_status,
            list_downloads,
            start_vision_projector_download,
        ])
        .build()
}
