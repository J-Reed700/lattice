//! QA Plugin (Phase 4: Consolidation)
//!
//! Thin plugin wrapper for question-answering commands with Retrieval-Augmented Generation (RAG).
//! Delegates all business logic to `interfaces/commands/domains/qa_commands.rs`.

use crate::features::qa::dto::{QARequestDto, QAResponseDto};
use crate::features::qa::commands::{
    ask_question, ask_question_stream, check_llm_health, get_qa_model,
};
use crate::interfaces::di::Container;
use crate::shared::api_result::ApiError;
use tauri::{plugin::Builder, AppHandle, Manager, Runtime, State};

pub fn init<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    Builder::new("qa")
        .invoke_handler(tauri::generate_handler![
            ask_question_wrapper,
            ask_question_stream_wrapper,
            get_qa_model_wrapper,
            check_llm_health_wrapper,
        ])
        .build()
}

#[tauri::command]
#[specta::specta]
async fn ask_question_wrapper(
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
async fn ask_question_stream_wrapper<R: Runtime>(
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
async fn get_qa_model_wrapper(container: State<'_, Container>) -> Result<String, ApiError> {
    get_qa_model(container).await.map_err(|e| ApiError {
        code: crate::shared::api_result::ErrorCode::InternalError,
        message: e.to_string(),
        details: None,
    })
}

#[tauri::command]
#[specta::specta]
async fn check_llm_health_wrapper(
    container: State<'_, Container>,
) -> Result<serde_json::Value, ApiError> {
    check_llm_health(container).await.map_err(|e| ApiError {
        code: crate::shared::api_result::ErrorCode::InternalError,
        message: e.to_string(),
        details: None,
    })
}
