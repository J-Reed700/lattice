//! Native OpenAI Responses and Anthropic Messages adapters.
//! Explicit selection only; Auto never sends local documents to a cloud provider.
use crate::application::{
    contracts::settings::{LLMProvider, LLMSettingsDto},
    ports::llm_port::{CompletionInput, CompletionRequest, CompletionResponse, LLMPort},
};
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

pub struct CloudLlm {
    provider: LLMProvider,
    model: String,
    key: String,
    endpoint: String,
    client: reqwest::Client,
    /// Longest silence tolerated inside a streamed response.
    stall_timeout: Duration,
    max_tokens: u32,
    context_window: usize,
    system_prompt: String,
}

impl CloudLlm {
    pub fn new(settings: &LLMSettingsDto, key: String) -> Result<Self> {
        if key.trim().is_empty() || settings.model.trim().is_empty() {
            return Err(AppError::InvalidConfig(
                "Cloud model and API key are required".into(),
            ));
        }
        let endpoint = match settings.provider {
            LLMProvider::Openai => "https://api.openai.com/v1/responses",
            LLMProvider::Anthropic => "https://api.anthropic.com/v1/messages",
            _ => {
                return Err(AppError::InvalidConfig(
                    "Expected an explicit cloud provider".into(),
                ))
            }
        };
        // No client-wide timeout: a long answer is bounded by the request's time
        // budget, and a streamed one additionally by stall detection.
        let client = crate::shared::utils::reqwest_client_builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| AppError::InvalidConfig(e.to_string()))?;
        Ok(Self {
            provider: settings.provider,
            model: settings.model.clone(),
            key,
            endpoint: endpoint.into(),
            client,
            stall_timeout: Duration::from_secs(u64::from(settings.timeout_seconds.max(1))),
            max_tokens: settings.max_tokens,
            context_window: settings.context_window as usize,
            system_prompt: settings.prompts.system_prompt.clone(),
        })
    }

    fn body(&self, request: &CompletionRequest, stream: bool) -> Result<Value> {
        if self.provider == LLMProvider::Openai {
            let input: Vec<Value> = request.input.iter().map(|item| match item {
                CompletionInput::Native { value } => value.clone(),
                CompletionInput::Message { role, content } => json!({"role":role,"content":content}),
                CompletionInput::ToolCall { id, name, arguments } => json!({"type":"function_call","call_id":id,"name":name,"arguments":arguments.to_string()}),
                CompletionInput::ToolResult { id, output } => json!({"type":"function_call_output","call_id":id,"output":output}),
            }).collect();
            let mut body = json!({"model":self.model,"input":input,"stream":stream,"store":false,"include":["reasoning.encrypted_content"],"max_output_tokens":self.max_tokens});
            if !request.tools.is_empty() {
                set_field(&mut body, "tools", json!(request.tools.iter().map(|t| json!({"type":"function","name":t.name,"description":t.description,"parameters":t.parameters,"strict":false})).collect::<Vec<_>>()))?;
            }
            if let Some(schema) = &request.json_schema {
                set_field(
                    &mut body,
                    "text",
                    json!({"format":{"type":"json_schema","name":"response","schema":schema,"strict":true}}),
                )?;
            }
            if let Some(effort) =
                openai_reasoning_effort(&self.model, request.reasoning_effort.as_deref())
            {
                set_field(&mut body, "reasoning", json!({"effort":effort}))?;
            }
            Ok(body)
        } else {
            let mut system = Vec::new();
            let mut messages = Vec::new();
            for item in &request.input {
                match item {
                    CompletionInput::Native { value } => messages.push(value.clone()),
                    CompletionInput::Message { role, content } if role == "system" || role == "developer" => system.push(content.clone()),
                    CompletionInput::Message { role, content } => messages.push(json!({"role":role,"content":content})),
                    CompletionInput::ToolCall { id, name, arguments } => messages.push(json!({"role":"assistant","content":[{"type":"tool_use","id":id,"name":name,"input":arguments}]})),
                    CompletionInput::ToolResult { id, output } => {
                        let result = json!({"type":"tool_result","tool_use_id":id,"content":output});
                        if let Some(last) = messages.last_mut().filter(|m| field(m, "role") == "user" && field(m, "content").is_array()) {
                            if let Some(content) = last.get_mut("content").and_then(Value::as_array_mut) { content.push(result); }
                        } else { messages.push(json!({"role":"user","content":[result]})); }
                    },
                }
            }
            let mut body = json!({"model":self.model,"messages":messages,"max_tokens":self.max_tokens,"stream":stream});
            if !system.is_empty() {
                set_field(&mut body, "system", json!(system.join("\n\n")))?;
            }
            if !request.tools.is_empty() {
                set_field(&mut body, "tools", json!(request.tools.iter().map(|t| json!({"name":t.name,"description":t.description,"input_schema":t.parameters})).collect::<Vec<_>>()))?;
            }
            if let Some(effort) = &request.reasoning_effort {
                // Extended thinking is off unless a `thinking` block asks for it,
                // so "none" is expressed by leaving that block out — requesting
                // adaptive thinking turned "do not think" into its opposite.
                // Anthropic has no "none" effort of its own, so the answer itself
                // is held at the lowest setting instead.
                if effort == "none" {
                    set_field(&mut body, "output_config", json!({"effort":"low"}))?;
                } else {
                    set_field(&mut body, "thinking", json!({"type":"adaptive"}))?;
                    set_field(&mut body, "output_config", json!({"effort":effort}))?;
                }
            }
            if let Some(schema) = &request.json_schema {
                let mut config = field(&body, "output_config").clone();
                if config.is_null() {
                    config = json!({});
                }
                set_field(
                    &mut config,
                    "format",
                    json!({"type":"json_schema","schema":schema}),
                )?;
                set_field(&mut body, "output_config", config)?;
            }
            Ok(body)
        }
    }

    /// All three attempts and their backoffs share one `deadline`, so a retry
    /// never multiplies the caller's time budget. `bound_body` applies what is
    /// left of it to the whole exchange; without it only the wait for response
    /// headers is bounded and the streamed body is left to stall detection.
    async fn send(
        &self,
        body: &Value,
        deadline: Instant,
        bound_body: bool,
    ) -> Result<reqwest::Response> {
        let mut last_error = None;
        for attempt in 0..3u32 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(last_error.unwrap_or_else(|| self.budget_error()));
            }
            let request = self.client.post(&self.endpoint).json(body);
            let request = if bound_body {
                request.timeout(remaining)
            } else {
                request
            };
            let request = if self.provider == LLMProvider::Openai {
                request.bearer_auth(&self.key)
            } else {
                request
                    .header("x-api-key", &self.key)
                    .header("anthropic-version", "2023-06-01")
            };
            let send = request.send();
            let response = if bound_body {
                send.await
            } else {
                tokio::time::timeout(remaining, send)
                    .await
                    .map_err(|_| self.budget_error())?
            }
            .map_err(|e| {
                if e.is_timeout() {
                    self.budget_error()
                } else {
                    AppError::Network(format!("Cloud request failed: {e}"))
                }
            })?;
            let status = response.status();
            if status.is_success() {
                return Ok(response);
            }
            // Avoid echoing provider bodies which can contain submitted user text.
            let error = AppError::Network(format!(
                "{} API returned HTTP {}",
                self.provider_name(),
                status
            ));
            if attempt == 2 || !(status.as_u16() == 429 || status.is_server_error()) {
                return Err(error);
            }
            let delay = Duration::from_secs(
                response
                    .headers()
                    .get("retry-after")
                    .and_then(|h| h.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(1 << attempt)
                    .min(30),
            );
            // Sleeping past the deadline would report a budget expiry instead of
            // the provider's own reason for refusing.
            if deadline.saturating_duration_since(Instant::now()) <= delay {
                return Err(error);
            }
            last_error = Some(error);
            tokio::time::sleep(delay).await;
        }
        Err(AppError::Network("Cloud request retries exhausted".into()))
    }

    /// Exhausting the budget is not a stall, and must not be reported as one.
    fn budget_error(&self) -> AppError {
        AppError::ServiceNotAvailable(format!(
            "{} did not respond within the request's time budget",
            self.provider_name()
        ))
    }

    fn legacy_request(
        &self,
        prompt: &str,
        context: &[String],
        images: Option<Vec<String>>,
    ) -> Result<CompletionRequest> {
        if images.is_some_and(|i| !i.is_empty()) {
            return Err(AppError::InvalidInput(
                "Cloud image input is not yet supported by this chat interface".into(),
            ));
        }
        // Existing callers pass formatted history/context; preserve it as user data.
        Ok(CompletionRequest {
            input: vec![
                CompletionInput::Message {
                    role: "system".into(),
                    content: self.system_prompt.clone(),
                },
                CompletionInput::Message {
                    role: "user".into(),
                    content: format!("{}\n\n{}", context.join("\n\n"), prompt),
                },
            ],
            ..Default::default()
        })
    }
}

