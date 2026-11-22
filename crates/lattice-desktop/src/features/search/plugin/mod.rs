//! Search Plugin
//!
//! Registers search commands so the frontend can invoke them via plugin:search|command.
//! This plugin registers search commands from interfaces/commands.

pub mod commands;

use crate::features::search::commands as search_commands;
use crate::features::search::reranker_setup::{
    download_reranker_impl, remove_reranker_impl, reranker_status_impl, RerankerStatusDto,
};
use crate::interfaces::di::Container;
use lattice_core::shared::ipc::ApiError;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
};

/// Report whether the reranker's model is present, and so whether search reranks.
#[tauri::command]
#[specta::specta]
pub async fn reranker_status(
    container: State<'_, Container>,
) -> Result<RerankerStatusDto, ApiError> {
    Ok(reranker_status_impl(&container.models_path()))
}

/// Start downloading the reranker's model through the shared download
/// manager, after which search reranks. Follow it in the Downloads list.
#[tauri::command]
#[specta::specta]
pub async fn download_reranker(
    container: State<'_, Container>,
) -> Result<RerankerStatusDto, ApiError> {
    let manager = container.download_manager().map_err(ApiError::from)?;
    download_reranker_impl(manager.as_ref(), &container.models_path())
        .await
        .map_err(ApiError::from)
}

/// Delete the reranker's model. Search keeps its first-stage order until it
/// is downloaded again. Refused while its download is running.
#[tauri::command]
#[specta::specta]
pub async fn remove_reranker(
    container: State<'_, Container>,
) -> Result<RerankerStatusDto, ApiError> {
    let manager = container.download_manager().map_err(ApiError::from)?;
    let reranker = container.reranker();
    // Stop its server first so nothing holds the file open while it goes
    // (Windows refuses to delete an open file), and again after, in case a
    // search started one in between: on macOS and Linux a server still
    // holding the deleted file keeps its disk space taken.
    reranker.release().await;
    let result = remove_reranker_impl(manager.as_ref(), &container.models_path()).await;
    reranker.release().await;
    result.map_err(ApiError::from)
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
            remove_reranker,
        ])
        .build()
}
