//! Config plugin commands - direct calls to ConfigService

use crate::interfaces::commands::config::{
    add_watch_folder as add_watch_folder_impl, remove_watch_folder as remove_watch_folder_impl,
    save_config as save_config_impl, ConfigService,
};
use crate::shared::api_result::{ApiError, ErrorCode};
use tauri::State;

pub use super::types::*;

#[tauri::command]
#[specta::specta]
pub async fn get_config(config_service: State<'_, ConfigService>) -> Result<AppConfig, ApiError> {
    config_service
        .get_config()
        .await
        .map_err(|e| ApiError::from(e))
}

#[tauri::command]
#[specta::specta]
pub async fn save_config(
    config: AppConfig,
    config_service: State<'_, ConfigService>,
) -> Result<(), ApiError> {
    save_config_impl(config, config_service)
        .await
        .map_err(|e| ApiError::from(e))
}

#[tauri::command]
#[specta::specta]
pub async fn get_watch_folders(
    config_service: State<'_, ConfigService>,
) -> Result<Vec<String>, ApiError> {
    config_service
        .get_watch_folders()
        .await
        .map_err(|e| ApiError::from(e))
}

#[tauri::command]
#[specta::specta]
pub async fn add_watch_folder(
    path: String,
    config_service: State<'_, ConfigService>,
) -> Result<(), ApiError> {
    add_watch_folder_impl(path, config_service)
        .await
        .map_err(|e| ApiError::from(e))
}

#[tauri::command]
#[specta::specta]
pub async fn remove_watch_folder(
    path: String,
    config_service: State<'_, ConfigService>,
) -> Result<(), ApiError> {
    remove_watch_folder_impl(path, config_service)
        .await
        .map_err(|e| ApiError::from(e))
}
