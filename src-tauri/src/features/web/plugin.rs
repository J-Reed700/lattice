//! Web Plugin
//!
//! Web ingestion plugin for web content commands.

use crate::features::web::snapshots::{commands as snapshots, WebPageSnapshotDto};
use crate::{features::web::commands as web_ingest, interfaces::di::Container};
use lattice_core::shared::ipc::ApiError;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime, State,
};

#[tauri::command]
#[specta::specta]
pub async fn ingest_web_url(
    url: String,
    space_id: Option<String>,
    conversation_id: Option<String>,
    container: State<'_, Container>,
) -> Result<web_ingest::WebIngestResponse, ApiError> {
    web_ingest::ingest_web_url(url, space_id, conversation_id, container)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn fetch_url_preview(
    url: String,
    container: State<'_, Container>,
) -> Result<lattice_core::application::contracts::web::UrlPreview, ApiError> {
    web_ingest::fetch_url_preview(url, container)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn extract_article(
    url: String,
    container: State<'_, Container>,
) -> Result<lattice_core::application::contracts::web::CleanArticle, ApiError> {
    web_ingest::extract_article(url, container)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn read_web_page(
    url: String,
    container: State<'_, Container>,
) -> Result<web_ingest::WebPageDto, ApiError> {
    web_ingest::read_web_page(url, container)
        .await
        .map_err(ApiError::from)
}

/// The frozen copy of `url` in this conversation, if one was ever requested.
#[tauri::command]
#[specta::specta]
pub async fn get_web_page_snapshot(
    conversation_id: String,
    url: String,
    container: State<'_, Container>,
) -> Result<Option<WebPageSnapshotDto>, ApiError> {
    snapshots::get_web_page_snapshot(&container.web_page_snapshots(), &conversation_id, &url)
        .await
        .map_err(ApiError::from)
}

/// Capture a frozen copy now, or try a failed one again.
#[tauri::command]
#[specta::specta]
pub async fn capture_web_page_snapshot(
    conversation_id: String,
    url: String,
    container: State<'_, Container>,
) -> Result<WebPageSnapshotDto, ApiError> {
    snapshots::capture_web_page_snapshot(&container.web_page_snapshots(), &conversation_id, &url)
        .await
        .map_err(ApiError::from)
}

/// Every frozen copy this conversation has.
#[tauri::command]
#[specta::specta]
pub async fn list_web_page_snapshots(
    conversation_id: String,
    container: State<'_, Container>,
) -> Result<Vec<WebPageSnapshotDto>, ApiError> {
    snapshots::list_web_page_snapshots(&container.web_page_snapshots(), &conversation_id)
        .await
        .map_err(ApiError::from)
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("web")
        .setup(|app, _api| {
            let container = app.state::<Container>().inner().clone();
            // Registration settles snapshot jobs the last process left
            // running, then starts delivering saved work.
            tauri::async_runtime::block_on(container.web_page_snapshots().register_job())?;
            // The scheme handler resolves the service from managed state (not the container).
            app.manage(container.web_page_snapshots());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ingest_web_url,
            fetch_url_preview,
            extract_article,
            read_web_page,
            get_web_page_snapshot,
            capture_web_page_snapshot,
            list_web_page_snapshots,
        ])
        .build()
}
