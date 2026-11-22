//! The answer bubble's stream: every event tagged with the turn it belongs to.

use super::super::{ChatEventSink, ChatStreamEventDto};
use lattice_core::application::services::model_contract::kept::TurnNames;
use lattice_core::application::services::model_contract::reply::{self, Markers, Piece, Splitter};
use lattice_core::application::services::model_contract::shown::Kind;
use lattice_core::application::services::tool_handles::{picture_line, Picture};
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
    /// Reply text emitted so far (with markers preserved), so a user stop keeps partial tokens.
    written: Mutex<String>,
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
            written: Mutex::new(String::new()),
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
            let (written_part, dto) = match piece {
                Piece::Text(text) => (
                    text.clone(),
                    ChatStreamEventDto {
                        content: Some(text),
                        ..event
                    },
                ),
                Piece::Reference(reference) if reference.prefix == Picture::PREFIX => {
                    let line = picture_line(&reference, &self.request_id)
                        .unwrap_or_else(|| reply::written(&[Piece::Reference(reference)]));
                    (
                        line.clone(),
                        ChatStreamEventDto {
                            content: Some(line),
                            ..event
                        },
                    )
                }
                Piece::Reference(reference) => {
                    let marker = reply::written(&[Piece::Reference(reference.clone())]);
                    (
                        marker,
                        ChatStreamEventDto {
                            reference: Some(super::super::reply_text::wire(&reference)),
                            ..event
                        },
                    )
                }
            };
            self.written
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push_str(&written_part);
            self.emit(dto)?;
        }
        Ok(())
    }

    /// Ends a cancelled turn's stream once and returns any non-whitespace partial
    /// reply already written to the bubble.
    pub(super) fn finish_cancelled(&mut self) -> Option<String> {
        if !self.done_sent {
            if let Err(e) = self.flush() {
                warn!(error = %e, "Failed to flush cancelled reply");
            }
            self.done_sent = true;
            if let Err(e) = self.emit(ChatStreamEventDto {
                done: true,
                status: Some("cancelled".to_owned()),
                ..ChatStreamEventDto::new(&self.conversation_id, &self.request_id)
            }) {
                warn!(error = %e, "Failed to emit stream cancellation");
            }
        }
        let written =
            std::mem::take(&mut *self.written.lock().unwrap_or_else(PoisonError::into_inner));
        (!written.trim().is_empty()).then_some(written)
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
