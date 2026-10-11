//! Byte-safe SSE decoder; failed or truncated generations must not look successful.
use crate::shared::error::{AppError, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Default)]
pub(crate) struct Decoder {
    buffer: Vec<u8>,
    received_bytes: usize,
    done: bool,
    retryable_error: bool,
    finish_reason: Option<String>,
    allow_tool_calls: bool,
    content: String,
    reasoning: BTreeMap<String, String>,
    /// Reasoning received since the caller last drained it. The aggregate
    /// above is retained for native replay; this buffer is only UI progress.
    reasoning_delta: String,
    tool_calls: BTreeMap<u64, Value>,
    usage: Value,
    /// Probabilities for the first public answer token. Hidden reasoning
    /// probabilities cannot be used as confidence in a factual verdict.
    first_logprobs: Option<Value>,
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

/// Keep protocol failures typed across durable retries. Only fixed descriptions
/// of known server failures may be exposed; arbitrary error text can echo input.
fn generation_error(error: &Value) -> AppError {
    let code = error.get("code").and_then(Value::as_u64);
    let cause = match error.get("message").and_then(Value::as_str) {
        Some("Invalid input batch.") => ": the server could not process its inference batch",
        Some("Compute error.") => ": the server's inference computation failed",
        Some("Context size has been exceeded.") => ": the server exhausted its context capacity",
        _ => "",
    };
    let description = format!(
        "llama.cpp reported a generation error{}{cause}",
        error_kind(error)
    );
    match code {
        Some(429) => AppError::RateLimitExceeded(description),
        Some(408) => AppError::Network(description),
        Some(500..=599) => AppError::ServiceNotAvailable(description),
        _ => AppError::InvalidState(description),
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
    pub fn retryable_error(&self) -> bool {
        self.retryable_error
    }
    pub fn take_reasoning_delta(&mut self) -> String {
        std::mem::take(&mut self.reasoning_delta)
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
                // its structured status; never classify free-form messages.
                self.retryable_error = matches!(
                    error.get("code").and_then(Value::as_u64),
                    Some(408 | 429 | 500 | 502 | 503 | 504)
                );
                return Err(generation_error(error));
            }
            if let Some(usage) = event.get("usage").filter(|value| value.is_object()) {
                self.usage = usage.clone();
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
                // Retain native reasoning alongside tool calls for the next request.
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
    pub fn into_response(self) -> Result<crate::application::ports::llm_port::CompletionResponse> {
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
            .and_then(crate::application::ports::llm_port::first_token_logprobs);
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
