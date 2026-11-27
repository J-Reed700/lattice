//! Byte-safe SSE decoder; failed or truncated generations must not look successful.

mod server_failure;
mod think_filter;

pub(crate) use server_failure::{ServerFailure, ServerTimings};
pub(crate) use think_filter::strip_unmatched_think_close;

use super::repetition::{has_repetition_loop, LOOP_CHECK_STRIDE_BYTES};
use crate::features::llm::fitting::ResponseBudget;
use lattice_core::application::ports::llm_port::{first_token_logprobs, CompletionResponse};
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use think_filter::ContentFilter;

#[derive(Default)]
pub(crate) struct Decoder {
    budget: ResponseBudget,
    buffer: Vec<u8>,
    received_bytes: usize,
    token_events: usize,
    done: bool,
    /// The failure the server reported inside the stream, when it did.
    server_failure: Option<ServerFailure>,
    /// Whether the stream was aborted after detecting a degenerate repetition loop.
    repetition_loop: bool,
    finish_reason: Option<String>,
    allow_tool_calls: bool,
    content: String,
    content_loop_checked_at: usize,
    content_filter: ContentFilter,
    reasoning: BTreeMap<String, String>,
    reasoning_loop_checked_at: usize,
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

impl Decoder {
    #[cfg(test)]
    pub fn for_completion() -> Self {
        Self::with_budget(ResponseBudget::default())
    }
    pub fn with_budget(budget: ResponseBudget) -> Self {
        Self {
            allow_tool_calls: true,
            budget,
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
    /// Whether [`Self::push`] aborted the stream due to a confirmed repetition loop.
    pub fn repetition_loop(&self) -> bool {
        self.repetition_loop
    }
    pub fn take_reasoning_delta(&mut self) -> String {
        std::mem::take(&mut self.reasoning_delta)
    }
    pub fn timings(&self) -> Option<ServerTimings> {
        self.timings
    }
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<String>> {
        self.received_bytes = self.received_bytes.saturating_add(bytes.len());
        if self.received_bytes > self.budget.max_stream_bytes {
            return Err(AppError::InvalidState(
                "llama.cpp response exceeds output budget size limit".into(),
            ));
        }
        self.buffer.extend_from_slice(bytes);
        let mut text = Vec::new();
        while let Some(end) = self.buffer.iter().position(|b| *b == b'\n') {
            if end + 1 > self.budget.max_event_bytes {
                return Err(AppError::Network(
                    "llama.cpp event exceeds output budget size limit".into(),
                ));
            }
            let line: Vec<u8> = self.buffer.drain(..=end).collect();
            let line = std::str::from_utf8(&line)
                .map_err(|_| AppError::Network("Invalid UTF-8 in llama.cpp stream".into()))?;
            let Some(data) = line.trim().strip_prefix("data:") else {
                continue;
            };
            if data.trim() == "[DONE]" {
                let tail = self.content_filter.flush();
                if !tail.is_empty() {
                    self.content.push_str(&tail);
                    text.push(tail);
                }
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
                let finish_reason = choice.get("finish_reason").and_then(Value::as_str);
                if let Some(reason) = finish_reason {
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
                let was_empty = self.content.is_empty();
                let mut emitted_token = false;
                if let Some(content) = choice
                    .pointer("/delta/content")
                    .and_then(Value::as_str)
                    .filter(|c| !c.is_empty())
                {
                    emitted_token = true;
                    let emitted = self.content_filter.push(content);
                    if !emitted.is_empty() {
                        self.content.push_str(&emitted);
                        text.push(emitted);
                        if self.content.len()
                            >= self
                                .content_loop_checked_at
                                .saturating_add(LOOP_CHECK_STRIDE_BYTES)
                        {
                            self.content_loop_checked_at = self.content.len();
                            if has_repetition_loop(&self.content) {
                                self.repetition_loop = true;
                                tracing::warn!(
                                    bytes = self.content.len(),
                                    "Aborted llama.cpp stream after detecting a degenerate repetition loop"
                                );
                                return Err(AppError::ModelAnswerUnusable(
                                    "The model entered a repetition loop before finishing its answer."
                                        .into(),
                                ));
                            }
                        }
                    }
                }
                if finish_reason.is_some() {
                    let tail = self.content_filter.flush();
                    if !tail.is_empty() {
                        self.content.push_str(&tail);
                        text.push(tail);
                    }
                }
                if was_empty
                    && !self.content.is_empty()
                    && self.first_logprobs.is_none()
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
                // Keep the reasoning for display; the replayed turn drops it.
                for key in ["reasoning_content", "reasoning"] {
                    if let Some(value) = choice
                        .get("delta")
                        .and_then(|d| d.get(key))
                        .and_then(Value::as_str)
                    {
                        if !value.is_empty() {
                            emitted_token = true;
                        }
                        self.reasoning_delta.push_str(value);
                        let entry = self.reasoning.entry(key.into()).or_default();
                        entry.push_str(value);
                        if entry.len()
                            >= self
                                .reasoning_loop_checked_at
                                .saturating_add(LOOP_CHECK_STRIDE_BYTES)
                        {
                            self.reasoning_loop_checked_at = entry.len();
                            if has_repetition_loop(entry) {
                                self.repetition_loop = true;
                                tracing::warn!(
                                    bytes = entry.len(),
                                    "Aborted llama.cpp stream after detecting a degenerate repetition loop in reasoning"
                                );
                                return Err(AppError::ModelAnswerUnusable(
                                    "The model entered a repetition loop before finishing its answer."
                                        .into(),
                                ));
                            }
                        }
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
                        if !self.tool_calls.contains_key(&index)
                            && self.tool_calls.len() >= self.budget.max_tool_calls
                        {
                            return Err(AppError::InvalidState(
                                "llama.cpp tool call count exceeds output budget".into(),
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
                                if !fragment.is_empty() {
                                    emitted_token = true;
                                }
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
                if emitted_token {
                    self.token_events = self.token_events.saturating_add(1);
                    if self.token_events > self.budget.max_token_events {
                        return Err(AppError::InvalidState(
                            "llama.cpp stream token events exceed output budget".into(),
                        ));
                    }
                }
            }
        }
        if self.buffer.len() > self.budget.max_event_bytes {
            return Err(AppError::Network(
                "llama.cpp event exceeds output budget size limit".into(),
            ));
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
    pub fn into_response(mut self) -> Result<CompletionResponse> {
        if !self.done && !self.buffer.is_empty() {
            let _ = self.push(b"\n")?;
        }
        self.finish()?;
        let timings = self.timings;
        let content = if self.content_filter.closed_unmatched_think() {
            self.content.trim_end().to_owned()
        } else {
            self.content
        };
        let mut message = serde_json::Map::from_iter([
            ("role".into(), json!("assistant")),
            ("content".into(), json!(content)),
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
        let public_logprobs = self.first_logprobs.as_ref().and_then(first_token_logprobs);
        let mut response = super::parse_completion_with_budget(
            json!({
                "choices": [{
                    "message": message,
                    "finish_reason": self.finish_reason.unwrap_or_else(|| if has_tools { "tool_calls" } else { "stop" }.into()),
                    "logprobs": self.first_logprobs,
                }],
                "usage": self.usage,
            }),
            self.budget.max_tool_calls,
        )?;
        response.first_token_logprobs = public_logprobs;
        response.timing = timings.map(|timings| timings.served());
        Ok(response)
    }
}
