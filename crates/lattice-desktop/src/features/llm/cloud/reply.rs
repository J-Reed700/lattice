//! A whole reply, streamed or not, read back into a completion.
use super::{field, timing};
use lattice_core::application::{
    contracts::settings::LLMProvider,
    ports::llm_port::{invalid_tool_arguments, CompletionInput, CompletionResponse},
};
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};

pub(super) fn parse_completion(provider: LLMProvider, value: Value) -> Result<CompletionResponse> {
    if provider == LLMProvider::Google {
        let mut response = CompletionResponse::default();
        let choices = field(&value, "choices")
            .as_array()
            .ok_or_else(|| AppError::Network("Missing cloud response choices".into()))?;
        let first_choice = choices
            .first()
            .ok_or_else(|| AppError::Network("Empty choices in cloud response".into()))?;
        let message = field(first_choice, "message");
        if let Some(content) = field(message, "content").as_str() {
            response.text.push_str(content);
        }
        if let Some(reasoning) = field(message, "reasoning_content")
            .as_str()
            .or_else(|| field(message, "reasoning").as_str())
            .or_else(|| field(message, "thought").as_str())
            .filter(|t| !t.trim().is_empty())
        {
            response.reasoning = Some(reasoning.to_string());
        }
        if let Some(tool_calls) = field(message, "tool_calls").as_array() {
            for call in tool_calls {
                let id = field(call, "id")
                    .as_str()
                    .ok_or_else(|| AppError::Network("Missing tool call ID".into()))?;
                let function = field(call, "function");
                let name = field(function, "name")
                    .as_str()
                    .ok_or_else(|| AppError::Network("Missing tool name".into()))?;
                let args_val = field(function, "arguments");
                let arguments = if let Some(args_str) = args_val.as_str() {
                    call_arguments(args_str)
                } else if args_val.is_object() {
                    args_val.clone()
                } else {
                    json!({})
                };
                response.tool_calls.push(CompletionInput::ToolCall {
                    id: id.into(),
                    name: name.into(),
                    arguments,
                });
            }
        }
        let usage = field(&value, "usage");
        response.input_tokens = field(usage, "prompt_tokens").as_u64().unwrap_or(0);
        response.output_tokens = field(usage, "completion_tokens").as_u64().unwrap_or(0);
        let mut finish_reason = field(first_choice, "finish_reason")
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        if finish_reason == "stop" && !response.tool_calls.is_empty() {
            finish_reason = "tool_calls".to_string();
        }
        response.finish_reason = finish_reason;
        response.replay = vec![CompletionInput::Native {
            value: message.clone(),
        }];
        response.timing = timing::reported(&value);
        return Ok(response);
    }
    let mut response = CompletionResponse::default();
    let mut reasoning_parts = Vec::new();
    let key = if provider == LLMProvider::Openai {
        "output"
    } else {
        "content"
    };
    let blocks = field(&value, key)
        .as_array()
        .ok_or_else(|| AppError::Network("Missing cloud response content".into()))?;
    for block in blocks {
        match field(block, "type").as_str().unwrap_or("") {
            "message" => {
                if let Some(parts) = field(block, "content").as_array() {
                    for part in parts {
                        if let Some(text) = field(part, "text")
                            .as_str()
                            .or_else(|| field(part, "refusal").as_str())
                        {
                            response.text.push_str(text);
                        }
                    }
                }
            }
            "text" => {
                if let Some(text) = field(block, "text").as_str() {
                    response.text.push_str(text);
                }
            }
            "reasoning" if provider == LLMProvider::Openai => {
                if let Some(summary) = field(block, "summary").as_array() {
                    reasoning_parts.extend(summary.iter().filter_map(|part| {
                        field(part, "text")
                            .as_str()
                            .map(str::trim)
                            .filter(|text| !text.is_empty())
                            .map(str::to_owned)
                    }));
                }
            }
            "thinking" if provider == LLMProvider::Anthropic => {
                if let Some(thinking) = field(block, "thinking")
                    .as_str()
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                {
                    reasoning_parts.push(thinking.to_owned());
                }
            }
            "function_call" | "tool_use" => {
                let id = field(
                    block,
                    if provider == LLMProvider::Openai {
                        "call_id"
                    } else {
                        "id"
                    },
                )
                .as_str()
                .ok_or_else(|| AppError::Network("Missing tool call ID".into()))?;
                let name = field(block, "name")
                    .as_str()
                    .ok_or_else(|| AppError::Network("Missing tool name".into()))?;
                let arguments = if provider == LLMProvider::Openai {
                    call_arguments(field(block, "arguments").as_str().unwrap_or(""))
                } else {
                    field(block, "input").clone()
                };
                response.tool_calls.push(CompletionInput::ToolCall {
                    id: id.into(),
                    name: name.into(),
                    arguments,
                });
            }
            _ => {}
        }
    }
    // The whole prompt: Anthropic counts cache reads and writes apart from
    // `input_tokens`.
    response.input_tokens = [
        "input_tokens",
        "cache_creation_input_tokens",
        "cache_read_input_tokens",
    ]
    .iter()
    .filter_map(|key| field(field(&value, "usage"), key).as_u64())
    .sum();
    response.output_tokens = field(field(&value, "usage"), "output_tokens")
        .as_u64()
        .unwrap_or(0);
    response.finish_reason = field(
        &value,
        if provider == LLMProvider::Openai {
            "status"
        } else {
            "stop_reason"
        },
    )
    .as_str()
    .unwrap_or("unknown")
    .into();
    if response.finish_reason == "refusal" {
        // Claude's safety classifiers decline with a 200 and name the one
        // that did; the reply itself says nothing worth logging.
        tracing::warn!(
            category = field(field(&value, "stop_details"), "category")
                .as_str()
                .unwrap_or("unnamed"),
            "The model declined the request"
        );
    }
    if !reasoning_parts.is_empty() {
        response.reasoning = Some(reasoning_parts.join("\n\n"));
    }
    response.timing = timing::reported(&value);
    // OpenAI replays its output items one by one; Anthropic replays the
    // content blocks as the assistant message they were, signed thinking
    // included.
    response.replay = match (provider, field(&value, key)) {
        (LLMProvider::Openai, Value::Array(items)) => items
            .iter()
            .cloned()
            .map(|value| CompletionInput::Native { value })
            .collect(),
        (_, blocks) => vec![CompletionInput::Native {
            value: json!({"role": "assistant", "content": blocks}),
        }],
    };
    Ok(response)
}

/// A call's arguments as the provider wrote them. None at all is `{}`, which
/// a call that takes no arguments is; text that is not JSON (a call cut off
/// at the output limit) is answered with why, as the local adapter answers
/// one, rather than failing the whole reply as an outage.
pub(super) fn call_arguments(raw: &str) -> Value {
    if raw.trim().is_empty() {
        return json!({});
    }
    serde_json::from_str(raw).unwrap_or_else(|error| {
        invalid_tool_arguments(raw, &format!("its arguments are not JSON ({error})"))
    })
}
