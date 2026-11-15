//! Typed cloud streams publish only displayable text through callbacks while
//! retaining the exact native completion—including opaque replay state—in the
//! returned response.
use super::protocol::{CompletionProtocol, EventDecoder};
use super::timing::StreamClock;
use super::{field, CompletionResponse};
use futures::{Stream, StreamExt};
use lattice_core::shared::error::WithCauses;
use lattice_core::shared::error::{AppError, Result};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub(super) type Callback<'a> = &'a (dyn Fn(String) -> Result<()> + Send + Sync);

/// Read a stream to its completion. The request went out at `sent`; unless
/// the completion carries the server's own timings, the first streamed delta
/// splits the time since into reading the prompt and writing the answer.
pub(super) async fn drain<S, B, E>(
    protocol: &'static dyn CompletionProtocol,
    mut bytes: S,
    stall_timeout: Duration,
    sent: Instant,
    on_text: Callback<'_>,
    on_reasoning: Callback<'_>,
) -> Result<CompletionResponse>
where
    S: Stream<Item = std::result::Result<B, E>> + Unpin,
    B: AsRef<[u8]>,
    E: std::error::Error + 'static,
{
    let clock = StreamClock::started(sent);
    let generating = AtomicBool::new(false);
    let timed_text = |text: String| {
        clock.delta();
        generating.store(true, Ordering::Relaxed);
        on_text(text)
    };
    let timed_reasoning = |text: String| {
        clock.delta();
        generating.store(true, Ordering::Relaxed);
        on_reasoning(text)
    };
    let (on_text, on_reasoning): (Callback<'_>, Callback<'_>) = (&timed_text, &timed_reasoning);
    let mut decoder = Decoder::new(protocol);
    loop {
        // Silence before the first delta is the model reading the prompt or
        // thinking, which a reasoning model can do for minutes with nothing to
        // send: Claude streams no thinking unless asked to, and an OpenAI
        // reasoning model without a summary sends only keepalives. That wait
        // is bounded by the turn's deadline, Stop and shutdown. Once the
        // answer is streaming, a silence this long is a stall.
        let armed = generating.load(Ordering::Relaxed) || decoder.events.has_tool_output();
        let next = if armed {
            tokio::time::timeout(stall_timeout, bytes.next())
                .await
                .map_err(|_| {
                    AppError::Network(format!(
                        "Cloud stream stalled for {}s",
                        stall_timeout.as_secs()
                    ))
                })?
        } else {
            bytes.next().await
        };
        let Some(chunk) = next else { break };
        let chunk = chunk.map_err(|error| {
            AppError::Network(format!("Cloud stream interrupted: {}", WithCauses(&error)))
        })?;
        decoder.push(chunk.as_ref(), on_text, on_reasoning)?;
        if decoder.events.is_complete() {
            break;
        }
    }
    let completed = decoder.into_completed(on_text, on_reasoning)?;
    let prompt_tokens = protocol.prompt_tokens_read(&completed);
    let mut response = protocol.parse_reply(completed)?;
    if response.timing.is_none() {
        response.timing = clock.timing(prompt_tokens, response.output_tokens);
    }
    Ok(response)
}

/// Splits a byte stream into server-sent events and hands each to the
/// provider's decoder.
struct Decoder {
    #[cfg(test)]
    protocol: &'static dyn CompletionProtocol,
    events: Box<dyn EventDecoder>,
    buffer: Vec<u8>,
    data: String,
    received: usize,
}

/// Joins a reasoning summary's parts with a blank line between them.
#[derive(Default)]
pub(super) struct ReasoningParts {
    part: Option<(String, u64)>,
}

impl ReasoningParts {
    pub(super) fn push(
        &mut self,
        part: (String, u64),
        text: &str,
        callback: Callback<'_>,
    ) -> Result<()> {
        if self.part.as_ref().is_some_and(|previous| previous != &part) {
            callback("\n\n".into())?;
        }
        self.part = Some(part);
        callback(text.to_owned())
    }
}

pub(super) fn invalid(detail: &str) -> AppError {
    AppError::Network(format!("Invalid cloud stream: {detail}"))
}

impl Decoder {
    const MAX_EVENT_BYTES: usize = 4 * 1024 * 1024;
    const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

    fn new(protocol: &'static dyn CompletionProtocol) -> Self {
        Self {
            #[cfg(test)]
            protocol,
            events: protocol.event_decoder(),
            buffer: Vec::new(),
            data: String::new(),
            received: 0,
        }
    }

    fn push(
        &mut self,
        bytes: &[u8],
        on_text: Callback<'_>,
        on_reasoning: Callback<'_>,
    ) -> Result<()> {
        self.received = self.received.saturating_add(bytes.len());
        if self.received > Self::MAX_RESPONSE_BYTES {
            return Err(invalid("response exceeds size limit"));
        }
        self.buffer.extend_from_slice(bytes);
        while let Some(end) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<_> = self.buffer.drain(..=end).collect();
            self.line(&line, on_text, on_reasoning)?;
            if self.events.is_complete() {
                break;
            }
        }
        if self.buffer.len().saturating_add(self.data.len()) > Self::MAX_EVENT_BYTES {
            return Err(invalid("event exceeds size limit"));
        }
        Ok(())
    }

    fn line(
        &mut self,
        bytes: &[u8],
        on_text: Callback<'_>,
        on_reasoning: Callback<'_>,
    ) -> Result<()> {
        let line = std::str::from_utf8(bytes)
            .map_err(|_| invalid("invalid UTF-8"))?
            .trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            return self.dispatch(on_text, on_reasoning);
        }
        if let Some(data) = line.strip_prefix("data:") {
            if !self.data.is_empty() {
                self.data.push('\n');
            }
            self.data.push_str(data.strip_prefix(' ').unwrap_or(data));
            if self.data.len() > Self::MAX_EVENT_BYTES {
                return Err(invalid("event exceeds size limit"));
            }
        }
        Ok(())
    }

    fn dispatch(&mut self, on_text: Callback<'_>, on_reasoning: Callback<'_>) -> Result<()> {
        let data = std::mem::take(&mut self.data);
        if data.is_empty() || data == "[DONE]" {
            return Ok(());
        }
        let event: Value =
            serde_json::from_str(&data).map_err(|_| invalid("invalid event JSON"))?;
        let kind = field(&event, "type").as_str().unwrap_or("");
        if self.events.is_failure(kind) {
            // Provider error text may contain the user's prompt.
            return Err(AppError::Network(format!("Cloud stream failed ({kind})")));
        }
        self.events.event(&event, kind, on_text, on_reasoning)
    }

    #[cfg(test)]
    fn finish(
        self,
        on_text: Callback<'_>,
        on_reasoning: Callback<'_>,
    ) -> Result<CompletionResponse> {
        let protocol = self.protocol;
        protocol.parse_reply(self.into_completed(on_text, on_reasoning)?)
    }

    /// The completed response in the provider's own shape, once the stream
    /// has ended.
    fn into_completed(
        mut self,
        on_text: Callback<'_>,
        on_reasoning: Callback<'_>,
    ) -> Result<Value> {
        if !self.events.is_complete() {
            let remaining = std::mem::take(&mut self.buffer);
            self.line(&remaining, on_text, on_reasoning)?;
            self.dispatch(on_text, on_reasoning)?;
        }
        self.events
            .finish()?
            .ok_or_else(|| invalid("stream ended before completion"))
    }
}

#[cfg(test)]
mod tests;
