//! Credentials plugin commands - secure API key storage

use crate::features::credentials::commands as credentials;
use crate::interfaces::di::Container;
use crate::shared::api_result::ApiError;
use tauri::State;

pub use super::types::*;

#[tauri::command]
#[specta::specta]
pub async fn credentials_store(
    service: String,
    key: String,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    credentials::set_api_key_impl(container.inner(), service, key)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn credentials_get(
    service: String,
    container: State<'_, Container>,
) -> Result<Option<String>, ApiError> {
    credentials::get_api_key_impl(container.inner(), service)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn credentials_delete(
    service: String,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    credentials::delete_api_key_impl(container.inner(), service)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn credentials_has(
    service: String,
    container: State<'_, Container>,
) -> Result<bool, ApiError> {
    credentials::has_api_key_impl(container.inner(), service)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn credentials_clear_all(container: State<'_, Container>) -> Result<(), ApiError> {
    credentials::clear_all_credentials_impl(container.inner())
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn credentials_set_endpoint(
    endpoint: String,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    credentials::set_custom_endpoint_impl(container.inner(), endpoint)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn credentials_get_endpoint(
    container: State<'_, Container>,
) -> Result<Option<String>, ApiError> {
    credentials::get_custom_endpoint_impl(container.inner())
        .await
        .map_err(ApiError::from)
}
