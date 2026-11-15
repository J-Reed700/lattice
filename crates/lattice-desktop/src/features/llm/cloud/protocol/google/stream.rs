//! Gemini's OpenAI-compatible chat completion stream chunks.

use std::collections::BTreeMap;

use lattice_core::shared::error::Result;
use serde_json::{json, Value};

use super::super::EventDecoder;
use crate::features::llm::cloud::streaming::{Callback, ReasoningParts};
use crate::features::llm::cloud::{field, set_field};

#[derive(Default)]
pub(super) struct Events {
    tool_calls: BTreeMap<u64, ToolCallDraft>,
    text: String,
    reasoning_text: String,
    finish_reason: Option<String>,
    usage: Option<Value>,
    /// A llama.cpp-style `timings` object, which an OpenAI-compatible
    /// llama-server sends with its last chunk.
    timings: Option<Value>,
    reasoning: ReasoningParts,
    begun: bool,
}

#[derive(Default)]
struct ToolCallDraft {
    id: String,
    name: String,
    arguments: String,
    /// Gemini 3's `extra_content` (its `google.thought_signature`), which the
    /// next round must send back with the call or be refused.
    extra_content: serde_json::Map<String, Value>,
}

impl EventDecoder for Events {
    fn event(
        &mut self,
        event: &Value,
        _kind: &str,
        on_text: Callback<'_>,
        on_reasoning: Callback<'_>,
    ) -> Result<()> {
        if let Some(usage) = field(event, "usage").as_object() {
            self.usage = Some(Value::Object(usage.clone()));
        }
        if let Some(timings) = event.get("timings").filter(|timings| timings.is_object()) {
            self.timings = Some(timings.clone());
        }
        let choices = match field(event, "choices").as_array() {
            Some(choices) => choices,
            None => return Ok(()),
        };
        for choice in choices {
            if let Some(reason) = field(choice, "finish_reason").as_str() {
                self.finish_reason = Some(reason.to_string());
            }
            let delta = field(choice, "delta");
            if !delta.is_object() {
                continue;
            }
            if let Some(text) = field(delta, "content").as_str().filter(|t| !t.is_empty()) {
                self.begun = true;
                self.text.push_str(text);
                on_text(text.to_owned())?;
            }
            if let Some(reasoning) = field(delta, "reasoning_content")
                .as_str()
                .or_else(|| field(delta, "reasoning").as_str())
                .or_else(|| field(delta, "thought").as_str())
                .filter(|t| !t.is_empty())
            {
                self.begun = true;
                self.reasoning_text.push_str(reasoning);
                self.reasoning
                    .push((String::new(), 0), reasoning, on_reasoning)?;
            }
            if let Some(tool_calls) = field(delta, "tool_calls").as_array() {
                self.begun |= !tool_calls.is_empty();
                for call in tool_calls {
                    let index = field(call, "index").as_u64().unwrap_or(0);
                    let draft = self.tool_calls.entry(index).or_default();
                    if let Some(id) = field(call, "id").as_str() {
                        draft.id.push_str(id);
                    }
                    let function = field(call, "function");
                    if let Some(name) = field(function, "name").as_str() {
                        draft.name.push_str(name);
                    }
                    if let Some(args) = field(function, "arguments").as_str() {
                        draft.arguments.push_str(args);
                    }
                    if let Some(extra) = field(call, "extra_content").as_object() {
                        draft.extra_content.extend(extra.clone());
                    }
                }
            }
        }
        Ok(())
    }

    /// Chunks carry no terminal event: the stream's end completes it.
    fn is_complete(&self) -> bool {
        false
    }

    /// Chunks carry no `type`, so no event is a failure.
    fn is_failure(&self, _kind: &str) -> bool {
        false
    }

    fn has_tool_output(&self) -> bool {
        !self.tool_calls.is_empty()
    }

    fn answer_begun(&self) -> bool {
        self.begun
    }

    fn finish(mut self: Box<Self>) -> Result<Option<Value>> {
        if self.finish_reason.is_none() && self.text.is_empty() && self.tool_calls.is_empty() {
            return Ok(None);
        }
        let mut tool_calls_json = Vec::new();
        for (_idx, draft) in std::mem::take(&mut self.tool_calls) {
            let mut call = json!({
                "id": draft.id,
                "type": "function",
                "function": {
                    "name": draft.name,
                    "arguments": draft.arguments,
                }
            });
            if !draft.extra_content.is_empty() {
                set_field(
                    &mut call,
                    "extra_content",
                    Value::Object(draft.extra_content),
                )?;
            }
            tool_calls_json.push(call);
        }
        let mut message = json!({
            "role": "assistant",
            "content": self.text,
        });
        if !self.reasoning_text.is_empty() {
            set_field(
                &mut message,
                "reasoning_content",
                json!(self.reasoning_text),
            )?;
        }
        if !tool_calls_json.is_empty() {
            set_field(&mut message, "tool_calls", json!(tool_calls_json))?;
        }
        let finish_reason = self.finish_reason.take().unwrap_or_else(|| {
            if !tool_calls_json.is_empty() {
                "tool_calls".to_string()
            } else {
                "stop".to_string()
            }
        });
        let mut completed = json!({
            "choices": [
                {
                    "message": message,
                    "finish_reason": finish_reason,
                }
            ],
            "usage": self.usage.take().unwrap_or(json!({
                "prompt_tokens": 0,
                "completion_tokens": 0,
            }))
        });
        if let Some(timings) = self.timings.take() {
            set_field(&mut completed, "timings", timings)?;
        }
        Ok(Some(completed))
    }
}
