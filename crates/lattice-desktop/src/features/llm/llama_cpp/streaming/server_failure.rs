//! Structured server failure parsing, sanitization, and final-frame timing metrics.

use crate::features::llm::scheduler::rejection;
use lattice_core::application::ports::llm_port::{RequestSize, ServedTiming};
use lattice_core::shared::error::AppError;
use lattice_core::shared::hash::sha256_hex;
use serde_json::{json, Value};
use std::time::Duration;

/// What llama-server reports about one request's work in the `timings`
/// object of its final frame. The cached share of the prompt is what shows
/// whether a conversation's prefix survived from the previous round.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct ServerTimings {
    /// Prompt tokens reused from the slot's KV cache (`cache_n`).
    pub cached_prompt_tokens: u64,
    /// Prompt tokens the server had to evaluate (`prompt_n`).
    pub evaluated_prompt_tokens: u64,
    /// Milliseconds spent evaluating them (`prompt_ms`).
    pub prompt_ms: f64,
    /// Tokens generated (`predicted_n`).
    pub generated_tokens: u64,
    /// Generation speed (`predicted_per_second`).
    pub generated_per_second: f64,
}

impl ServerTimings {
    /// The server's `timings` object, when it holds the counts this needs.
    pub(crate) fn from_value(timings: &Value) -> Option<Self> {
        let count = |key: &str| timings.get(key).and_then(Value::as_u64);
        let rate = |key: &str| timings.get(key).and_then(Value::as_f64).unwrap_or_default();
        Some(Self {
            cached_prompt_tokens: count("cache_n").unwrap_or_default(),
            evaluated_prompt_tokens: count("prompt_n")?,
            prompt_ms: rate("prompt_ms"),
            generated_tokens: count("predicted_n")?,
            generated_per_second: rate("predicted_per_second"),
        })
    }

    /// The request's cost in time, for the backend's pace: the prompt tokens
    /// the server evaluated, its cache hits left out, and the tokens it
    /// generated, at the speed it reports.
    pub(crate) fn served(&self) -> ServedTiming {
        ServedTiming {
            prompt_tokens: self.evaluated_prompt_tokens,
            prompt_seconds: self.prompt_ms / 1000.0,
            output_tokens: self.generated_tokens,
            output_seconds: if self.generated_per_second > 0.0 {
                self.generated_tokens as f64 / self.generated_per_second
            } else {
                0.0
            },
        }
    }
}

/// The server's own label for a failure: its status code and error type.
///
/// Never its message, which is free text and may quote the prompt. The type is
/// an identifier, so anything that does not look like one is dropped.
fn error_kind(error: &Value) -> String {
    let code = error.get("code").and_then(Value::as_u64);
    let kind = error.get("type").and_then(Value::as_str).filter(|kind| {
        !kind.is_empty()
            && kind.len() <= 48
            && kind.chars().all(|c| c.is_ascii_lowercase() || c == '_')
    });
    match (code, kind) {
        (Some(code), Some(kind)) => format!(" ({code} {kind})"),
        (Some(code), None) => format!(" ({code})"),
        (None, Some(kind)) => format!(" ({kind})"),
        (None, None) => String::new(),
    }
}

/// A fixed account of a server message the client recognises, which says
/// what happened better than the server's own words.
fn known_cause(message: &str) -> Option<&'static str> {
    match message {
        "Invalid input batch." => Some("the server could not process its inference batch"),
        "Compute error." => Some("the server's inference computation failed"),
        "Context size has been exceeded." => Some("the server exhausted its context capacity"),
        _ if message.contains("System message must be at the beginning") => {
            Some("the model requires a single system message at the beginning")
        }
        _ if message.contains("Unable to generate parser for this template") => {
            Some("the model's chat template rejected the message format")
        }
        _ => None,
    }
}

/// A failure the server reported about a request, in an HTTP status or inside
/// the stream.
///
/// The server's words are free text that can quote the prompt or echo what it
/// was sent. They leave this type only sanitized against the request
/// ([`rejection::sanitized_message`]), or replaced by a fixed account of a
/// message the client recognises; the log gets their digest, never the words.
#[derive(Debug, Clone)]
pub(crate) struct ServerFailure {
    /// The HTTP status, or the code a streamed error carried.
    status: Option<u16>,
    /// What the client says before the server's account.
    preface: String,
    /// The server's labels for the failure, from [`error_kind`].
    kind: String,
    /// The server's words, as it sent them.
    message: Option<String>,
    /// How long the server asked to be left alone.
    pub retry_after: Option<Duration>,
}

impl ServerFailure {
    /// An `error` event inside an answering stream.
    pub fn streamed(error: &Value) -> Self {
        Self {
            status: error
                .get("code")
                .and_then(Value::as_u64)
                .and_then(|code| u16::try_from(code).ok()),
            preface: "llama.cpp reported a generation error".into(),
            kind: error_kind(error),
            message: error.get("message").and_then(Value::as_str).map(Into::into),
            retry_after: None,
        }
    }

    /// A failed HTTP response from `path`, with the error the body carried.
    /// OpenAI-compatible servers put it under `error`, as an object or a bare
    /// string; some put its fields at the top level.
    pub fn http(
        status: reqwest::StatusCode,
        path: &str,
        body: Option<&Value>,
        retry_after: Option<Duration>,
    ) -> Self {
        let error = body.and_then(|body| match body.get("error") {
            Some(error @ Value::Object(_)) => Some(error.clone()),
            Some(Value::String(message)) => Some(json!({ "message": message })),
            _ => body.get("message").is_some().then(|| body.clone()),
        });
        let kind = error.as_ref().map(error_kind).unwrap_or_default();
        Self {
            status: Some(status.as_u16()),
            preface: format!("llama.cpp returned HTTP {status} from {path}"),
            kind: if kind.is_empty() {
                format!(" ({})", status.as_u16())
            } else {
                kind
            },
            message: error
                .as_ref()
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .map(Into::into),
            retry_after,
        }
    }

