//! Typed cloud streams publish only displayable text through callbacks while
//! retaining the exact native completion—including opaque replay state—in the
//! returned response.
use super::timing::{evaluated_prompt_tokens, StreamClock};
use super::{field, parse_completion, set_field, CompletionResponse, LLMProvider};
use super::reply::call_arguments;
use futures::{Stream, StreamExt};
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

type Callback<'a> = &'a (dyn Fn(String) -> Result<()> + Send + Sync);

/// Read a stream to its completion. The request went out at `sent`; unless
/// the completion carries the server's own timings, the first streamed delta
/// splits the time since into reading the prompt and writing the answer.
pub(super) async fn drain<S, B, E>(
    provider: LLMProvider,
    mut bytes: S,
    stall_timeout: Duration,
    sent: Instant,
    on_text: Callback<'_>,
    on_reasoning: Callback<'_>,
) -> Result<CompletionResponse>
where
    S: Stream<Item = std::result::Result<B, E>> + Unpin,
    B: AsRef<[u8]>,
    E: std::fmt::Display,
{
    let clock = StreamClock::started(sent);
    let generating = AtomicBool::new(false);
    let timed_text = |text: String| {
        clock.delta();
        generating.store(true, Ordering::Relaxed);
        on_text(text)
    };
    let timed_reasoning = |text: String| {
        clock.delta();
        generating.store(true, Ordering::Relaxed);
        on_reasoning(text)
    };
    let (on_text, on_reasoning): (Callback<'_>, Callback<'_>) = (&timed_text, &timed_reasoning);
    let mut decoder = Decoder::new(provider);
    loop {
        // Silence before the first delta is the model reading the prompt or
        // thinking, which a reasoning model can do for minutes with nothing to
        // send: Claude streams no thinking unless asked to, and an OpenAI
        // reasoning model without a summary sends only keepalives. That wait
        // is bounded by the turn's deadline, Stop and shutdown. Once the
        // answer is streaming, a silence this long is a stall.
        let armed = generating.load(Ordering::Relaxed) || decoder.has_tool_output();
        let next = if armed {
            tokio::time::timeout(stall_timeout, bytes.next())
                .await
                .map_err(|_| {
                    AppError::Network(format!(
                        "Cloud stream stalled for {}s",
                        stall_timeout.as_secs()
                    ))
                })?
        } else {
            bytes.next().await
        };
        let Some(chunk) = next else { break };
        let chunk = chunk
            .map_err(|error| AppError::Network(format!("Cloud stream interrupted: {error}")))?;
        decoder.push(chunk.as_ref(), on_text, on_reasoning)?;
        if decoder.completed.is_some() {
            break;
        }
    }
    let completed = decoder.into_completed(on_text, on_reasoning)?;
    let prompt_tokens = evaluated_prompt_tokens(&completed);
    let mut response = parse_completion(provider, completed)?;
    if response.timing.is_none() {
        response.timing = clock.timing(prompt_tokens, response.output_tokens);
    }
    Ok(response)
}

struct Decoder {
    provider: LLMProvider,
    buffer: Vec<u8>,
    data: String,
    received: usize,
    completed: Option<Value>,
    message: Option<Value>,
    blocks: BTreeMap<u64, Value>,
    open_blocks: BTreeSet<u64>,
    input_json: BTreeMap<u64, String>,
    reasoning_part: Option<(String, u64)>,
    google_tool_calls: BTreeMap<u64, GoogleToolCallDraft>,
    google_text: String,
    google_reasoning: String,
    google_finish_reason: Option<String>,
    google_usage: Option<Value>,
    /// A llama.cpp-style `timings` object, which an OpenAI-compatible
    /// llama-server sends with its last chunk.
    google_timings: Option<Value>,
}

#[derive(Default)]
struct GoogleToolCallDraft {
    id: String,
    name: String,
    arguments: String,
    /// Gemini 3's `extra_content` (its `google.thought_signature`), which the
    /// next round must send back with the call or be refused.
    extra_content: serde_json::Map<String, Value>,
}

fn invalid(detail: &str) -> AppError {
    AppError::Network(format!("Invalid cloud stream: {detail}"))
}

impl Decoder {
    const MAX_EVENT_BYTES: usize = 4 * 1024 * 1024;
    const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

