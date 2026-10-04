//! Tauri adapter for the transport-independent turn workflow.
use super::{ChatResponse, ToolPreferences};
use crate::{
    interfaces::di::Container,
    shared::error::{AppError, Result},
};
use std::sync::Arc;
use tauri::Emitter;

#[allow(clippy::too_many_arguments)]
pub async fn chat_with_conversation_impl<R: tauri::Runtime>(
    container: &Container,
    conversation_id: Option<String>,
    message: String,
    tool_preferences: Option<ToolPreferences>,
    cancel_only: Option<bool>,
    request_id: Option<String>,
    attachment_names: Option<Vec<String>>,
    attachment_document_ids: Option<Vec<String>>,
    window: tauri::Window<R>,
) -> Result<ChatResponse> {
    let emit = Arc::new(move |payload| {
        window
            .emit("llm-stream", payload)
            .map_err(|error| AppError::InvalidState(format!("Frontend disconnected: {error}")))
    });
    super::run_turn(
        container,
        conversation_id,
        message,
        tool_preferences,
        cancel_only,
        request_id,
        attachment_names,
        attachment_document_ids,
        emit,
    )
    .await
}