    /// The server was not there to answer, or asked to wait: worth retrying
    /// however often it happens, and never a verdict on the request.
    pub fn outage(&self) -> bool {
        matches!(self.status, Some(408 | 429 | 502 | 503 | 504))
    }

    /// The server failed while working on the request: worth one more try
    /// with a fresh prompt cache, but the same failure twice is deterministic.
    pub fn server_error(&self) -> bool {
        matches!(self.status, Some(500..=599)) && !self.outage()
    }

    /// The server refused the request itself; sending it again changes
    /// nothing.
    pub fn refusal(&self) -> bool {
        matches!(self.status, Some(400 | 413 | 422))
    }

    /// The server aborted a structured-output response because it exhausted
    /// its output token budget (`finish_reason=length`). Surface as
    /// [`AppError::AnswerCutOff`] rather than a fatal [`AppError::InvalidState`]
    /// so contract-level cut-off recovery can react.
    pub fn structured_output_cut_off(&self) -> bool {
        self.kind.contains("structured_output_failed")
            && self
                .message
                .as_deref()
                .is_some_and(|message| message.contains("finish_reason=length"))
    }

    /// The server's own words, safe to show: a fixed account of a message
    /// the client recognises, otherwise the words sanitized against the
    /// request. An authentication failure's words are not shown at all: they
    /// may repeat the credential that was sent.
    fn words(&self, request_texts: &[&str]) -> String {
        if matches!(self.status, Some(401 | 403 | 407)) {
            return String::new();
        }
        let Some(message) = &self.message else {
            return String::new();
        };
        known_cause(message).map_or_else(
            || rejection::sanitized_message(message, request_texts),
            Into::into,
        )
    }

    /// What tells one failure from another: the status and the server's
    /// error type. Never its words, which may carry a request id or a slot
    /// number that changes on every attempt.
    pub fn signature(&self) -> (Option<u16>, &str) {
        (self.status, self.kind.trim())
    }

    /// The server's labels and words, as one line.
    pub fn account(&self, request_texts: &[&str]) -> String {
        let words = self.words(request_texts);
        let kind = self.kind.trim();
        if words.is_empty() {
            kind.to_owned()
        } else {
            format!("{kind} {words}")
        }
    }

    /// The failure as an error, typed so a durable caller can tell an outage
    /// from a failure of the work.
    pub fn error(&self, request_texts: &[&str]) -> AppError {
        let words = self.words(request_texts);
        let description = if words.is_empty() {
            format!("{}{}", self.preface, self.kind)
        } else {
            format!("{}{}: {words}", self.preface, self.kind)
        };
        if self.structured_output_cut_off() {
            return AppError::AnswerCutOff(description);
        }
        match self.status {
            Some(429) => AppError::RateLimitExceeded(description),
            Some(408 | 502 | 503 | 504) => AppError::Network(description),
            _ if self.refusal() => self.rejected(request_texts, None, false),
            Some(500..=599) => AppError::ServiceNotAvailable(description),
            _ => AppError::InvalidState(description),
        }
    }

    /// The failure as a rejection retrying cannot change: the server refused
    /// the request, or (`twice`) failed it the same way two times. `request`
    /// is its size and the configured window, when known.
    pub fn rejected(
        &self,
        request_texts: &[&str],
        request: Option<(RequestSize, usize)>,
        twice: bool,
    ) -> AppError {
        let too_large = rejection::is_size_refusal(
            self.status,
            &self.kind,
            self.message.as_deref().unwrap_or_default(),
        );
        rejection::rejected(
            "llama.cpp",
            twice,
            &self.account(request_texts),
            request,
            too_large,
        )
    }

    /// One structured line per failure: the server's sanitized account and
    /// its words' digest, against the request's size and the window.
    pub fn log(&self, request_texts: &[&str], attempt: usize, size: RequestSize, window: usize) {
        let digest = self.message.as_deref().map(sha256_hex).unwrap_or_default();
        tracing::warn!(
            status = self.status,
            kind = self.kind.trim(),
            sanitized = %self.words(request_texts),
            sha256 = %digest,
            prompt_tokens = size.prompt_tokens,
            counted_exactly = size.counted_exactly,
            output_tokens = size.output_tokens,
            window,
            attempt,
            "llm.request_rejected"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn structured_output_length_cutoff_maps_to_answer_cut_off() {
        for error_event in [
            json!({
                "type": "structured_output_failed",
                "message": "structured output was incomplete (finish_reason=length); increase the output budget"
            }),
            json!({
                "code": 500,
                "type": "structured_output_failed",
                "message": "structured output was incomplete (finish_reason=length); increase the output budget"
            }),
        ] {
            let failure = ServerFailure::streamed(&error_event);
            assert!(failure.structured_output_cut_off());
            let err = failure.error(&[]);
            assert!(matches!(err, AppError::AnswerCutOff(_)), "{err:?}");
        }

        let other = ServerFailure::streamed(&json!({
            "type": "structured_output_failed",
            "message": "grammar compilation error"
        }));
        assert!(!other.structured_output_cut_off());
        assert!(matches!(other.error(&[]), AppError::InvalidState(_)));
    }
}
