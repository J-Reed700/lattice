//! What the providers' reply formats share when read back into a completion.
use super::{field, timing};
use lattice_core::application::ports::llm_port::{invalid_tool_arguments, CompletionResponse};
use serde_json::{json, Value};

/// The text of a `message` block's parts, refusals included.
pub(super) fn push_message_text(text: &mut String, block: &Value) {
    if let Some(parts) = field(block, "content").as_array() {
        for part in parts {
            if let Some(part_text) = field(part, "text")
                .as_str()
                .or_else(|| field(part, "refusal").as_str())
            {
                text.push_str(part_text);
            }
        }
    }
}

/// The text of a `text` block.
pub(super) fn push_block_text(text: &mut String, block: &Value) {
    if let Some(block_text) = field(block, "text").as_str() {
        text.push_str(block_text);
    }
}

/// Usage, finish reason, reasoning and timing of a reply whose content
/// blocks are already read into `response`. The finish reason is the
/// reply's `status_key`, and `input_tokens` the whole prompt as the provider
/// counts it.
pub(super) fn finish(
    response: &mut CompletionResponse,
    value: &Value,
    status_key: &str,
    input_tokens: u64,
    reasoning_parts: &[String],
) {
    response.input_tokens = input_tokens;
    response.output_tokens = field(field(value, "usage"), "output_tokens")
        .as_u64()
        .unwrap_or(0);
    response.finish_reason = field(value, status_key)
        .as_str()
        .unwrap_or("unknown")
        .into();
    if !reasoning_parts.is_empty() {
        response.reasoning = Some(reasoning_parts.join("\n\n"));
    }
    response.timing = timing::reported(value);
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
