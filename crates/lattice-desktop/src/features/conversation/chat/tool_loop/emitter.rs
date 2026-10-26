//! The answer bubble's stream: every event tagged with the turn it belongs to.

use super::super::{ChatEventSink, ChatStreamEventDto};
use lattice_core::shared::error::Result;
use tracing::warn;

/// Emits `llm-stream` events tagged with the turn they belong to.
///
/// `llm-stream` is a single global Tauri channel with more than one producer
/// (this tool loop and the QA service), and the payloads previously carried no
/// identity at all. Any consumer therefore had to accept every event on the
/// channel, so a second concurrent generation — a query rewrite running during
/// a chat turn, or a resend whose predecessor's listener was still attached —
/// interleaved its tokens into the wrong bubble.
///
/// Tagging every emit with `conversation_id` and a per-turn `request_id` lets
/// each consumer keep only what is addressed to it.
pub(super) struct StreamEmitter<'a> {
    sink: &'a ChatEventSink,
    conversation_id: String,
    request_id: String,
    /// Whether a terminal `done` has been emitted for this turn.
    done_sent: bool,
}

impl<'a> StreamEmitter<'a> {
    pub(super) fn new(sink: &'a ChatEventSink, conversation_id: &str, request_id: &str) -> Self {
        Self {
            sink,
            conversation_id: conversation_id.to_string(),
            request_id: request_id.to_string(),
            done_sent: false,
        }
    }

    /// Emit a content chunk.
    pub(super) fn content(&self, chunk: &str) -> Result<()> {
        self.emit(ChatStreamEventDto {
            content: Some(chunk.to_owned()),
            ..ChatStreamEventDto::new(&self.conversation_id, &self.request_id)
        })
    }

    /// Emit the terminal event. Idempotent, so belt-and-braces calls on
    /// several exit paths cannot produce duplicates.
    pub(super) fn done(&mut self) {
        if self.done_sent {
            return;
        }
        self.done_sent = true;
        if let Err(e) = self.emit(ChatStreamEventDto {
            done: true,
            ..ChatStreamEventDto::new(&self.conversation_id, &self.request_id)
        }) {
            warn!(error = %e, "Failed to emit stream completion");
        }
    }

    fn emit(&self, payload: ChatStreamEventDto) -> Result<()> {
        (self.sink)(payload)
    }
}

impl Drop for StreamEmitter<'_> {
    /// Guarantee a terminal event on **every** exit path, including the error
    /// returns scattered through the tool loop. Without this the UI stays
    /// stuck "generating" unless the outer invoke happens to reject.
    fn drop(&mut self) {
        self.done();
    }
}
