//! Favorites Plugin - Favorite documents management
//!
//! Migrated from ipc/domains/favorites.rs as part of Operation Scorched Earth Batch 2

use crate::features::favorites::commands as favorites;
use crate::interfaces::di::Container;
use crate::shared::api_result::ApiError;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
};

pub use crate::features::favorites::commands::FavoriteDocument;

#[tauri::command]
#[specta::specta]
pub async fn add_favorite(
    document_id: String,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    favorites::add_favorite_impl(container.inner(), document_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn remove_favorite(
    document_id: String,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    favorites::remove_favorite_impl(container.inner(), document_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_favorites(
    container: State<'_, Container>,
) -> Result<Vec<FavoriteDocument>, ApiError> {
    favorites::get_favorites_impl(container.inner())
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn is_favorite(
    document_id: String,
    container: State<'_, Container>,
) -> Result<bool, ApiError> {
    favorites::is_favorite_impl(container.inner(), document_id)
        .await
        .map_err(ApiError::from)
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("favorites")
        .invoke_handler(tauri::generate_handler![
            add_favorite,
            remove_favorite,
            get_favorites,
            is_favorite,
        ])
        .build()
}
