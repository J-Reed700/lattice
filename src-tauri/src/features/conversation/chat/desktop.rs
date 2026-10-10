//! Tauri adapter for the transport-independent turn workflow.
use super::{ChatResponse, ToolPreferences};
use crate::{
    interfaces::di::Container,
    shared::error::{AppError, Result},
};
use std::sync::Arc;
use tauri::Emitter;

/// The event every chat stream goes out on; each payload names its turn.
const STREAM_EVENT: &str = "llm-stream";

/// Streams to every window: for a turn no window is waiting on, such as a
/// research turn resumed after a restart.
pub fn app_event_sink<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> super::ChatEventSink {
    Arc::new(move |payload| {
        app.emit(STREAM_EVENT, payload)
            .map_err(|error| AppError::InvalidState(format!("Frontend disconnected: {error}")))
    })
}

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
            .emit(STREAM_EVENT, payload)
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
