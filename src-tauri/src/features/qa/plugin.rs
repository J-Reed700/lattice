//! QA plugin.
//!
//! Thin plugin wrapper for question-answering commands with Retrieval-Augmented Generation (RAG).
//! Delegates all business logic to `interfaces/commands/domains/qa_commands.rs`.

use crate::features::qa::commands::{
    ask_question, ask_question_stream, check_llm_health, get_qa_model,
};
use crate::features::qa::dto::{QARequestDto, QAResponseDto};
use crate::features::qa::starters_dto::ChatStartersDto;
use crate::interfaces::di::Container;
use crate::shared::api_result::ApiError;
use tauri::{plugin::Builder, AppHandle, Runtime, State};

pub fn init<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    Builder::new("qa")
        .invoke_handler(tauri::generate_handler![
            ask_question_wrapper,
            ask_question_stream_wrapper,
            get_qa_model_wrapper,
            check_llm_health_wrapper,
            generate_chat_starters_wrapper,
        ])
        .build()
}

#[tauri::command]
#[specta::specta]
pub async fn ask_question_wrapper(
    container: State<'_, Container>,
    request: QARequestDto,
) -> Result<QAResponseDto, ApiError> {
    ask_question(container, request)
        .await
        .map_err(|e| ApiError {
            code: crate::shared::api_result::ErrorCode::InternalError,
            message: e.to_string(),
            details: None,
        })
}

#[tauri::command]
#[specta::specta]
pub async fn ask_question_stream_wrapper<R: Runtime>(
    app_handle: AppHandle<R>,
    container: State<'_, Container>,
    request: QARequestDto,
) -> Result<QAResponseDto, ApiError> {
    ask_question_stream(app_handle, container, request)
        .await
        .map_err(|e| ApiError {
            code: crate::shared::api_result::ErrorCode::InternalError,
            message: e.to_string(),
            details: None,
        })
}

#[tauri::command]
#[specta::specta]
pub async fn get_qa_model_wrapper(container: State<'_, Container>) -> Result<String, ApiError> {
    get_qa_model(container).await.map_err(|e| ApiError {
        code: crate::shared::api_result::ErrorCode::InternalError,
        message: e.to_string(),
        details: None,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn check_llm_health_wrapper(
    container: State<'_, Container>,
) -> Result<super::dto::LLMHealthStatusDto, ApiError> {
    check_llm_health(container).await.map_err(|e| ApiError {
        code: crate::shared::api_result::ErrorCode::InternalError,
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
