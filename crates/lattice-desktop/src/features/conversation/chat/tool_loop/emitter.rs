//! The answer bubble's stream: every event tagged with the turn it belongs to.

use super::super::{ChatEventSink, ChatStreamEventDto, ReplyReferenceDto};
use lattice_core::application::services::model_contract::kept::TurnNames;
use lattice_core::application::services::model_contract::reply::{Markers, Piece, Splitter};
use lattice_core::shared::error::Result;
use std::sync::{Arc, Mutex, PoisonError};
use tracing::warn;

#[cfg(test)]
mod tests;

/// Emits `llm-stream` events tagged with their conversation and turn.
/// `llm-stream` is one global channel with several producers (this loop and
/// the QA service), so each consumer keeps only the events addressed to it.
pub(super) struct StreamEmitter<'a> {
    sink: &'a ChatEventSink,
    conversation_id: String,
    request_id: String,
    /// Whether a terminal `done` has been emitted for this turn.
    done_sent: bool,
    names: Arc<TurnNames>,
    markers: Markers,
    /// Splits the reply text of the generation round in progress into text and references.
    round: Mutex<Splitter>,
}

impl<'a> StreamEmitter<'a> {
    pub(super) fn new(
        sink: &'a ChatEventSink,
        conversation_id: &str,
        request_id: &str,
        names: Arc<TurnNames>,
        markers: Markers,
    ) -> Self {
        Self {
            sink,
            conversation_id: conversation_id.to_string(),
            request_id: request_id.to_string(),
            done_sent: false,
            names,
            round: Mutex::new(Splitter::new(markers.clone())),
            markers,
        }
    }

    /// Emit reply text: its text as `content` events and each marker that
    /// resolves as one `reference` event, in reply order.
    pub(super) fn content(&self, chunk: &str) -> Result<()> {
        let pieces = self.names.with(|scope| {
            self.round
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(scope, chunk)
        });
        self.send(pieces)
    }

    /// Ends the round in progress: what the splitter still holds is text from here on,
    /// and the next text starts a new round.
    pub(super) fn flush(&self) -> Result<()> {
        let ended = std::mem::replace(
            &mut *self.round.lock().unwrap_or_else(PoisonError::into_inner),
            Splitter::new(self.markers.clone()),
        );
        let pieces = self.names.with(|scope| ended.finish(scope));
        self.send(pieces)
    }

    fn send(&self, pieces: Vec<Piece>) -> Result<()> {
        for piece in pieces {
            let event = ChatStreamEventDto::new(&self.conversation_id, &self.request_id);
            self.emit(match piece {
                Piece::Text(text) => ChatStreamEventDto {
                    content: Some(text),
                    ..event
                },
                Piece::Reference(reference) => ChatStreamEventDto {
                    reference: Some(ReplyReferenceDto::from(&reference)),
                    ..event
                },
            })?;
        }
        Ok(())
    }

    /// Emit the terminal event. Idempotent, so belt-and-braces calls on
    /// several exit paths cannot produce duplicates.
    pub(super) fn done(&mut self) {
        if self.done_sent {
            return;
        }
        if let Err(e) = self.flush() {
            warn!(error = %e, "Failed to emit the end of the reply");
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