#[async_trait]
impl LLMPort for CloudLlm {
    fn supports_typed_completions(&self) -> bool {
        true
    }
    fn supports_tool_calling(&self) -> bool {
        true
    }
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let deadline = Instant::now() + request.effective_time_budget();
        let response = self
            .send(&self.body(request, false)?, deadline, true)
            .await?;
        let value: Value = response
            .json()
            .await
            .map_err(|e| AppError::Network(format!("Invalid cloud response: {e}")))?;
        parse_completion(self.provider, value)
    }
    async fn generate(
        &self,
        prompt: &str,
        context: &[String],
        images: Option<Vec<String>>,
    ) -> Result<String> {
        let response = self
            .complete(&self.legacy_request(prompt, context, images)?)
            .await?;
        if !response.tool_calls.is_empty() {
            return Err(AppError::InvalidState(
                "Unexpected tool call in text generation".into(),
            ));
        }
        if matches!(
            response.finish_reason.as_str(),
            "incomplete" | "max_tokens" | "failed"
        ) {
            return Err(AppError::InvalidState(format!(
                "Cloud response stopped: {}",
                response.finish_reason
            )));
        }
        Ok(response.text)
    }
    async fn generate_streaming(
        &self,
        prompt: &str,
        context: &[String],
        images: Option<Vec<String>>,
    ) -> Result<Box<dyn Stream<Item = Result<String>> + Send + Unpin + '_>> {
        let request = self.legacy_request(prompt, context, images)?;
        // The streamed body is bounded by stall detection below, but waiting for
        // the status line is bounded by nothing else.
        let deadline = Instant::now() + request.effective_time_budget();
        let response = self
            .send(&self.body(&request, true)?, deadline, false)
            .await?;
        let mut bytes = response.bytes_stream();
        let provider = self.provider;
        let stall_timeout = self.stall_timeout;
        let output = async_stream::try_stream! {
            let mut buffer = Vec::new();
            let mut completed = false;
            loop {
                let next = tokio::time::timeout(stall_timeout, bytes.next()).await
                    .map_err(|_| AppError::Network(format!("Cloud stream stalled for {}s", stall_timeout.as_secs())))?;
                let Some(chunk) = next else { break };
                buffer.extend_from_slice(&chunk.map_err(|e| AppError::Network(format!("Cloud stream interrupted: {e}")))?);
                if buffer.len() > 4 * 1024 * 1024 { Err(AppError::Network("Cloud event exceeds size limit".into()))?; }
                // Split complete lines at the byte level; UTF-8 characters may span network packets.
                while let Some(end) = buffer.iter().position(|b| *b == b'\n') {
                    let line: Vec<u8> = buffer.drain(..=end).collect();
                    let line = std::str::from_utf8(&line).map_err(|e| AppError::Network(e.to_string()))?;
                    let Some(data) = line.trim_end().strip_prefix("data:") else { continue; };
                    let data = data.trim_start();
                    if data == "[DONE]" { continue; }
                    let event: Value = serde_json::from_str(data).map_err(|e| AppError::Network(format!("Invalid cloud event: {e}")))?;
                    let kind = field(&event, "type").as_str().unwrap_or("");
                    match kind {
                        "response.output_text.delta" | "response.refusal.delta" => if let Some(text) = field(&event, "delta").as_str() { yield text.to_owned(); },
                        "content_block_delta" if provider == LLMProvider::Anthropic => if let Some(text) = field(field(&event, "delta"), "text").as_str() { yield text.to_owned(); },
                        "response.completed" | "message_stop" => completed = true,
                        "message_delta" if field(field(&event, "delta"), "stop_reason") == "max_tokens" => Err(AppError::InvalidState("Cloud response reached its output limit".into()))?,
                        "error" | "response.failed" | "response.incomplete" => Err(AppError::Network(format!("Cloud stream failed ({kind})")))?,
                        _ => {}
                    }
                }
            }
            if !completed { Err(AppError::Network("Cloud stream ended before completion".into()))?; }
        };
        Ok(Box::new(Box::pin(output)))
    }
    async fn generate_streaming_with_tools(
        &self,
        prompt: &str,
        context: &[String],
        images: Option<Vec<String>>,
        tools: Option<&[crate::application::ports::ToolDefinition]>,
    ) -> Result<
        Box<dyn Stream<Item = Result<crate::application::ports::StreamChunk>> + Send + Unpin + '_>,
    > {
        use crate::application::ports::{StreamChunk, ToolCall};
        if tools.is_none_or(|items| items.is_empty()) {
            let stream = self.generate_streaming(prompt, context, images).await?;
            return Ok(Box::new(stream.map(|item| item.map(StreamChunk::Content))));
        }
        let mut request = self.legacy_request(prompt, context, images)?;
        request.tools = tools.unwrap_or(&[]).to_vec();
        let response = self.complete(&request).await?;
        if matches!(
            response.finish_reason.as_str(),
            "incomplete" | "max_tokens" | "failed"
        ) {
            return Err(AppError::InvalidState(format!(
                "Cloud response stopped: {}",
                response.finish_reason
            )));
        }
        let calls = response
            .tool_calls
            .into_iter()
            .filter_map(|input| match input {
                CompletionInput::ToolCall {
                    id,
                    name,
                    arguments,
                } => Some(ToolCall {
                    id: Some(id),
                    name,
                    arguments,
                }),
                _ => None,
            })
            .collect();
        Ok(Box::new(futures::stream::iter(vec![
            Ok(StreamChunk::Content(response.text)),
            Ok(StreamChunk::ToolCalls(calls)),
            Ok(StreamChunk::Done),
        ])))
    }

    fn model_name(&self) -> &str {
        &self.model
    }
    fn provider_name(&self) -> &str {
        if self.provider == LLMProvider::Openai {
            "openai"
        } else {
            "anthropic"
        }
    }
    fn max_context_tokens(&self) -> usize {
        self.context_window
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len().div_ceil(4)
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(!self.key.is_empty())
    }
}

