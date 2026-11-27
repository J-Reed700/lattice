//! Parsing Chat Completions JSON envelopes into typed [`CompletionResponse`] values.

use super::streaming;
#[cfg(test)]
use crate::features::llm::fitting::ResponseBudget;
use lattice_core::application::ports::llm_port::{
    first_token_logprobs, invalid_tool_arguments, CompletionInput, CompletionResponse,
};
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};
use std::collections::HashSet;

/// Stands in for the name of a call that gave none, so the replayed history
/// still has a name the chat template can render.
const UNNAMED_TOOL: &str = "unnamed_tool";

#[cfg(test)]
pub(crate) fn parse_completion(value: Value) -> Result<CompletionResponse> {
    parse_completion_with_budget(value, ResponseBudget::default().max_tool_calls)
}

pub(crate) fn parse_completion_with_budget(
    value: Value,
    max_tool_calls: usize,
) -> Result<CompletionResponse> {
    let choice = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|c| c.first())
        .ok_or_else(|| AppError::Network("llama.cpp returned no completion choices".into()))?;
    let message = choice
        .get("message")
        .filter(|m| m.is_object())
        .ok_or_else(|| AppError::Network("llama.cpp returned no completion message".into()))?;
    let finish_reason = choice
        .get("finish_reason")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let raw_calls: Vec<&Value> = message
        .get("tool_calls")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .collect();
    if raw_calls.len() > max_tool_calls {
        return Err(AppError::InvalidState(
            "llama.cpp tool call count exceeds output budget".into(),
        ));
    }
    let mut calls = Vec::new();
    let mut ids = HashSet::new();
    let mut repaired = false;
    for (index, call) in raw_calls.iter().enumerate() {
        // A small model gets a call wrong often enough that failing the whole
        // completion over it throws away a turn the model can recover: it is
        // handed back as a call the tool loop answers with the error, and the
        // model tries again next round.
        let function = call.get("function");
        let name = function
            .and_then(|f| f.get("name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty());
        let raw_arguments = function
            .and_then(|f| f.get("arguments"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let id = match call
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty() && !ids.contains(*id))
        {
            Some(id) => id.to_owned(),
            None => {
                repaired = true;
                format!("call_invalid_{index}")
            }
        };
        ids.insert(id.clone());
        // A call that parses whole was finished even when the round then hit
        // its output limit; only the one whose arguments broke off was cut.
        let cut_off = finish_reason == "length" && index + 1 == raw_calls.len();
        let problem = match (name, serde_json::from_str::<Value>(raw_arguments)) {
            (None, _) => Some("the call named no tool".to_owned()),
            (Some(name), Ok(arguments)) if arguments.is_object() => {
                calls.push(CompletionInput::ToolCall {
                    id: id.clone(),
                    name: name.into(),
                    arguments,
                });
                None
            }
            (Some(_), Ok(_)) => Some("arguments must be a JSON object".to_owned()),
            (Some(_), Err(error)) if cut_off => Some(format!(
                "arguments were cut off by the output limit before the JSON closed ({error})"
            )),
            (Some(_), Err(error)) => Some(format!("arguments were not valid JSON: {error}")),
        };
        if let Some(problem) = problem {
            repaired = true;
            calls.push(CompletionInput::ToolCall {
                id,
                name: name.unwrap_or(UNNAMED_TOOL).into(),
                arguments: invalid_tool_arguments(raw_arguments, &problem),
            });
        }
    }
    // The assistant message is replayed as the server returned it, so it has
    // to agree with the calls the tool results will answer: the same ids, and
    // arguments the chat template can parse again.
    let mut assistant = message.clone();
    if repaired {
        if let Some(object) = assistant.as_object_mut() {
            object.insert(
                "tool_calls".into(),
                Value::Array(
                    calls
                        .iter()
                        .filter_map(|call| match call {
                            CompletionInput::ToolCall {
                                id,
                                name,
                                arguments,
                            } => Some(json!({
                                "id": id, "type": "function",
                                "function": {"name": name, "arguments": arguments.to_string()}
                            })),
                            _ => None,
                        })
                        .collect(),
                ),
            );
        }
    }
    let raw_content = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let cleaned_content = streaming::strip_unmatched_think_close(raw_content);
    // Reasoning is shown, not replayed. Carried through a tool loop it soon
    // outweighs everything else in the prompt. Dropping it costs the server a
    // re-read of the newest turn's visible output, which it cached with the
    // reasoning still in place; that is small next to carrying every round's.
    if let Some(object) = assistant.as_object_mut() {
        object.remove("reasoning_content");
        object.remove("reasoning");
        if let Some(cleaned) = &cleaned_content {
            object.insert("content".into(), json!(cleaned));
        }
    }
    let reasoning = ["reasoning_content", "reasoning"]
        .iter()
        .find_map(|key| message.get(key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned);
    Ok(CompletionResponse {
        text: cleaned_content.unwrap_or_else(|| raw_content.to_owned()),
        reasoning,
        tool_calls: calls,
        finish_reason: finish_reason.into(),
        input_tokens: value
            .pointer("/usage/prompt_tokens")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        output_tokens: value
            .pointer("/usage/completion_tokens")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        replay: vec![CompletionInput::Native { value: assistant }],
        // Non-streaming servers can include reasoning tokens in this array.
        // Without token/channel alignment, do not treat them as verdict confidence.
        first_token_logprobs: choice
            .get("logprobs")
            .filter(|_| {
                !["reasoning_content", "reasoning"].iter().any(|key| {
                    message
                        .get(key)
                        .and_then(Value::as_str)
                        .is_some_and(|text| !text.is_empty())
                })
            })
            .and_then(first_token_logprobs),
        timing: value
            .get("timings")
            .and_then(streaming::ServerTimings::from_value)
            .map(|timings| timings.served()),
    })
}
