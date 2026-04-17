//! Web Plugin
//!
//! Web ingestion plugin for web content commands.

use crate::{
    interfaces::{commands::web_ingest, di::Container},
    shared::api_result::ApiError,
};
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
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
) -> Result<crate::features::function_calling::dto::UrlPreview, ApiError> {
    web_ingest::fetch_url_preview(url, container)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn extract_article(
    url: String,
    container: State<'_, Container>,
) -> Result<crate::features::function_calling::dto::CleanArticle, ApiError> {
    web_ingest::extract_article(url, container)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn reindex_web_archive(container: State<'_, Container>) -> Result<usize, ApiError> {
    web_ingest::reindex_web_archive(container)
        .await
        .map_err(ApiError::from)
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("web")
        .invoke_handler(tauri::generate_handler![
            ingest_web_url,
            fetch_url_preview,
            extract_article,
            reindex_web_archive,
        ])
        .build()
}