fn parse_completion(provider: LLMProvider, value: Value) -> Result<CompletionResponse> {
    let mut response = CompletionResponse::default();
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
                    serde_json::from_str(field(block, "arguments").as_str().unwrap_or(""))
                        .map_err(|e| AppError::Network(format!("Invalid tool arguments: {e}")))?
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
    response.input_tokens = field(field(&value, "usage"), "input_tokens")
        .as_u64()
        .unwrap_or(0);
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
    response.provider_output = field(&value, key).clone();
    Ok(response)
}

fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap_or(&Value::Null)
}

fn set_field(value: &mut Value, key: &str, field_value: Value) -> Result<()> {
    value
        .as_object_mut()
        .ok_or_else(|| AppError::InvalidState("Expected a JSON object".into()))?
        .insert(key.into(), field_value);
    Ok(())
}

/// What, if anything, belongs in the Responses API's `reasoning` block.
///
/// The field is not universally accepted: a model without reasoning controls
/// rejects the whole request with a 400, and the retrieval callers swallow that
/// with `.ok()`, so every turn would quietly lose its query rewrites and plans
/// with nothing in the log to say why. Only models that have the control are
/// sent one.
///
/// There is also no "off" switch: the floor is `minimal` on the gpt-5 family and
/// `low` on the o-series, so a request for no reasoning is clamped to whichever
/// the target model understands. A non-reasoning model needs no clamp — it never
/// thinks in the first place.
fn openai_reasoning_effort<'a>(model: &str, effort: Option<&'a str>) -> Option<&'a str> {
    let effort = effort?;
    let floor = if model.starts_with("gpt-5") {
        "minimal"
    } else if model.starts_with('o') {
        "low"
    } else {
        return None;
    };
    Some(if effort == "none" { floor } else { effort })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_tool_calls_keep_ids_and_usage() {
        let result = parse_completion(LLMProvider::Openai, json!({"status":"completed","output":[{"type":"function_call","call_id":"call_123","name":"search","arguments":"{\"q\":\"rust\"}"}],"usage":{"input_tokens":12,"output_tokens":3}})).unwrap();
        assert!(
            matches!(&result.tool_calls[0], CompletionInput::ToolCall { id, .. } if id == "call_123")
        );
        assert_eq!(result.input_tokens, 12);
    }
    #[test]
    fn provider_wire_formats_preserve_tool_results() {
        let mut settings = LLMSettingsDto::default();
        let request = CompletionRequest {
            input: vec![CompletionInput::ToolResult {
                id: "abc".into(),
                output: "found".into(),
            }],
            ..Default::default()
        };
        settings.provider = LLMProvider::Openai;
        let openai = CloudLlm::new(&settings, "test".into()).unwrap();
        assert_eq!(
            openai.body(&request, false).unwrap()["input"][0]["call_id"],
            "abc"
        );
        settings.provider = LLMProvider::Anthropic;
        let anthropic = CloudLlm::new(&settings, "test".into()).unwrap();
        assert_eq!(
            anthropic.body(&request, false).unwrap()["messages"][0]["content"][0]["tool_use_id"],
            "abc"
        );
    }

    fn anthropic_body(effort: &str) -> Value {
        let settings = LLMSettingsDto {
            provider: LLMProvider::Anthropic,
            ..Default::default()
        };
        let anthropic = CloudLlm::new(&settings, "test".into()).unwrap();
        anthropic
            .body(
                &CompletionRequest {
                    reasoning_effort: Some(effort.into()),
                    ..Default::default()
                },
                false,
            )
            .unwrap()
    }

    /// A `thinking` block is what switches extended thinking on, so asking for
    /// no reasoning must not send one.
    #[test]
    fn anthropic_omits_the_thinking_block_when_no_reasoning_is_asked_for() {
        let body = anthropic_body("none");
        assert!(body.get("thinking").is_none(), "{body}");
        assert_eq!(body["output_config"]["effort"], "low");
    }

    #[test]
    fn anthropic_still_enables_thinking_for_a_real_effort() {
        let body = anthropic_body("medium");
        assert_eq!(body["thinking"]["type"], "adaptive");
        assert_eq!(body["output_config"]["effort"], "medium");
    }

    fn openai_body(model: &str, effort: Option<&str>) -> Value {
        let settings = LLMSettingsDto {
            provider: LLMProvider::Openai,
            model: model.into(),
            ..Default::default()
        };
        let openai = CloudLlm::new(&settings, "test".into()).unwrap();
        openai
            .body(
                &CompletionRequest {
                    reasoning_effort: effort.map(str::to_owned),
                    ..Default::default()
                },
                false,
            )
            .unwrap()
    }

    /// A `reasoning` block on a model without reasoning controls is a 400, and
    /// the retrieval callers discard errors — so the field must never be sent
    /// speculatively.
    #[test]
    fn openai_sends_no_reasoning_block_to_a_model_that_has_no_reasoning() {
        let body = openai_body("gpt-4o-mini", Some("none"));
        assert!(body.get("reasoning").is_none(), "{body}");
    }

    #[test]
    fn openai_clamps_no_reasoning_to_each_family_floor() {
        assert_eq!(
            openai_body("gpt-5.1", Some("none"))["reasoning"]["effort"],
            "minimal"
        );
        assert_eq!(
            openai_body("o3", Some("none"))["reasoning"]["effort"],
            "low"
        );
    }

    #[test]
    fn openai_forwards_a_real_effort_unchanged() {
        assert_eq!(
            openai_body("gpt-5.1", Some("high"))["reasoning"]["effort"],
            "high"
        );
        assert!(openai_body("gpt-5.1", None).get("reasoning").is_none());
    }
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    use wiremock::{
        matchers::{header, method, path},
        Mock, MockServer, ResponseTemplate,
    };
    async fn client(server: &MockServer) -> CloudLlm {
        let settings = LLMSettingsDto {
            provider: LLMProvider::Openai,
            model: "test-model".into(),
            ..Default::default()
        };
        let mut client = CloudLlm::new(&settings, "test-secret".into()).unwrap();
        client.endpoint = format!("{}/responses", server.uri());
        client
    }
    #[tokio::test]
    async fn streams_text_and_requires_terminal_event() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/responses")).and(header("authorization", "Bearer test-secret"))
            .respond_with(ResponseTemplate::new(200).set_body_string("data: {\"type\":\"response.output_text.delta\",\"delta\":\"héllo\"}\n\ndata: {\"type\":\"response.completed\"}\n\n"))
            .mount(&server).await;
        let client = client(&server).await;
        let mut stream = client.generate_streaming("hello", &[], None).await.unwrap();
        assert_eq!(stream.next().await.unwrap().unwrap(), "héllo");
        assert!(stream.next().await.is_none());
        server.reset().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\n",
            ))
            .mount(&server)
            .await;
        let mut stream = client.generate_streaming("hello", &[], None).await.unwrap();
        assert_eq!(stream.next().await.unwrap().unwrap(), "partial");
        assert!(stream.next().await.unwrap().is_err());
    }
    #[tokio::test]
    async fn streaming_tool_interface_preserves_native_call_ids() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status":"completed", "output":[{"type":"function_call", "call_id":"call_7", "name":"search", "arguments":"{}"}], "usage":{"input_tokens":4,"output_tokens":2}
            }))).mount(&server).await;
        let client = client(&server).await;
        let tools = vec![crate::application::ports::ToolDefinition {
            name: "search".into(),
            description: "Search documents".into(),
            parameters: json!({"type":"object","properties":{}}),
        }];
        let mut stream = client
            .generate_streaming_with_tools("find", &[], None, Some(&tools))
            .await
            .unwrap();
        let mut observed = false;
        while let Some(chunk) = stream.next().await {
            if let crate::application::ports::StreamChunk::ToolCalls(calls) = chunk.unwrap() {
                assert_eq!(calls.first().unwrap().id.as_deref(), Some("call_7"));
                observed = true;
            }
        }
        assert!(observed);
        let requests = server.received_requests().await.unwrap();
        let body: Value = serde_json::from_slice(&requests.first().unwrap().body).unwrap();
        assert_eq!(
            body.pointer("/input/0/role").and_then(Value::as_str),
            Some("system")
        );
        assert_eq!(
            body.pointer("/input/0/content").and_then(Value::as_str),
            Some(client.system_prompt.as_str())
        );
    }

    #[tokio::test]
    async fn authentication_failure_is_not_retried_or_echoed() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401).set_body_string("sensitive-provider-body"))
            .expect(1)
            .mount(&server)
            .await;
        let client = client(&server).await;
        let error = client.generate("hello", &[], None).await.unwrap_err();
        assert!(error.to_string().contains("401"));
        assert!(!error.to_string().contains("sensitive-provider-body"));
    }
    #[tokio::test]
    async fn cancellation_by_dropping_the_stream_cancels_the_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("data: {\"type\":\"response.completed\"}\n\n"),
            )
            .expect(1)
            .mount(&server)
            .await;
        let client = client(&server).await;
        let stream = client.generate_streaming("hello", &[], None).await.unwrap();
        drop(stream);
        // No detached generation or retry task survives a dropped stream.
        server.verify().await;
    }
}
