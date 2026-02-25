//! Extraction Plugin
//!
//! Tauri commands for wikilink parsing, title extraction, and link resolution.
//! Routes to interfaces/commands/domains/extraction.rs implementations.

use crate::{
    application::dtos::extraction_dto::DocumentRefDto,
    interfaces::{
        commands::extraction::{self, ParsedLinksResponse, ResolveLinkResponse},
        di::Container,
    },
    shared::api_result::ApiError,
};
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
};

#[tauri::command]
#[specta::specta]
pub async fn parse_wikilinks(
    text: String,
    source_path: Option<String>,
    container: State<'_, Container>,
) -> Result<ParsedLinksResponse, ApiError> {
    extraction::parse_wikilinks(text, source_path, container)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn extract_document_title(
    content: String,
    container: State<'_, Container>,
) -> Result<Option<String>, ApiError> {
    extraction::extract_document_title(content, container)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn resolve_wikilink(
    target: String,
    source_path: String,
    available_documents: Vec<DocumentRefDto>,
    container: State<'_, Container>,
) -> Result<ResolveLinkResponse, ApiError> {
    extraction::resolve_wikilink(target, source_path, available_documents, container)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn extract_and_resolve_links(
    document_id: String,
    content: String,
    container: State<'_, Container>,
) -> Result<extraction::ExtractAndResolveResponseDto, ApiError> {
    extraction::extract_and_resolve_links(document_id, content, container)
        .await
        .map_err(ApiError::from)
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("extraction")
        .invoke_handler(tauri::generate_handler![
            parse_wikilinks,
            extract_document_title,
            resolve_wikilink,
            extract_and_resolve_links,
        ])
        .build()
}
