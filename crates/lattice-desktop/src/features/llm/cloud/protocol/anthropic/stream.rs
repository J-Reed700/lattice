//! Anthropic's Messages stream events.

use std::collections::{BTreeMap, BTreeSet};

use lattice_core::shared::error::Result;
use serde_json::{json, Value};

use super::super::EventDecoder;
use crate::features::llm::cloud::reply::call_arguments;
use crate::features::llm::cloud::streaming::{invalid, Callback, ReasoningParts};
use crate::features::llm::cloud::{field, set_field};

#[derive(Default)]
pub(super) struct Events {
    completed: Option<Value>,
    message: Option<Value>,
    blocks: BTreeMap<u64, Value>,
    open_blocks: BTreeSet<u64>,
    input_json: BTreeMap<u64, String>,
    reasoning: ReasoningParts,
}

impl EventDecoder for Events {
    fn event(
        &mut self,
        event: &Value,
        kind: &str,
        on_text: Callback<'_>,
        on_reasoning: Callback<'_>,
    ) -> Result<()> {
        match kind {
            "message_start" => {
                if self.message.is_some() || !field(event, "message").is_object() {
                    return Err(invalid("invalid message start"));
                }
                self.message = Some(field(event, "message").clone());
            }
            "content_block_start" => {
                let index = block_index(event)?;
                let block = field(event, "content_block");
                if self.message.is_none() || !block.is_object() || self.blocks.contains_key(&index)
                {
                    return Err(invalid("invalid content block start"));
                }
                match field(block, "type").as_str() {
                    Some("text") => {
                        if let Some(text) = field(block, "text")
                            .as_str()
                            .filter(|text| !text.is_empty())
                        {
                            on_text(text.to_owned())?;
                        }
                    }
                    Some("thinking") => {
                        if let Some(text) = field(block, "thinking")
                            .as_str()
                            .filter(|text| !text.is_empty())
                        {
                            self.reasoning
                                .push((String::new(), index), text, on_reasoning)?;
                        }
                    }
                    _ => {}
                }
                self.blocks.insert(index, block.clone());
                self.open_blocks.insert(index);
            }
            "content_block_delta" => {
                let index = block_index(event)?;
                if !self.open_blocks.contains(&index) {
                    return Err(invalid("delta for an unopened content block"));
                }
                let delta = field(event, "delta");
                let block = self
                    .blocks
                    .get_mut(&index)
                    .ok_or_else(|| invalid("missing content block"))?;
                match field(delta, "type").as_str() {
                    Some("text_delta") => {
                        let text = append(block, delta, "text")?;
                        if !text.is_empty() {
                            on_text(text)?;
                        }
                    }
                    Some("thinking_delta") => {
                        let text = append(block, delta, "thinking")?;
                        if !text.is_empty() {
                            self.reasoning
                                .push((String::new(), index), &text, on_reasoning)?;
                        }
                    }
                    Some("signature_delta") => {
                        append(block, delta, "signature")?;
                    }
                    Some("input_json_delta") => {
                        let partial = field(delta, "partial_json")
                            .as_str()
                            .ok_or_else(|| invalid("missing tool argument delta"))?;
                        self.input_json.entry(index).or_default().push_str(partial);
                    }
                    Some("citations_delta") => {
                        if !field(block, "citations").is_array() {
                            set_field(block, "citations", json!([]))?;
                        }
                        block
                            .get_mut("citations")
                            .and_then(Value::as_array_mut)
                            .ok_or_else(|| invalid("invalid citations"))?
                            .push(field(delta, "citation").clone());
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                let index = block_index(event)?;
                if !self.open_blocks.remove(&index) {
                    return Err(invalid("stop for an unopened content block"));
                }
                // A call with no arguments can stream nothing but empty
                // deltas, and is `{}`. A call cut off or malformed is answered
                // with why instead of failing the whole reply as a broken
                // stream.
                if let Some(input) = self.input_json.remove(&index) {
                    let input = call_arguments(&input);
                    let block = self
                        .blocks
                        .get_mut(&index)
                        .ok_or_else(|| invalid("missing tool block"))?;
                    set_field(block, "input", input)?;
                }
            }
            "message_delta" => {
                let message = self
                    .message
                    .as_mut()
                    .ok_or_else(|| invalid("message delta before start"))?;
                if let Some(delta) = field(event, "delta").as_object() {
                    for (key, value) in delta {
                        set_field(message, key, value.clone())?;
                    }
                }
                if let Some(usage) = field(event, "usage").as_object() {
                    if !field(message, "usage").is_object() {
                        set_field(message, "usage", json!({}))?;
                    }
                    let total = message
                        .get_mut("usage")
                        .ok_or_else(|| invalid("missing usage"))?;
                    for (key, value) in usage {
                        set_field(total, key, value.clone())?;
                    }
                }
            }
            "message_stop" => {
                if !self.open_blocks.is_empty() {
                    return Err(invalid("message ended with unfinished content blocks"));
                }
                let mut message = self
                    .message
                    .take()
                    .ok_or_else(|| invalid("message stop before start"))?;
                if !field(&message, "stop_reason").is_string() {
                    return Err(invalid("missing terminal stop reason"));
                }
                set_field(
                    &mut message,
                    "content",
                    json!(self.blocks.values().collect::<Vec<_>>()),
                )?;
                self.completed = Some(message);
            }
            _ => {}
        }
        Ok(())
    }

    fn is_complete(&self) -> bool {
        self.completed.is_some()
    }

    fn is_failure(&self, kind: &str) -> bool {
        kind == "error"
    }

    fn has_tool_output(&self) -> bool {
        !self.input_json.is_empty()
    }

    fn answer_begun(&self) -> bool {
        !self.blocks.is_empty()
    }

    fn finish(self: Box<Self>) -> Result<Option<Value>> {
        Ok(self.completed)
    }
}

fn block_index(event: &Value) -> Result<u64> {
    field(event, "index")
        .as_u64()
        .ok_or_else(|| invalid("invalid content block index"))
}

fn append(block: &mut Value, delta: &Value, key: &str) -> Result<String> {
    let text = field(delta, key)
        .as_str()
        .ok_or_else(|| invalid("missing text delta"))?;
    let mut complete = field(block, key).as_str().unwrap_or_default().to_owned();
    complete.push_str(text);
    set_field(block, key, json!(complete))?;
    Ok(text.to_owned())
}
