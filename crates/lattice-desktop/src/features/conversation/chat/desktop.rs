//! Tauri adapter for the transport-independent turn workflow.
use super::{ChatResponse, ChatRuntime, ToolPreferences};
use lattice_core::domain::models::model_ref::ModelRef;
use lattice_core::shared::error::{AppError, Result};
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

pub async fn chat_with_conversation_impl<R: tauri::Runtime>(
    runtime: &dyn ChatRuntime,
    conversation_id: Option<String>,
    message: String,
    tool_preferences: Option<ToolPreferences>,
    cancel_only: Option<bool>,
    request_id: Option<String>,
    attachments: super::MessageAttachments,
    model: Option<ModelRef>,
    window: tauri::Window<R>,
) -> Result<ChatResponse> {
    let emit = Arc::new(move |payload| {
        window
            .emit(STREAM_EVENT, payload)
            .map_err(|error| AppError::InvalidState(format!("Frontend disconnected: {error}")))
    });
    super::run_turn(
        runtime,
        conversation_id,
        message,
        tool_preferences,
        cancel_only,
        request_id,
        attachments,
        model,
        emit,
    )
    .await
}
