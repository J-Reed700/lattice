//! OpenAI Chat Completions message normalization for llama-server requests.

use crate::features::llm::openai_chat::openai_content_parts;
use lattice_core::application::ports::llm_port::CompletionInput;
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};

/// A typed request's input as OpenAI chat messages.
///
/// Shared with the bundled sidecar, which is the same llama-server behind the
/// same `/v1/chat/completions` route: one translation means a tool round
/// replays identically whichever of the two the model is reached through.
pub(crate) fn chat_messages(input: &[CompletionInput]) -> Result<Vec<Value>> {
    let mut messages: Vec<Value> = Vec::with_capacity(input.len());
    // Whether the last message is a model turn that a call joins: its calls
    // so far, or the text it wrote before them.
    let mut calls_join_last = false;
    for item in input {
        let mut joins = false;
        match item {
            CompletionInput::Native { value } => messages.push(value.clone()),
            // Chat templates know system, user, assistant and tool; a
            // developer instruction is a system one to every one of them.
            CompletionInput::Message { role, content } if role == "developer" => {
                messages.push(json!({"role":"system","content":content}))
            }
            CompletionInput::Message { role, content } => {
                messages.push(json!({"role":role,"content":content}));
                joins = role == "assistant";
            }
            CompletionInput::Multimodal { role, parts } => {
                messages.push(json!({"role":role,"content":openai_content_parts(role, parts)?}))
            }
            CompletionInput::ToolCall {
                id,
                name,
                arguments,
            } => {
                let call = json!({"id":id,"type":"function",
                    "function":{"name":name,"arguments":arguments.to_string()}});
                // Calls made in one round are one assistant message, with
                // the text the model wrote before them. Replayed as messages
                // of their own, they break the role alternation that strict
                // chat templates enforce.
                match messages
                    .last_mut()
                    .filter(|_| calls_join_last)
                    .and_then(Value::as_object_mut)
                    .and_then(|turn| {
                        turn.entry("tool_calls")
                            .or_insert_with(|| Value::Array(Vec::new()))
                            .as_array_mut()
                    }) {
                    Some(calls) => calls.push(call),
                    None => messages
                        .push(json!({"role":"assistant","content":null,"tool_calls":[call]})),
                }
                joins = true;
            }
            CompletionInput::ToolResult { id, output } => {
                messages.push(json!({"role":"tool","tool_call_id":id,"content":output}))
            }
        }
        calls_join_last = joins;
    }
    coalesce_system_messages(messages)
}

/// The server's chat template may allow only one system message at index zero.
/// Normalize typed/native requests too, without changing history or tool payloads.
fn coalesce_system_messages(messages: Vec<Value>) -> Result<Vec<Value>> {
    let mut instructions = Vec::new();
    let mut conversation = Vec::new();
    for message in messages {
        if message.get("role").and_then(Value::as_str) == Some("system") {
            let content = message
                .get("content")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    AppError::InvalidInput("llama.cpp system instructions must be text".into())
                })?
                .trim();
            if !content.is_empty() && !instructions.iter().any(|existing| existing == content) {
                instructions.push(content.to_owned());
            }
        } else {
            conversation.push(message);
        }
    }
    if !instructions.is_empty() {
        conversation.insert(
            0,
            json!({"role":"system","content":instructions.join("\n\n")}),
        );
    }
    Ok(conversation)
}
