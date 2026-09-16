//! Byte-safe SSE decoder; failed or truncated generations must not look successful.
use crate::shared::error::{AppError, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Decoder {
    buffer: Vec<u8>,
    received_bytes: usize,
    done: bool,
    finish_reason: Option<String>,
    allow_tool_calls: bool,
    content: String,
    reasoning: BTreeMap<String, String>,
    tool_calls: BTreeMap<u64, Value>,
    usage: Value,
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
            if event.get("error").is_some() {
                return Err(AppError::Network(
                    "llama.cpp reported a generation error".into(),
                ));
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
                    if reason != "stop" && !(self.allow_tool_calls && reason == "tool_calls") {
                        return Err(AppError::Network(format!(
                            "llama.cpp generation stopped: {reason}"
                        )));
                    }
                    self.finish_reason = Some(reason.into());
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
        super::parse_completion(json!({
            "choices": [{"message": message, "finish_reason": self.finish_reason.unwrap_or_else(|| if has_tools { "tool_calls" } else { "stop" }.into())}],
            "usage": self.usage,
        }))
    }
}
