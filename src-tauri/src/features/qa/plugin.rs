//! QA plugin.
//!
//! Thin plugin wrapper for the chat surface's LLM health check and starters.

use crate::features::qa::commands::check_llm_health;
use crate::features::qa::starters_dto::ChatStartersDto;
use crate::interfaces::di::Container;
use crate::shared::ipc::ApiError;
use tauri::{plugin::Builder, Runtime, State};

pub fn init<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    Builder::new("qa")
        .invoke_handler(tauri::generate_handler![
            check_llm_health_wrapper,
            generate_chat_starters_wrapper,
        ])
        .build()
}

#[tauri::command]
#[specta::specta]
pub async fn check_llm_health_wrapper(
    container: State<'_, Container>,
) -> Result<super::dto::LLMHealthStatusDto, ApiError> {
    check_llm_health(container).await.map_err(|e| ApiError {
        code: crate::shared::ipc::ErrorCode::InternalError,
        message: e.to_string(),
        details: None,
    })
}

/// Corpus-derived opening questions for the Chat empty state, drawn from the
/// documents one space can see. A blank `space_id` means General.
#[tauri::command]
#[specta::specta]
pub async fn generate_chat_starters_wrapper(
    container: State<'_, Container>,
    space_id: Option<String>,
) -> Result<ChatStartersDto, ApiError> {
    crate::features::qa::starters::generate_chat_starters_impl(container.inner(), space_id).await
}
