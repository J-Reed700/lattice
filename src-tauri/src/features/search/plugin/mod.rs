//! Search Plugin
//!
//! Registers search commands so the frontend can invoke them via plugin:search|command.
//! This plugin registers search commands from interfaces/commands.

pub mod commands;

use crate::features::search::commands as search_commands;
use crate::features::search::reranker_setup::{
    download_reranker_impl, reranker_status_impl, RerankerStatusDto,
};
use crate::interfaces::di::Container;
use crate::shared::api_result::ApiError;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
};

/// Report whether reranking is switched on and whether its model is present.
#[tauri::command]
#[specta::specta]
pub async fn reranker_status(
    container: State<'_, Container>,
) -> Result<RerankerStatusDto, ApiError> {
    reranker_status_impl(container.inner())
        .await
        .map_err(ApiError::from)
}

/// Download the reranker artifacts. Does not switch reranking on.
#[tauri::command]
#[specta::specta]
pub async fn download_reranker(
    container: State<'_, Container>,
) -> Result<RerankerStatusDto, ApiError> {
    download_reranker_impl(container.inner())
        .await
        .map_err(ApiError::from)
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("search")
        .invoke_handler(tauri::generate_handler![
            search_commands::search_documents,
            search_commands::search_fast,
            search_commands::semantic_search,
            search_commands::hybrid_search,
            search_commands::find_similar,
            search_commands::find_similar_documents,
            search_commands::search_with_recency,
            search_commands::batch_search,
            reranker_status,
            download_reranker,
        ])
        .build()
}