    fn new(provider: LLMProvider) -> Self {
        Self {
            provider,
            buffer: Vec::new(),
            data: String::new(),
            received: 0,
            completed: None,
            message: None,
            blocks: BTreeMap::new(),
            open_blocks: BTreeSet::new(),
            input_json: BTreeMap::new(),
            reasoning_part: None,
            google_tool_calls: BTreeMap::new(),
            google_text: String::new(),
            google_reasoning: String::new(),
            google_finish_reason: None,
            google_usage: None,
            google_timings: None,
        }
    }

    /// Whether a tool call has started streaming its arguments, which counts
    /// as the answer having started, as text does.
    fn has_tool_output(&self) -> bool {
        !self.input_json.is_empty() || !self.google_tool_calls.is_empty()
    }

    fn push(
        &mut self,
        bytes: &[u8],
        on_text: Callback<'_>,
        on_reasoning: Callback<'_>,
    ) -> Result<()> {
        self.received = self.received.saturating_add(bytes.len());
        if self.received > Self::MAX_RESPONSE_BYTES {
            return Err(invalid("response exceeds size limit"));
        }
        self.buffer.extend_from_slice(bytes);
        while let Some(end) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<_> = self.buffer.drain(..=end).collect();
            self.line(&line, on_text, on_reasoning)?;
            if self.completed.is_some() {
                break;
            }
        }
        if self.buffer.len().saturating_add(self.data.len()) > Self::MAX_EVENT_BYTES {
            return Err(invalid("event exceeds size limit"));
        }
        Ok(())
    }

    fn line(
        &mut self,
        bytes: &[u8],
        on_text: Callback<'_>,
        on_reasoning: Callback<'_>,
    ) -> Result<()> {
        let line = std::str::from_utf8(bytes)
            .map_err(|_| invalid("invalid UTF-8"))?
            .trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            return self.dispatch(on_text, on_reasoning);
        }
        if let Some(data) = line.strip_prefix("data:") {
            if !self.data.is_empty() {
                self.data.push('\n');
            }
            self.data.push_str(data.strip_prefix(' ').unwrap_or(data));
            if self.data.len() > Self::MAX_EVENT_BYTES {
                return Err(invalid("event exceeds size limit"));
            }
        }
        Ok(())
    }

    fn dispatch(&mut self, on_text: Callback<'_>, on_reasoning: Callback<'_>) -> Result<()> {
        let data = std::mem::take(&mut self.data);
        if data.is_empty() || data == "[DONE]" {
            return Ok(());
        }
        let event: Value =
            serde_json::from_str(&data).map_err(|_| invalid("invalid event JSON"))?;
        let kind = field(&event, "type").as_str().unwrap_or("");
        if matches!(kind, "error" | "response.failed") {
            // Provider error text may contain the user's prompt.
            return Err(AppError::Network(format!("Cloud stream failed ({kind})")));
        }
        if self.provider == LLMProvider::Openai {
            match kind {
                "response.output_text.delta" | "response.refusal.delta" => {
                    if let Some(text) = field(&event, "delta")
                        .as_str()
                        .filter(|text| !text.is_empty())
                    {
                        on_text(text.to_owned())?;
                    }
                }
                "response.reasoning_summary_text.delta" => {
                    if let Some(text) = field(&event, "delta")
                        .as_str()
                        .filter(|text| !text.is_empty())
                    {
                        let part = (
                            field(&event, "item_id").as_str().unwrap_or("").to_owned(),
                            field(&event, "summary_index").as_u64().unwrap_or(0),
                        );
                        self.reasoning(part, text, on_reasoning)?;
                    }
                }
                "response.completed" | "response.incomplete" => {
                    let response = field(&event, "response");
                    if !response.is_object() || !field(response, "output").is_array() {
                        return Err(invalid("missing terminal response"));
                    }
                    // The terminal response contains complete tool arguments,
                    // encrypted reasoning, annotations and final token usage.
                    self.completed = Some(response.clone());
                }
                _ => {}
            }
        } else if self.provider == LLMProvider::Google {
            self.google(&event, on_text, on_reasoning)?;
        } else {
            self.anthropic(&event, kind, on_text, on_reasoning)?;
        }
        Ok(())
    }

    fn google(
        &mut self,
        event: &Value,
        on_text: Callback<'_>,
        on_reasoning: Callback<'_>,
    ) -> Result<()> {
        if let Some(usage) = field(event, "usage").as_object() {
            self.google_usage = Some(Value::Object(usage.clone()));
        }
        if let Some(timings) = event.get("timings").filter(|timings| timings.is_object()) {
            self.google_timings = Some(timings.clone());
        }
        let choices = match field(event, "choices").as_array() {
            Some(choices) => choices,
            None => return Ok(()),
        };
        for choice in choices {
            if let Some(reason) = field(choice, "finish_reason").as_str() {
                self.google_finish_reason = Some(reason.to_string());
            }
            let delta = field(choice, "delta");
            if !delta.is_object() {
                continue;
            }
            if let Some(text) = field(delta, "content").as_str().filter(|t| !t.is_empty()) {
                self.google_text.push_str(text);
                on_text(text.to_owned())?;
            }
            if let Some(reasoning) = field(delta, "reasoning_content")
                .as_str()
                .or_else(|| field(delta, "reasoning").as_str())
                .or_else(|| field(delta, "thought").as_str())
                .filter(|t| !t.is_empty())
            {
                self.google_reasoning.push_str(reasoning);
                self.reasoning((String::new(), 0), reasoning, on_reasoning)?;
            }
            if let Some(tool_calls) = field(delta, "tool_calls").as_array() {
                for call in tool_calls {
                    let index = field(call, "index").as_u64().unwrap_or(0);
                    let draft = self.google_tool_calls.entry(index).or_default();
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

    fn reasoning(&mut self, part: (String, u64), text: &str, callback: Callback<'_>) -> Result<()> {
        if self
            .reasoning_part
            .as_ref()
            .is_some_and(|previous| previous != &part)
        {
            callback("\n\n".into())?;
        }
        self.reasoning_part = Some(part);
        callback(text.to_owned())
    }

    fn anthropic(
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
                            self.reasoning((String::new(), index), text, on_reasoning)?;
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
                            self.reasoning((String::new(), index), &text, on_reasoning)?;
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

    #[cfg(test)]
    fn finish(
        self,
        on_text: Callback<'_>,
        on_reasoning: Callback<'_>,
    ) -> Result<CompletionResponse> {
        let provider = self.provider;
        parse_completion(provider, self.into_completed(on_text, on_reasoning)?)
    }

    /// The completed response in the provider's own shape, once the stream
    /// has ended.
    fn into_completed(
        mut self,
        on_text: Callback<'_>,
        on_reasoning: Callback<'_>,
    ) -> Result<Value> {
        if self.completed.is_none() {
            let remaining = std::mem::take(&mut self.buffer);
            self.line(&remaining, on_text, on_reasoning)?;
            self.dispatch(on_text, on_reasoning)?;
        }
        if self.provider == LLMProvider::Google
            && self.completed.is_none()
            && (self.google_finish_reason.is_some()
                || !self.google_text.is_empty()
                || !self.google_tool_calls.is_empty())
        {
            let mut tool_calls_json = Vec::new();
            for (_idx, draft) in std::mem::take(&mut self.google_tool_calls) {
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
                "content": self.google_text,
            });
            if !self.google_reasoning.is_empty() {
                set_field(
                    &mut message,
                    "reasoning_content",
                    json!(self.google_reasoning),
                )?;
            }
            if !tool_calls_json.is_empty() {
                set_field(&mut message, "tool_calls", json!(tool_calls_json))?;
            }
            let finish_reason = self.google_finish_reason.unwrap_or_else(|| {
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
                "usage": self.google_usage.unwrap_or(json!({
                    "prompt_tokens": 0,
                    "completion_tokens": 0,
                }))
            });
            if let Some(timings) = self.google_timings.take() {
                set_field(&mut completed, "timings", timings)?;
            }
            self.completed = Some(completed);
        }
        self.completed
            .ok_or_else(|| invalid("stream ended before completion"))
    }
}

fn block_index(event: &Value) -> Result<u64> {
    field(event, "index")
        .as_u64()
        .filter(|index| *index < 1024)
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

#[cfg(test)]
mod tests;
