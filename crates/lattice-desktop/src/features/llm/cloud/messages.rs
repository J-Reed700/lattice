//! A typed request's input in each cloud provider's message format: OpenAI
//! Responses `input` items, Gemini's OpenAI-compatible chat messages, and
//! Anthropic Messages with the system prompt lifted out.

use lattice_core::application::ports::llm_port::{CompletionInput, ContentPart};
use lattice_core::shared::error::Result;
use serde_json::{json, Value};

use super::field;
use crate::features::llm::engine::vision::pictures_outside_a_user_message;
use crate::features::llm::llama_cpp::openai_content_parts;

/// OpenAI Responses `input` items.
pub(super) fn openai_input(input: &[CompletionInput]) -> Result<Vec<Value>> {
    input
        .iter()
        .map(|item| {
            Ok(match item {
                CompletionInput::Native { value } => value.clone(),
                CompletionInput::Message { role, content } => {
                    json!({"role":role,"content":content})
                }
                CompletionInput::Multimodal { role, parts } => {
                    let content = user_parts(role, parts, |part| match part {
                        ContentPart::Text { text } => json!({"type":"input_text","text":text}),
                        ContentPart::Image(image) => json!({
                            "type":"input_image",
                            "image_url":image.data_uri(),
                            "detail":"auto"
                        }),
                    })?;
                    json!({"role":role,"content":content})
                }
                CompletionInput::ToolCall {
                    id,
                    name,
                    arguments,
                } => {
                    json!({"type":"function_call","call_id":id,"name":name,"arguments":arguments.to_string()})
                }
                CompletionInput::ToolResult { id, output } => {
                    json!({"type":"function_call_output","call_id":id,"output":output})
                }
            })
        })
        .collect()
}

/// Gemini's OpenAI-compatible Chat Completions messages. Calls made in one
/// round are one assistant message.
pub(super) fn chat_messages(input: &[CompletionInput]) -> Result<Vec<Value>> {
    let mut messages: Vec<Value> = Vec::new();
    for item in input {
        match item {
            CompletionInput::Native { value } => messages.push(value.clone()),
            CompletionInput::Message { role, content } => {
                messages.push(json!({"role": role, "content": content}));
            }
            CompletionInput::Multimodal { role, parts } => {
                messages.push(json!({"role": role, "content": openai_content_parts(role, parts)?}));
            }
            CompletionInput::ToolCall {
                id,
                name,
                arguments,
            } => {
                let call = json!({
                    "id": id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": arguments.to_string(),
                    }
                });
                if let Some(last) = messages.last_mut().filter(|m| {
                    field(m, "role") == "assistant" && field(m, "tool_calls").is_array()
                }) {
                    if let Some(calls) = last.get_mut("tool_calls").and_then(Value::as_array_mut) {
                        calls.push(call);
                    }
                } else {
                    messages.push(json!({
                        "role": "assistant",
                        "tool_calls": [call],
                    }));
                }
            }
            CompletionInput::ToolResult { id, output } => {
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": id,
                    "content": output,
                }));
            }
        }
    }
    Ok(messages)
}

/// Anthropic Messages: the system and developer instructions, then the
/// messages. Tool results that follow one another share one user message.
pub(super) fn anthropic_messages(input: &[CompletionInput]) -> Result<(Vec<String>, Vec<Value>)> {
    let mut system = Vec::new();
    let mut messages: Vec<Value> = Vec::new();
    for item in input {
        match item {
            CompletionInput::Native { value } => messages.push(value.clone()),
            CompletionInput::Message { role, content } if role == "system" || role == "developer" => {
                system.push(content.clone())
            }
            CompletionInput::Message { role, content } => {
                messages.push(json!({"role":role,"content":content}))
            }
            CompletionInput::Multimodal { role, parts } => {
                let content = user_parts(role, parts, |part| match part {
                    ContentPart::Text { text } => json!({"type":"text","text":text}),
                    ContentPart::Image(image) => json!({
                        "type":"image",
                        "source":{"type":"base64","media_type":image.mime,"data":image.data_base64}
                    }),
                })?;
                messages.push(json!({"role":role,"content":content}))
            }
            CompletionInput::ToolCall {
                id,
                name,
                arguments,
            } => messages.push(
                json!({"role":"assistant","content":[{"type":"tool_use","id":id,"name":name,"input":arguments}]}),
            ),
            CompletionInput::ToolResult { id, output } => {
                let result = json!({"type":"tool_result","tool_use_id":id,"content":output});
                if let Some(last) = messages
                    .last_mut()
                    .filter(|m| field(m, "role") == "user" && field(m, "content").is_array())
                {
                    if let Some(content) = last.get_mut("content").and_then(Value::as_array_mut) {
                        content.push(result);
                    }
                } else {
                    messages.push(json!({"role":"user","content":[result]}));
                }
            }
        }
    }
    Ok((system, messages))
}

/// A user message's parts in a provider's own part format, in the order
/// given. Pictures in any other role are refused.
fn user_parts(
    role: &str,
    parts: &[ContentPart],
    part: impl Fn(&ContentPart) -> Value,
) -> Result<Vec<Value>> {
    if role != "user" {
        return Err(pictures_outside_a_user_message(role));
    }
    Ok(parts.iter().map(part).collect())
}
