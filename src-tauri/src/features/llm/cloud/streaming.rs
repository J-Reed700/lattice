//! Typed cloud streams publish only displayable text through callbacks while
//! retaining the exact native completion—including opaque replay state—in the
//! returned response.
use super::{field, parse_completion, set_field, CompletionResponse, LLMProvider};
use crate::shared::error::{AppError, Result};
use futures::{Stream, StreamExt};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

type Callback<'a> = &'a (dyn Fn(String) -> Result<()> + Send + Sync);

pub(super) async fn drain<S, B, E>(
    provider: LLMProvider,
    mut bytes: S,
    stall_timeout: Duration,
    on_text: Callback<'_>,
    on_reasoning: Callback<'_>,
) -> Result<CompletionResponse>
where
    S: Stream<Item = std::result::Result<B, E>> + Unpin,
    B: AsRef<[u8]>,
    E: std::fmt::Display,
{
    let mut decoder = Decoder::new(provider);
    loop {
        let next = tokio::time::timeout(stall_timeout, bytes.next())
            .await
            .map_err(|_| {
                AppError::Network(format!(
                    "Cloud stream stalled for {}s",
                    stall_timeout.as_secs()
                ))
            })?;
        let Some(chunk) = next else { break };
        let chunk = chunk
            .map_err(|error| AppError::Network(format!("Cloud stream interrupted: {error}")))?;
        decoder.push(chunk.as_ref(), on_text, on_reasoning)?;
        if decoder.completed.is_some() {
            break;
        }
    }
    decoder.finish(on_text, on_reasoning)
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
        }
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
        } else {
            self.anthropic(&event, kind, on_text, on_reasoning)?;
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
                if let Some(input) = self.input_json.remove(&index) {
                    let input: Value = serde_json::from_str(&input)
                        .map_err(|_| invalid("invalid tool arguments"))?;
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

    fn finish(
        mut self,
        on_text: Callback<'_>,
        on_reasoning: Callback<'_>,
    ) -> Result<CompletionResponse> {
        if self.completed.is_none() {
            let remaining = std::mem::take(&mut self.buffer);
            self.line(&remaining, on_text, on_reasoning)?;
            self.dispatch(on_text, on_reasoning)?;
        }
        let response = self
            .completed
            .ok_or_else(|| invalid("stream ended before completion"))?;
        parse_completion(self.provider, response)
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
mod tests {
    use super::*;
    use crate::application::ports::llm_port::CompletionInput;
    use std::sync::Mutex;

    /// What the response replays, as the provider's own JSON.
    fn replayed(response: &CompletionResponse) -> Value {
        Value::Array(
            response
                .replay
                .iter()
                .map(|item| match item {
                    CompletionInput::Native { value } => value.clone(),
                    other => panic!("cloud replay is native, not {other:?}"),
                })
                .collect(),
        )
    }

    fn frames(events: &[Value]) -> Vec<u8> {
        events
            .iter()
            .map(|event| format!("data: {event}\r\n\r\n"))
            .collect::<String>()
            .into_bytes()
    }

    #[test]
    fn openai_stream_keeps_native_tools_and_opaque_state_out_of_displayable_reasoning() {
        let terminal = json!({"status":"completed","usage":{"input_tokens":14,"output_tokens":9},"output":[
            {"type":"reasoning","id":"r1","summary":[{"type":"summary_text","text":"Compare 世界"},{"type":"summary_text","text":"Choose evidence"}],"encrypted_content":"opaque-state"},
            {"type":"message","content":[{"type":"output_text","text":"Hello"}]},
            {"type":"function_call","id":"f1","call_id":"call-1","name":"search","arguments":"{\"q\":\"世界\"}"}
        ]});
        let events = frames(&[
            json!({"type":"response.reasoning_summary_text.delta","item_id":"r1","summary_index":0,"delta":"Compare 世界"}),
            json!({"type":"response.reasoning_text.delta","delta":"private raw reasoning"}),
            json!({"type":"response.reasoning_summary_text.delta","item_id":"r1","summary_index":1,"delta":"Choose evidence"}),
            json!({"type":"response.output_text.delta","delta":"Hello"}),
            json!({"type":"response.completed","response":terminal}),
        ]);
        let text = Mutex::new(String::new());
        let reasoning = Mutex::new(String::new());
        let on_text = |delta: String| {
            text.lock().unwrap().push_str(&delta);
            Ok(())
        };
        let on_reasoning = |delta: String| {
            reasoning.lock().unwrap().push_str(&delta);
            Ok(())
        };
        let mut decoder = Decoder::new(LLMProvider::Openai);
        // Split every UTF-8 character and SSE boundary across network packets.
        for byte in events.chunks(1) {
            decoder.push(byte, &on_text, &on_reasoning).unwrap();
        }
        let response = decoder.finish(&on_text, &on_reasoning).unwrap();
        assert_eq!(*text.lock().unwrap(), "Hello");
        assert_eq!(
            *reasoning.lock().unwrap(),
            "Compare 世界\n\nChoose evidence"
        );
        assert_eq!(
            response.reasoning.as_deref(),
            Some("Compare 世界\n\nChoose evidence")
        );
        assert_eq!(replayed(&response), terminal["output"]);
        assert_eq!((response.input_tokens, response.output_tokens), (14, 9));
        assert!(
            matches!(&response.tool_calls[0], CompletionInput::ToolCall { id, arguments, .. } if id == "call-1" && arguments["q"] == "世界")
        );
    }

    #[test]
    fn anthropic_stream_preserves_signatures_tool_arguments_and_cumulative_usage() {
        let events = frames(&[
            json!({"type":"message_start","message":{"id":"m1","content":[],"usage":{"input_tokens":23,"output_tokens":1},"stop_reason":null}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Compare the sources."}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"signed-"}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"state"}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"content_block_start","index":1,"content_block":{"type":"redacted_thinking","data":"redacted-state"}}),
            json!({"type":"content_block_stop","index":1}),
            json!({"type":"content_block_start","index":2,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":2,"delta":{"type":"text_delta","text":"Answer"}}),
            json!({"type":"content_block_delta","index":2,"delta":{"type":"citations_delta","citation":{"type":"page_location","start_page_number":2}}}),
            json!({"type":"content_block_stop","index":2}),
            json!({"type":"content_block_start","index":3,"content_block":{"type":"tool_use","id":"call-2","name":"search","input":{}}}),
            json!({"type":"content_block_delta","index":3,"delta":{"type":"input_json_delta","partial_json":"{\"query\":\""}}),
            json!({"type":"content_block_delta","index":3,"delta":{"type":"input_json_delta","partial_json":"世界\"}"}}),
            json!({"type":"content_block_stop","index":3}),
            json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":18}}),
            json!({"type":"message_stop"}),
        ]);
        let reasoning = Mutex::new(String::new());
        let on_reasoning = |delta: String| {
            reasoning.lock().unwrap().push_str(&delta);
            Ok(())
        };
        let mut decoder = Decoder::new(LLMProvider::Anthropic);
        for chunk in events.chunks(7) {
            decoder.push(chunk, &|_| Ok(()), &on_reasoning).unwrap();
        }
        let response = decoder.finish(&|_| Ok(()), &on_reasoning).unwrap();
        assert_eq!(*reasoning.lock().unwrap(), "Compare the sources.");
        assert_eq!(response.reasoning.as_deref(), Some("Compare the sources."));
        assert_eq!(response.text, "Answer");
        // One assistant message, its signed thinking included.
        let message = &replayed(&response)[0];
        assert_eq!(message["role"], "assistant");
        assert_eq!(message["content"][0]["signature"], "signed-state");
        assert_eq!(message["content"][1]["data"], "redacted-state");
        assert_eq!(
            message["content"][2]["citations"][0]["start_page_number"],
            2
        );
        assert_eq!((response.input_tokens, response.output_tokens), (23, 18));
        assert_eq!(response.finish_reason, "tool_use");
        assert!(
            matches!(&response.tool_calls[0], CompletionInput::ToolCall { id, arguments, .. } if id == "call-2" && arguments["query"] == "世界")
        );
    }

    #[tokio::test]
    async fn reasoning_is_delivered_before_the_completion_arrives() {
        let (sender, receiver) =
            tokio::sync::mpsc::unbounded_channel::<std::result::Result<Vec<u8>, String>>();
        let (shown, observed) = tokio::sync::oneshot::channel();
        let shown = Mutex::new(Some(shown));
        let on_reasoning = |delta: String| {
            if let Some(shown) = shown.lock().unwrap().take() {
                shown.send(delta).unwrap();
            }
            Ok(())
        };
        let on_text = |_| Ok(());
        sender.send(Ok(frames(&[json!({"type":"response.reasoning_summary_text.delta","item_id":"r1","summary_index":0,"delta":"Checking evidence."})]))).unwrap();
        let completion = drain(
            LLMProvider::Openai,
            tokio_stream::wrappers::UnboundedReceiverStream::new(receiver),
            Duration::from_secs(1),
            &on_text,
            &on_reasoning,
        );
        tokio::pin!(completion);
        tokio::select! {
            result = observed => assert_eq!(result.unwrap(), "Checking evidence."),
            _ = &mut completion => panic!("the response completed before its terminal event"),
        }
        sender
            .send(Ok(frames(&[
                json!({"type":"response.completed","response":{"status":"completed","output":[]}}),
            ])))
            .unwrap();
        completion.await.unwrap();
    }

    #[test]
    fn incomplete_answers_keep_their_status_but_truncated_streams_fail() {
        let mut decoder = Decoder::new(LLMProvider::Openai);
        decoder.push(&frames(&[json!({"type":"response.incomplete","response":{"status":"incomplete","output":[{"type":"message","content":[{"type":"output_text","text":"Partial answer"}]}]}})]), &|_| Ok(()), &|_| Ok(())).unwrap();
        let response = decoder.finish(&|_| Ok(()), &|_| Ok(())).unwrap();
        assert_eq!(response.finish_reason, "incomplete");
        assert_eq!(response.text, "Partial answer");
        let mut decoder = Decoder::new(LLMProvider::Openai);
        decoder
            .push(b"data: [DONE]\n\n", &|_| Ok(()), &|_| Ok(()))
            .unwrap();
        assert!(decoder.finish(&|_| Ok(()), &|_| Ok(())).is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn stalled_streams_and_oversized_events_fail() {
        let pending = futures::stream::pending::<std::result::Result<Vec<u8>, String>>();
        let error = drain(
            LLMProvider::Openai,
            pending,
            Duration::from_secs(2),
            &|_| Ok(()),
            &|_| Ok(()),
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("stalled"));
        let mut decoder = Decoder::new(LLMProvider::Openai);
        assert!(decoder
            .push(
                &vec![b'x'; Decoder::MAX_EVENT_BYTES + 1],
                &|_| Ok(()),
                &|_| Ok(())
            )
            .is_err());
    }
}
