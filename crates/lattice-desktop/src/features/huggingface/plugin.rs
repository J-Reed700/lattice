//! HuggingFace Plugin
//!
//! Tauri commands for HuggingFace token management. The token itself is never
//! returned: the renderer can set it, delete it and ask whether it is set.

use crate::features::huggingface::commands::{self as hf_settings, HfTokenStatus};
use crate::interfaces::di::Container;
use lattice_core::shared::ipc::ApiError;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
};

#[tauri::command]
#[specta::specta]
pub async fn set_huggingface_token(
    container: State<'_, Container>,
    token: String,
) -> Result<(), ApiError> {
    hf_settings::set_huggingface_token(container.credentials().as_ref(), token)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_huggingface_token_status(
    container: State<'_, Container>,
) -> Result<HfTokenStatus, ApiError> {
    hf_settings::get_huggingface_token_status(container.credentials().as_ref())
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_huggingface_token(container: State<'_, Container>) -> Result<(), ApiError> {
    hf_settings::delete_huggingface_token(container.credentials().as_ref())
        .await
        .map_err(ApiError::from)
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("huggingface")
        .invoke_handler(tauri::generate_handler![
            set_huggingface_token,
            get_huggingface_token_status,
            delete_huggingface_token,
        ])
        .build()
}
