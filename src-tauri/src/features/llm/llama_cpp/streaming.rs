//! Byte-safe SSE decoder; failed or truncated generations must not look successful.
use crate::features::llm::scheduler::rejection::{self, RequestSize};
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Default)]
pub(crate) struct Decoder {
    buffer: Vec<u8>,
    received_bytes: usize,
    done: bool,
    /// The failure the server reported inside the stream, when it did.
    server_failure: Option<ServerFailure>,
    finish_reason: Option<String>,
    allow_tool_calls: bool,
    content: String,
    reasoning: BTreeMap<String, String>,
    /// Reasoning received since the caller last drained it. The aggregate
    /// above becomes the response's displayable reasoning (it is not
    /// replayed); this buffer is only UI progress.
    reasoning_delta: String,
    tool_calls: BTreeMap<u64, Value>,
    usage: Value,
    /// Probabilities for the first public answer token. Hidden reasoning
    /// probabilities cannot be used as confidence in a factual verdict.
    first_logprobs: Option<Value>,
    /// The server's account of the request, from its final frame.
    timings: Option<ServerTimings>,
}

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
    fn from_value(timings: &Value) -> Option<Self> {
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
        let digest = self
            .message
            .as_deref()
            .map(rejection::digest)
            .unwrap_or_default();
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

impl Decoder {
    pub fn for_completion() -> Self {
        Self {
            allow_tool_calls: true,
            ..Self::default()
        }
    }
    pub fn done(&self) -> bool {
        self.done
    }
    /// The failure the server reported inside the stream, once [`Self::push`]
    /// has returned it as an error.
    pub fn take_server_failure(&mut self) -> Option<ServerFailure> {
        self.server_failure.take()
    }
    pub fn take_reasoning_delta(&mut self) -> String {
        std::mem::take(&mut self.reasoning_delta)
    }
    pub fn timings(&self) -> Option<ServerTimings> {
        self.timings
    }
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<String>> {
        self.received_bytes = self.received_bytes.saturating_add(bytes.len());
        if self.received_bytes > 16 * 1024 * 1024 {
            return Err(AppError::InvalidState(
                "llama.cpp response exceeds size limit".into(),
            ));
        }
        self.buffer.extend_from_slice(bytes);
        if self.buffer.len() > 4 * 1024 * 1024 {
            return Err(AppError::Network(
                "llama.cpp event exceeds size limit".into(),
            ));
        }
        let mut text = Vec::new();
        while let Some(end) = self.buffer.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=end).collect();
            let line = std::str::from_utf8(&line)
                .map_err(|_| AppError::Network("Invalid UTF-8 in llama.cpp stream".into()))?;
            let Some(data) = line.trim().strip_prefix("data:") else {
                continue;
            };
            if data.trim() == "[DONE]" {
                self.done = true;
                break;
            }
            let event: Value = serde_json::from_str(data.trim())
                .map_err(|_| AppError::Network("Invalid llama.cpp stream event".into()))?;
            if let Some(error) = event.get("error") {
                // HTTP 200 can still carry a server failure inside SSE. Keep
                // its structured status; the caller sanitizes its words.
                let failure = ServerFailure::streamed(error);
                let error = failure.error(&[]);
                self.server_failure = Some(failure);
                return Err(error);
            }
            if let Some(usage) = event.get("usage").filter(|value| value.is_object()) {
                self.usage = usage.clone();
            }
            if let Some(timings) = event.get("timings").and_then(ServerTimings::from_value) {
                self.timings = Some(timings);
            }
            if let Some(choice) = event
                .get("choices")
                .and_then(Value::as_array)
                .and_then(|c| c.first())
            {
                if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
                    // Running out of answer room still leaves an answer. The
                    // reason travels with the text so the caller decides what
                    // a cut-short answer is worth; failing here threw it away.
                    let expected = matches!(reason, "stop" | "length")
                        || (self.allow_tool_calls && reason == "tool_calls");
                    if !expected {
                        return Err(AppError::Network(format!(
                            "llama.cpp generation stopped: {reason}"
                        )));
                    }
                    self.finish_reason = Some(reason.into());
                }
                if self.content.is_empty()
                    && self.first_logprobs.is_none()
                    && choice
                        .pointer("/delta/content")
                        .and_then(Value::as_str)
                        .is_some_and(|content| !content.is_empty())
                    && !["reasoning_content", "reasoning"].iter().any(|key| {
                        choice
                            .get("delta")
                            .and_then(|delta| delta.get(key))
                            .and_then(Value::as_str)
                            .is_some_and(|text| !text.is_empty())
                    })
                {
                    self.first_logprobs = choice
                        .pointer("/logprobs/content/0")
                        .is_some()
                        .then(|| choice.get("logprobs").cloned())
                        .flatten();
                }
                if let Some(content) = choice
                    .pointer("/delta/content")
                    .and_then(Value::as_str)
                    .filter(|c| !c.is_empty())
                {
                    self.content.push_str(content);
                    text.push(content.into());
                }
                // Keep the reasoning for display; the replayed turn drops it.
                for key in ["reasoning_content", "reasoning"] {
                    if let Some(value) = choice
                        .get("delta")
                        .and_then(|d| d.get(key))
                        .and_then(Value::as_str)
                    {
                        self.reasoning_delta.push_str(value);
                        self.reasoning
                            .entry(key.into())
                            .or_default()
                            .push_str(value);
                    }
                }
                if let Some(calls) = choice
                    .pointer("/delta/tool_calls")
                    .and_then(Value::as_array)
                {
                    if !calls.is_empty() && !self.allow_tool_calls {
                        return Err(AppError::Network(
                            "Unexpected llama.cpp tool call in text generation".into(),
                        ));
                    }
                    for delta in calls {
                        if delta
                            .get("type")
                            .and_then(Value::as_str)
                            .is_some_and(|kind| kind != "function")
                        {
                            return Err(AppError::InvalidState(
                                "Unsupported llama.cpp tool call type".into(),
                            ));
                        }
                        let index =
                            delta.get("index").and_then(Value::as_u64).ok_or_else(|| {
                                AppError::Network(
                                    "llama.cpp returned a tool call without an index".into(),
                                )
                            })?;
                        if index >= 128 {
                            return Err(AppError::InvalidState(
                                "llama.cpp tool index exceeds limit".into(),
                            ));
                        }
                        let call = self.tool_calls.entry(index).or_insert_with(|| {
                            json!({
                                "type": "function", "function": {"name": "", "arguments": ""}
                            })
                        });
                        if let Some(id) = delta
                            .get("id")
                            .and_then(Value::as_str)
                            .filter(|id| !id.is_empty())
                        {
                            let object = call.as_object_mut().ok_or_else(|| {
                                AppError::Network("Invalid llama.cpp tool call".into())
                            })?;
                            object.insert("id".into(), json!(id));
                        }
                        for key in ["name", "arguments"] {
                            if let Some(fragment) = delta
                                .get("function")
                                .and_then(|f| f.get(key))
                                .and_then(Value::as_str)
                            {
                                let function = call
                                    .get_mut("function")
                                    .and_then(Value::as_object_mut)
                                    .ok_or_else(|| {
                                        AppError::Network("Invalid llama.cpp tool function".into())
                                    })?;
                                let mut value = function
                                    .get(key)
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_owned();
                                value.push_str(fragment);
                                function.insert(key.into(), json!(value));
                            }
                        }
                    }
                }
            }
        }
        Ok(text)
    }
    pub fn finish(&self) -> Result<()> {
        if self.finish_reason.is_some() {
            Ok(())
        } else {
            Err(AppError::Network(
                "llama.cpp stream ended before completion".into(),
            ))
        }
    }
    pub fn into_response(
        self,
    ) -> Result<lattice_core::application::ports::llm_port::CompletionResponse> {
        self.finish()?;
        let mut message = serde_json::Map::from_iter([
            ("role".into(), json!("assistant")),
            ("content".into(), json!(self.content)),
        ]);
        for (key, value) in self.reasoning {
            message.insert(key, json!(value));
        }
        let has_tools = !self.tool_calls.is_empty();
        if has_tools {
            message.insert(
                "tool_calls".into(),
                json!(self.tool_calls.into_values().collect::<Vec<_>>()),
            );
        }
        let public_logprobs = self
            .first_logprobs
            .as_ref()
            .and_then(lattice_core::application::ports::llm_port::first_token_logprobs);
        let mut response = super::parse_completion(json!({
            "choices": [{
                "message": message,
                "finish_reason": self.finish_reason.unwrap_or_else(|| if has_tools { "tool_calls" } else { "stop" }.into()),
                "logprobs": self.first_logprobs,
            }],
            "usage": self.usage,
        }))?;
        response.first_token_logprobs = public_logprobs;
        Ok(response)
    }
}
