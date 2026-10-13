//! A chat turn with no window attached, for the shipped-path evaluation.
//!
//! `examples/shipped_path_eval` has to measure the orchestrator the desktop
//! runs — intent, routing, the corpus planner, the corrective retry, the tool
//! loop, the grounding check and memory — not a reimplementation of it. The
//! desktop reaches that orchestrator through a Tauri window, which a headless
//! process does not have, so this is the one door it uses instead: the same
//! [`run_turn`] the window command calls, with the stream delivered to a sink
//! the caller owns.
use super::turn::run_turn;
use super::{ChatEventSink, ChatResponse, ToolPreferences};
use crate::interfaces::di::Container;
use crate::shared::error::Result;

/// Run one turn exactly as `chat_with_conversation` does, streaming every
/// event — answer text, steps, the finished grounding check — to `events`.
///
/// `conversation_id` `None` starts a new conversation, as the desktop does on
/// a first message. The grounding check finishes after this returns; it
/// arrives on `events` as a `verification` payload.
pub async fn run_headless_turn(
    container: &Container,
    conversation_id: Option<String>,
    message: String,
    tool_preferences: ToolPreferences,
    events: ChatEventSink,
) -> Result<ChatResponse> {
    run_turn(
        container,
        conversation_id,
        message,
        Some(tool_preferences),
        None,
        None,
        None,
        None,
        events,
    )
    .await
}
