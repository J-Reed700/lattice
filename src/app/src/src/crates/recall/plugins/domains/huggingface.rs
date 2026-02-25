//! HuggingFace Plugin
//!
//! Tauri commands for HuggingFace token management.
//! Routes to interfaces/commands/domains/hf_settings.rs implementations.

use crate::{
    interfaces::commands::hf_settings::{self, HfTokenStatus},
    shared::api_result::ApiError,
};
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime,
};

#[tauri::command]
#[specta::specta]
pub async fn set_huggingface_token(token: String) -> Result<(), ApiError> {
    hf_settings::set_huggingface_token(token)
        .await
        .map_err(|e| ApiError {
            code: crate::shared::api_result::ErrorCode::InvalidInput,
            message: e,
            details: None,
        })
}

#[tauri::command]
#[specta::specta]
pub async fn get_huggingface_token_status() -> Result<HfTokenStatus, ApiError> {
    hf_settings::get_huggingface_token_status()
        .await
        .map_err(|e| ApiError {
            code: crate::shared::api_result::ErrorCode::InternalError,
            message: e,
            details: None,
        })
}

#[tauri::command]
#[specta::specta]
pub async fn get_huggingface_token() -> Result<Option<String>, ApiError> {
    hf_settings::get_huggingface_token()
        .await
        .map_err(|e| ApiError {
            code: crate::shared::api_result::ErrorCode::InternalError,
            message: e,
            details: None,
        })
}

#[tauri::command]
#[specta::specta]
pub async fn delete_huggingface_token() -> Result<(), ApiError> {
    hf_settings::delete_huggingface_token()
        .await
        .map_err(|e| ApiError {
            code: crate::shared::api_result::ErrorCode::InternalError,
            message: e,
            details: None,
        })
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("huggingface")
        .invoke_handler(tauri::generate_handler![
            set_huggingface_token,
            get_huggingface_token_status,
            get_huggingface_token,
            delete_huggingface_token,
        ])
        .build()
}
