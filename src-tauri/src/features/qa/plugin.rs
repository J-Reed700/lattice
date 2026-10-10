//! QA plugin.
//!
//! Thin plugin wrapper for the chat surface's LLM health check.

use crate::features::qa::commands;
use crate::interfaces::di::Container;
use crate::shared::ipc::ApiError;
use tauri::{plugin::Builder, Runtime, State};

pub fn init<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    Builder::new("qa")
        .invoke_handler(tauri::generate_handler![check_llm_health,])
        .build()
}

#[tauri::command]
#[specta::specta]
pub async fn check_llm_health(
    container: State<'_, Container>,
) -> Result<super::dto::LLMHealthStatusDto, ApiError> {
    commands::check_llm_health(container)
        .await
        .map_err(|e| ApiError {
            code: crate::shared::ipc::ErrorCode::InternalError,
            message: e.to_string(),
            details: None,
        })
}
