//! Ollama over its native `/api/chat`: messages, streamed answers, tool calls
//! and thinking, behind one typed completion.
//!
//! # Timeout policy
//!
//! Every bound is per request; the shared client deliberately has none.
//! `ClientBuilder::timeout` is a *total* deadline that includes the response
//! body, so a client-wide value silently truncates any generation that takes
//! longer than it — exactly what a long answer does. Probes pass `.timeout(..)`
//! themselves; a completion bounds the wait for response headers and the
//! silence between chunks with `stream_timeout`, and the whole exchange with
//! the request's time budget. Do not reintroduce a client-wide timeout.

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, StatusCode};
use tracing::{debug, warn};

use crate::application::ports::llm_port::{CompletionInput, CompletionRequest, CompletionResponse};
use crate::application::ports::LLMPort;
use crate::features::llm::engine::circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError,
};
use crate::features::llm::engine::types::{
    OllamaChatMessage, OllamaChatRequest, OllamaChatStreamResponse, OllamaTool, OllamaToolCall,
    OllamaToolCallFunction,
};
use crate::features::llm::engine::GenerationConfig;
use crate::shared::error::{AppError, Result};
use crate::shared::http::reqwest_client_builder;

const DEFAULT_OLLAMA_MAX_CONCURRENCY: usize = 3;

/// Requests in flight to Ollama at once; its scheduler admits no more.
pub(crate) fn ollama_max_concurrency() -> usize {
    std::env::var("RECALL_OLLAMA_MAX_CONCURRENCY")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_OLLAMA_MAX_CONCURRENCY)
}

/// A whole stream larger than this is not an answer.
const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

/// Async HTTP client for one model on one Ollama server.
pub struct OllamaClient {
    base_url: String,
    model_name: String,
    client: Client,
    /// Bound on probes such as the health check.
    timeout: Duration,
    /// Longest wait for response headers, and for each chunk once streaming.
    stream_timeout: Duration,
    config: GenerationConfig,
    circuit_breaker: CircuitBreaker,
}

impl OllamaClient {
    /// A client for `model_name` with the default probe and stream timeouts.
    pub fn with_model(base_url: impl Into<String>, model_name: impl Into<String>) -> Result<Self> {
        Self::with_model_and_timeouts(
            base_url,
            model_name,
            Duration::from_secs(120),
            Duration::from_secs(300),
        )
    }

    /// `timeout` bounds probes; `stream_timeout` bounds the wait for headers
    /// and the silence between chunks of an answer.
    pub fn with_model_and_timeouts(
        base_url: impl Into<String>,
        model_name: impl Into<String>,
        timeout: Duration,
        stream_timeout: Duration,
    ) -> Result<Self> {
        Self::with_model_and_timeouts_and_header(
            base_url,
            model_name,
            timeout,
            stream_timeout,
            None,
        )
    }

    /// As [`Self::with_model_and_timeouts`], with an optional `(name, value)`
    /// header sent on every request.
    pub fn with_model_and_timeouts_and_header(
        base_url: impl Into<String>,
        model_name: impl Into<String>,
        timeout: Duration,
        stream_timeout: Duration,
        auth_header: Option<(String, String)>,
    ) -> Result<Self> {
        let mut builder = reqwest_client_builder();
        if let Some((name, value)) = auth_header {
            let header_name = HeaderName::from_bytes(name.trim().as_bytes()).map_err(|e| {
                AppError::InvalidInput(format!("Invalid Ollama header name: {}", e))
            })?;
            let mut header_value = HeaderValue::from_str(value.trim()).map_err(|e| {
                AppError::InvalidInput(format!("Invalid Ollama header value: {}", e))
            })?;
            header_value.set_sensitive(true);
            let mut headers = HeaderMap::new();
            headers.insert(header_name, header_value);
            builder = builder.default_headers(headers);
        }

        let client = builder
            .build()
            .map_err(|e| AppError::Network(format!("Failed to create HTTP client: {}", e)))?;

        let model_name = model_name.into();
        if model_name.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Model name cannot be empty".to_string(),
            ));
        }

        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            model_name,
            client,
            timeout,
            stream_timeout,
            config: GenerationConfig::default(),
            circuit_breaker: CircuitBreaker::new(CircuitBreakerConfig::default()),
        })
    }

    /// The sampling defaults and output ceiling every request starts from.
    pub fn with_generation_config(mut self, config: GenerationConfig) -> Self {
        self.config = config;
        self
    }

    /// Whether an Ollama server answers at this URL.
    ///
    /// Probes `/api/tags`, an Ollama-only route, rather than the bare base
    /// URL: a llama.cpp server answers the base URL with its web UI and would
    /// otherwise pass as Ollama, only to 404 on the first generation call.
    pub async fn health_check(&self) -> bool {
        let url = format!("{}/api/tags", self.base_url);
        debug!("Performing Ollama health check at {}", url);
        match self.client.get(&url).timeout(self.timeout).send().await {
            Ok(response) if response.status() == StatusCode::OK => true,
            Ok(response) => {
                warn!(status = %response.status(), "Ollama health check failed");
                false
            }
            Err(error) => {
                warn!(error = %error, "Ollama health check failed");
                false
            }
        }
    }

    /// The `/api/chat` body for a typed request.
    fn chat_request(&self, request: &CompletionRequest) -> Result<OllamaChatRequest> {
        let mut messages = Vec::with_capacity(request.input.len());
        // Ollama names the tool a result answers rather than the call's id,
        // so each id is resolved to the name of the call it answers.
        let mut call_names = HashMap::new();
        for input in &request.input {
            let message = match input {
                CompletionInput::Message { role, content } => OllamaChatMessage {
                    role: role.clone(),
                    content: content.clone(),
                    thinking: None,
                    images: None,
                    tool_calls: None,
                    tool_name: None,
                },
                CompletionInput::Native { value } => {
                    let message = serde_json::from_value::<OllamaChatMessage>(value.clone())
                        .map_err(|_| {
                            AppError::InvalidInput(
                                "Ollama cannot replay provider-native history from another provider"
                                    .into(),
                            )
                        })?;
                    for (index, call) in message.tool_calls.iter().flatten().enumerate() {
                        call_names.insert(call_id(index), call.function.name.clone());
                    }
                    message
                }
                CompletionInput::ToolCall {
                    id,
                    name,
                    arguments,
                } => {
                    call_names.insert(id.clone(), name.clone());
                    OllamaChatMessage {
                        role: "assistant".into(),
                        content: String::new(),
                        thinking: None,
                        images: None,
                        tool_calls: Some(vec![OllamaToolCall {
                            call_type: "function".into(),
                            function: OllamaToolCallFunction {
                                name: name.clone(),
                                arguments: arguments.clone(),
                                index: None,
                            },
                        }]),
                        tool_name: None,
                    }
                }
                CompletionInput::ToolResult { id, output } => {
                    let tool_name = call_names.get(id).cloned().ok_or_else(|| {
                        AppError::InvalidInput(
                            "Ollama tool result has no matching preceding tool call".into(),
                        )
                    })?;
                    OllamaChatMessage {
                        role: "tool".into(),
                        content: output.clone(),
                        thinking: None,
                        images: None,
                        tool_calls: None,
                        tool_name: Some(tool_name),
                    }
                }
            };
            messages.push(message);
        }
        let tools = (!request.tools.is_empty()).then(|| {
            request
                .tools
                .iter()
                .map(|tool| OllamaTool::new(&tool.name, &tool.description, tool.parameters.clone()))
                .collect()
        });
        let sampling = request.sampling.unwrap_or_default();
        let options = HashMap::from([
            (
                "temperature".to_string(),
                serde_json::json!(sampling.temperature.unwrap_or(self.config.temperature)),
            ),
            (
                "top_p".to_string(),
                serde_json::json!(sampling.top_p.unwrap_or(self.config.top_p)),
            ),
            (
                "top_k".to_string(),
                serde_json::json!(sampling.top_k.unwrap_or(self.config.top_k)),
            ),
            (
                "repeat_penalty".to_string(),
                serde_json::json!(self.config.repeat_penalty),
            ),
            (
                "num_predict".to_string(),
                serde_json::json!(request.effective_max_output_tokens(
                    u32::try_from(self.config.max_tokens).unwrap_or(u32::MAX)
                )),
            ),
        ]);

        let think = match request.reasoning_effort.as_deref() {
            // Capturing a supplied reasoning channel must not force thinking
            // on models that reject it. Keep Ollama's model default unless an
            // effort was explicitly requested.
            None | Some("none") => None,
            Some(effort @ ("low" | "medium" | "high")) => {
                Some(serde_json::Value::String(effort.to_string()))
            }
            Some(_) => {
                return Err(AppError::InvalidInput(
                    "Ollama supports model-default reasoning or low, medium, and high levels"
                        .into(),
                ));
            }
        };

        Ok(OllamaChatRequest {
            model: self.model_name.clone(),
            messages,
            stream: true,
            options: Some(options),
            keep_alive: None,
            tools,
            format: request.json_schema.clone(),
            think,
        })
    }

    /// Stream one chat completion, handing answer text and thinking to the
    /// callbacks as they arrive.
    async fn chat(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_reasoning: &(dyn Fn(String) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        let body = self.chat_request(request)?;
        let exchange = async {
            let response = self
                .circuit_breaker
                .call(async {
                    let send = self
                        .client
                        .post(format!("{}/api/chat", self.base_url))
                        .json(&body)
                        .send();
                    let response = tokio::time::timeout(self.stream_timeout, send)
                        .await
                        .map_err(|_| self.silent())?
                        .map_err(network_error)?;
                    if response.status().is_success() {
                        Ok(response)
                    } else {
                        Err(status_error(response.status()))
                    }
                })
                .await
                .map_err(|error| match error {
                    CircuitBreakerError::Open => AppError::ServiceNotAvailable(
                        "Ollama API unavailable (circuit breaker open)".into(),
                    ),
                    CircuitBreakerError::CallFailed(error) => error,
                })?;

            let mut bytes = response.bytes_stream();
            let mut answer = Answer::default();
            let mut buffer = Vec::new();
            let mut received = 0usize;
            while !answer.done {
                let next = tokio::time::timeout(self.stream_timeout, bytes.next())
                    .await
                    .map_err(|_| self.silent())?;
                let Some(chunk) = next else { break };
                let chunk = chunk.map_err(network_error)?;
                received = received.saturating_add(chunk.len());
                if received > MAX_RESPONSE_BYTES {
                    return Err(AppError::InvalidState(
                        "Ollama response exceeds size limit".into(),
                    ));
                }
                buffer.extend_from_slice(&chunk);
                // Lines are split at the byte level, so a character divided
                // between two network chunks is decoded whole.
                while let Some(newline) = buffer.iter().position(|byte| *byte == b'\n') {
                    let line: Vec<u8> = buffer.drain(..=newline).collect();
                    answer.push_line(&line, on_text, on_reasoning)?;
                }
            }
            if !answer.done {
                // A last line the server did not terminate.
                answer.push_line(&buffer, on_text, on_reasoning)?;
            }
            answer.into_response()
        };
        request.within_time_budget(exchange).await.map_err(|_| {
            AppError::ServiceNotAvailable("Ollama typed completion exceeded its time budget".into())
        })?
    }

    fn silent(&self) -> AppError {
        AppError::ServiceNotAvailable(format!(
            "Ollama sent nothing for {}s",
            self.stream_timeout.as_secs()
        ))
    }
}

/// The synthetic id of the `index`-th call in one reply: Ollama gives calls
/// no ids of their own.
fn call_id(index: usize) -> String {
    format!("ollama-call-{index}")
}

/// An HTTP failure as an error that is safe to show: the status and what it
/// usually means, never the body, which can quote the prompt.
fn status_error(status: StatusCode) -> AppError {
    let description = format!("Ollama chat returned HTTP {status}");
    match status.as_u16() {
        404 => AppError::InvalidConfig(format!(
            "{description}: the model is not installed in Ollama (pull it first) or this is not an Ollama server"
        )),
        408 => AppError::Network(description),
        429 => AppError::RateLimitExceeded(description),
        400..=499 => AppError::InvalidState(description),
        _ => AppError::ServiceNotAvailable(description),
    }
}

fn network_error(error: reqwest::Error) -> AppError {
    if error.is_connect() {
        AppError::Network("Ollama is not reachable; check that it is running".into())
    } else if error.is_timeout() {
        AppError::ServiceNotAvailable("Ollama request timed out".into())
    } else {
        AppError::Network("Ollama request failed; check the server connection".into())
    }
}

/// One streamed reply as it accumulates.
#[derive(Default)]
struct Answer {
    content: String,
    reasoning: String,
    tool_calls: BTreeMap<usize, OllamaToolCall>,
    done: bool,
    done_reason: Option<String>,
    prompt_eval_count: Option<u32>,
    eval_count: Option<u32>,
}

impl Answer {
    fn push_line(
        &mut self,
        line: &[u8],
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_reasoning: &(dyn Fn(String) -> Result<()> + Send + Sync),
    ) -> Result<()> {
        let line = line.trim_ascii();
        let line = line.strip_prefix(b"data:").map_or(line, <[u8]>::trim_ascii);
        if line.is_empty() {
            return Ok(());
        }
        let value: serde_json::Value = serde_json::from_slice(line).map_err(|_| {
            AppError::InvalidState("Ollama returned an invalid typed stream chunk".into())
        })?;
        // A failure after the headers arrives as a line of its own. Its text
        // can quote the prompt, so only the fact of it is reported.
        if value.get("error").is_some() {
            return Err(AppError::ServiceNotAvailable(
                "Ollama reported an error while generating".into(),
            ));
        }
        let chunk = serde_json::from_value::<OllamaChatStreamResponse>(value).map_err(|_| {
            AppError::InvalidState("Ollama returned an invalid typed stream chunk".into())
        })?;
        if !chunk.message.content.is_empty() {
            self.content.push_str(&chunk.message.content);
            on_text(chunk.message.content)?;
        }
        if let Some(thinking) = chunk.message.thinking.filter(|text| !text.is_empty()) {
            self.reasoning.push_str(&thinking);
            on_reasoning(thinking)?;
        }
        for (position, call) in chunk.message.tool_calls.into_iter().flatten().enumerate() {
            self.tool_calls
                .insert(call.function.index.unwrap_or(position), call);
        }
        if chunk.done {
            self.done = true;
            self.done_reason = chunk.done_reason;
            self.prompt_eval_count = chunk.prompt_eval_count;
            self.eval_count = chunk.eval_count;
        }
        Ok(())
    }

    fn into_response(self) -> Result<CompletionResponse> {
        if !self.done {
            return Err(AppError::ServiceNotAvailable(
                "Ollama typed stream ended before completion".into(),
            ));
        }
        let calls: Vec<OllamaToolCall> = self.tool_calls.into_values().collect();
        let tool_calls = calls
            .iter()
            .enumerate()
            .map(|(index, call)| CompletionInput::ToolCall {
                id: call_id(index),
                name: call.function.name.clone(),
                arguments: call.function.arguments.clone(),
            })
            .collect();
        // Replayed as Ollama's own assistant message, so the calls go back in
        // its native shape with the thinking that led to them.
        let assistant = OllamaChatMessage {
            role: "assistant".into(),
            content: self.content.clone(),
            thinking: (!self.reasoning.is_empty()).then(|| self.reasoning.clone()),
            images: None,
            tool_calls: (!calls.is_empty()).then_some(calls),
            tool_name: None,
        };
        Ok(CompletionResponse {
            text: self.content,
            reasoning: (!self.reasoning.is_empty()).then_some(self.reasoning),
            tool_calls,
            input_tokens: u64::from(self.prompt_eval_count.unwrap_or(0)),
            output_tokens: u64::from(self.eval_count.unwrap_or(0)),
            finish_reason: self.done_reason.unwrap_or_else(|| "stop".into()),
            replay: vec![CompletionInput::Native {
                value: serde_json::to_value(assistant).map_err(|error| {
                    AppError::InternalError(format!("Ollama reply is not serialisable: {error}"))
                })?,
            }],
            first_token_logprobs: None,
        })
    }
}

#[async_trait]
impl LLMPort for OllamaClient {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.chat(request, &|_| Ok(()), &|_| Ok(())).await
    }

    async fn complete_with_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        self.chat(request, on_text, &|_| Ok(())).await
    }

    async fn complete_with_reasoning_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_reasoning: &(dyn Fn(String) -> Result<()> + Send + Sync),
        _on_retry: &(dyn Fn(usize) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        self.chat(request, on_text, on_reasoning).await
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    fn max_context_tokens(&self) -> usize {
        // Default Ollama context window (varies by model)
        // llama3.1:8b has 128K context window
        128_000
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(self.health_check().await)
    }

    fn supports_tool_calling(&self) -> bool {
        true
    }

    fn provider_name(&self) -> &str {
        "ollama"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn typed_completion_sends_schema_reasoning_sampling_and_output_budget() {
        use crate::application::ports::llm_port::{
            CompletionInput, CompletionRequest, LLMPort, SamplingOverride,
        };
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };

        let server = MockServer::start().await;
        let schema = serde_json::json!({
            "type": "object",
            "properties": {"ok": {"type": "boolean"}},
            "required": ["ok"],
            "additionalProperties": false
        });
        let expected = serde_json::json!({
            "model":"utility",
            "messages":[
                {"role":"system","content":"Return JSON."},
                {"role":"user","content":"Check this batch."}
            ],
            "stream":true,
            "options":{"temperature":0.0,"top_p":1.0,"top_k":1,"num_predict":2048},
            "format":schema
        });
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(100))
                    .set_body_json(serde_json::json!({
                        "model": "utility",
                        "created_at": "2026-09-28T00:00:00Z",
                        "message": {"role": "assistant", "content": "{\"ok\":true}"},
                        "done": true,
                        "done_reason": "stop",
                        "prompt_eval_count": 30,
                        "eval_count": 4
                    })),
            )
            .mount(&server)
            .await;

        let client = OllamaClient::with_model_and_timeouts(
            server.uri(),
            "utility",
            Duration::from_millis(50),
            Duration::from_secs(1),
        )
        .unwrap();
        let request = CompletionRequest {
            input: vec![
                CompletionInput::Message {
                    role: "system".into(),
                    content: "Return JSON.".into(),
                },
                CompletionInput::Message {
                    role: "user".into(),
                    content: "Check this batch.".into(),
                },
            ],
            json_schema: Some(schema),
            reasoning_effort: Some("none".into()),
            sampling: Some(SamplingOverride::deterministic()),
            max_output_tokens: Some(2048),
            time_budget: Some(Duration::from_secs(2)),
            ..Default::default()
        };
        let response = LLMPort::complete(&client, &request).await.unwrap();
        assert_eq!(response.text, "{\"ok\":true}");
        assert_eq!(response.input_tokens, 30);
        assert_eq!(response.output_tokens, 4);
        assert_eq!(response.finish_reason, "stop");
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests[0].method.as_str(), "POST");
        assert_eq!(requests[0].url.path(), "/api/chat");
        let mut sent: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        let repeat_penalty = sent["options"]["repeat_penalty"]
            .as_f64()
            .expect("repeat penalty is numeric");
        assert!((repeat_penalty - 1.1).abs() < 1e-6);
        sent["options"]
            .as_object_mut()
            .unwrap()
            .remove("repeat_penalty");
        assert_eq!(sent, expected);
    }

    #[tokio::test]
    async fn typed_completion_replays_native_tool_call_and_result_on_next_round() {
        use crate::application::ports::llm_port::{
            CompletionInput, CompletionRequest, LLMPort, ToolDefinition,
        };
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };

        let server = MockServer::start().await;
        let tool = ToolDefinition {
            name: "search_saved_knowledge".into(),
            description: "Search saved notes".into(),
            parameters: serde_json::json!({"type":"object","properties":{}}),
        };
        let first = serde_json::json!({
            "model":"utility",
            "created_at":"2026-09-28T00:00:00Z",
            "message":{"role":"assistant","content":"","tool_calls":[{
                "function":{"name":"search_saved_knowledge","arguments":{"query":"trip"}}
            }]},
            "done":true,
            "done_reason":"stop"
        });
        let second = serde_json::json!({
            "model":"utility",
            "created_at":"2026-09-28T00:00:01Z",
            "message":{"role":"assistant","content":"Friday."},
            "done":true,
            "done_reason":"stop"
        });
        let response_index = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let response_index_for_mock = response_index.clone();
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(move |_request: &wiremock::Request| {
                let response = if response_index_for_mock
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                    == 0
                {
                    first.clone()
                } else {
                    second.clone()
                };
                ResponseTemplate::new(200).set_body_json(response)
            })
            .mount(&server)
            .await;

        let client = OllamaClient::with_model_and_timeouts(
            server.uri(),
            "utility",
            Duration::from_secs(1),
            Duration::from_secs(1),
        )
        .unwrap();
        let request = CompletionRequest {
            input: vec![CompletionInput::Message {
                role: "user".into(),
                content: "When is the trip?".into(),
            }],
            tools: vec![tool],
            max_output_tokens: Some(1024),
            ..Default::default()
        };
        let first_response = LLMPort::complete(&client, &request).await.unwrap();
        let call = first_response.tool_calls.first().unwrap();
        let (id, name) = match call {
            CompletionInput::ToolCall { id, name, .. } => (id.clone(), name.clone()),
            other => panic!("unexpected typed tool call: {other:?}"),
        };
        assert_eq!(name, "search_saved_knowledge");

        let mut next_request = request;
        next_request.input.extend(first_response.replay.clone());
        next_request.input.push(CompletionInput::ToolResult {
            id,
            output: "The trip is on Friday.".into(),
        });
        let second_response = LLMPort::complete(&client, &next_request).await.unwrap();
        assert_eq!(second_response.text, "Friday.");
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2);
        let first_request: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(
            first_request["tools"][0]["function"]["name"],
            "search_saved_knowledge"
        );
        let second_request: serde_json::Value = serde_json::from_slice(&requests[1].body).unwrap();
        let messages = second_request["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(
            messages[1]["tool_calls"][0]["function"]["name"],
            "search_saved_knowledge"
        );
        assert_eq!(messages[2]["role"], "tool");
        assert_eq!(messages[2]["tool_name"], "search_saved_knowledge");
        assert_eq!(messages[2]["content"], "The trip is on Friday.");
    }

    #[tokio::test]
    async fn typed_completion_cancels_http_work_when_request_budget_expires() {
        use crate::application::ports::llm_port::{CompletionInput, CompletionRequest, LLMPort};
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(200))
                    .set_body_json(serde_json::json!({
                        "model":"utility",
                        "created_at":"2026-09-28T00:00:00Z",
                        "message":{"role":"assistant","content":"late"},
                        "done":true
                    })),
            )
            .mount(&server)
            .await;
        let client = OllamaClient::with_model_and_timeouts(
            server.uri(),
            "utility",
            Duration::from_secs(1),
            Duration::from_secs(1),
        )
        .unwrap();
        let request = CompletionRequest {
            input: vec![CompletionInput::Message {
                role: "user".into(),
                content: "Please answer quickly.".into(),
            }],
            time_budget: Some(Duration::from_millis(20)),
            ..Default::default()
        };

        let error = LLMPort::complete(&client, &request).await.unwrap_err();
        assert!(error.to_string().contains("time budget"));
        let request = CompletionRequest {
            no_time_limit: true,
            ..request
        };
        assert_eq!(
            LLMPort::complete(&client, &request).await.unwrap().text,
            "late"
        );
        assert_eq!(
            LLMPort::complete_with_progress(&client, &request, &|_| Ok(()))
                .await
                .unwrap()
                .text,
            "late"
        );
    }

    #[tokio::test]
    async fn typed_completion_streams_content_and_keeps_terminal_tool_calls() {
        use crate::application::ports::llm_port::{
            CompletionInput, CompletionRequest, LLMPort, ToolDefinition,
        };
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };

        let server = MockServer::start().await;
        let chunks = [
            serde_json::json!({
                "model":"utility","created_at":"2026-09-28T00:00:00Z",
                "message":{"role":"assistant","content":"Hi ","thinking":"Compare "},"done":false
            }),
            serde_json::json!({
                "model":"utility","created_at":"2026-09-28T00:00:00Z",
                "message":{"role":"assistant","content":"世界","thinking":"sources","tool_calls":[{
                    "function":{"name":"search_saved_knowledge","arguments":{"query":"trip"},"index":0}
                }]},"done":true,"done_reason":"stop","prompt_eval_count":20,"eval_count":5
            }),
        ];
        let body = chunks
            .iter()
            .map(serde_json::Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_string(format!("{body}\n")))
            .mount(&server)
            .await;
        let client = OllamaClient::with_model_and_timeouts(
            server.uri(),
            "utility",
            Duration::from_secs(1),
            Duration::from_secs(1),
        )
        .unwrap();
        let request = CompletionRequest {
            input: vec![CompletionInput::Message {
                role: "user".into(),
                content: "When is the trip?".into(),
            }],
            include_reasoning: true,
            tools: vec![ToolDefinition {
                name: "search_saved_knowledge".into(),
                description: "Search saved notes".into(),
                parameters: serde_json::json!({"type":"object","properties":{}}),
            }],
            ..Default::default()
        };
        let received = Arc::new(std::sync::Mutex::new(Vec::new()));
        let received_by_callback = received.clone();
        let on_text = move |text: String| {
            received_by_callback.lock().unwrap().push(text);
            Ok(())
        };
        let received_reasoning = Arc::new(std::sync::Mutex::new(Vec::new()));
        let reasoning_by_callback = received_reasoning.clone();
        let on_reasoning = move |text: String| {
            reasoning_by_callback.lock().unwrap().push(text);
            Ok(())
        };
        let response = LLMPort::complete_with_reasoning_progress(
            &client,
            &request,
            &on_text,
            &on_reasoning,
            &|_| Ok(()),
        )
        .await
        .unwrap();

        assert_eq!(response.text, "Hi 世界");
        assert_eq!(response.reasoning.as_deref(), Some("Compare sources"));
        assert_eq!(*received.lock().unwrap(), vec!["Hi ", "世界"]);
        assert_eq!(
            *received_reasoning.lock().unwrap(),
            vec!["Compare ", "sources"]
        );
        assert_eq!(response.tool_calls.len(), 1);
        assert!(matches!(
            response.tool_calls.first(),
            Some(CompletionInput::ToolCall { name, .. }) if name == "search_saved_knowledge"
        ));
        assert_eq!(response.output_tokens, 5);
        assert_eq!(response.finish_reason, "stop");
        let sent: serde_json::Value =
            serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
        assert_eq!(sent["stream"], true);
        assert!(sent.get("think").is_none());
        assert_eq!(
            sent["tools"][0]["function"]["name"],
            "search_saved_knowledge"
        );
    }

    #[tokio::test]
    async fn typed_stream_handles_sparse_tool_indices_without_allocating_gaps() {
        use crate::application::ports::llm_port::{CompletionInput, CompletionRequest, LLMPort};
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };

        // Exercise both newline-delimited chunks and the final unterminated line.
        for suffix in ["\n", ""] {
            let server = MockServer::start().await;
            let body = serde_json::json!({
                "model": "utility", "created_at": "2026-09-30T00:00:00Z",
                "message": {"role": "assistant", "content": "", "tool_calls": [
                    {"function": {"name": "later", "arguments": {}, "index": usize::MAX}},
                    {"function": {"name": "replaced", "arguments": {}, "index": 0}},
                    {"function": {"name": "first", "arguments": {}, "index": 0}}
                ]}, "done": true
            });
            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_string(format!("  {body}  {suffix}")),
                )
                .mount(&server)
                .await;
            let client = OllamaClient::with_model_and_timeouts(
                server.uri(),
                "utility",
                Duration::from_secs(1),
                Duration::from_secs(1),
            )
            .unwrap();
            let response = LLMPort::complete_with_progress(
                &client,
                &CompletionRequest::default(),
                &|_| Ok(()),
            )
            .await
            .unwrap();
            let names: Vec<_> = response
                .tool_calls
                .iter()
                .filter_map(|call| match call {
                    CompletionInput::ToolCall { name, .. } => Some(name.as_str()),
                    _ => None,
                })
                .collect();
            assert_eq!(names, ["first", "later"]);
        }
    }

    #[test]
    fn the_base_url_loses_its_trailing_slash() {
        let client = OllamaClient::with_model("http://localhost:11434/", "mistral").unwrap();
        assert_eq!(client.base_url, "http://localhost:11434");
        assert_eq!(client.model_name(), "mistral");
        assert!(OllamaClient::with_model("http://localhost:11434", " ").is_err());
    }

    #[tokio::test]
    async fn health_check_requires_an_ollama_route_not_just_a_listening_port() {
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };

        // A llama.cpp server answers "/" with its web UI but has no /api/tags.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_string("<html>llama.cpp</html>"))
            .mount(&server)
            .await;
        let client = OllamaClient::with_model(server.uri(), "any-model").unwrap();
        assert!(!client.health_check().await);

        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"models":[]})),
            )
            .mount(&server)
            .await;
        assert!(client.health_check().await);
    }

    #[tokio::test]
    async fn a_failed_status_says_what_it_means_without_echoing_the_body() {
        use crate::application::ports::llm_port::CompletionRequest;
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(404).set_body_json(
                serde_json::json!({"error":"model 'private prompt text' not found"}),
            ))
            .mount(&server)
            .await;
        let client = OllamaClient::with_model(server.uri(), "absent").unwrap();
        let error = client
            .complete(&CompletionRequest::default())
            .await
            .unwrap_err();
        assert!(matches!(error, AppError::InvalidConfig(_)), "{error:?}");
        assert!(error.to_string().contains("pull it first"), "{error}");
        assert!(!error.to_string().contains("private prompt text"));
    }

    /// Scripted chunks written to a raw socket, each after its delay.
    async fn scripted_server(
        chunks: Vec<(Duration, Vec<u8>)>,
    ) -> (String, tokio::task::JoinHandle<()>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            // Drain the request headers; the body is irrelevant to these tests.
            let mut buffer = [0u8; 8192];
            let read = socket.read(&mut buffer).await.unwrap();
            assert!(read > 0, "client sent no request");
            let size: usize = chunks.iter().map(|(_, chunk)| chunk.len()).sum();
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: {size}\r\n\r\n").as_bytes()).await.unwrap();
            for (delay, chunk) in chunks {
                tokio::time::sleep(delay).await;
                if socket.write_all(&chunk).await.is_err() {
                    return;
                }
            }
        });
        (format!("http://{address}"), server)
    }

    /// A client-wide `timeout` covers the response body, so it would truncate a
    /// generation that is still producing bytes. A character split between two
    /// network chunks must also come out whole.
    #[tokio::test]
    async fn a_streamed_body_outlives_the_probe_timeout_and_keeps_split_characters() {
        let first = "{\"model\":\"m\",\"created_at\":\"\",\"message\":{\"role\":\"assistant\",\"content\":\"Slow 世\"},\"done\":false}\n{\"model\":\"m\",\"created_at\":\"\",\"message\":{\"role\":\"assistant\",\"content\":\"界\"},\"done\":true}\n";
        let split = first.find('界').unwrap() + 1;
        let bytes = first.as_bytes();
        let delay = Duration::from_millis(250);
        let (url, server) = scripted_server(vec![
            (delay, bytes[..split].to_vec()),
            (delay, bytes[split..].to_vec()),
        ])
        .await;

        let client = OllamaClient::with_model_and_timeouts(
            url,
            "any-model",
            Duration::from_millis(150),
            Duration::from_secs(5),
        )
        .unwrap();
        let response = client
            .complete(&crate::application::ports::llm_port::CompletionRequest::default())
            .await
            .unwrap();
        assert_eq!(response.text, "Slow 世界");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn a_stream_that_goes_silent_is_a_stall_not_a_wait() {
        let chunk = b"{\"model\":\"m\",\"created_at\":\"\",\"message\":{\"role\":\"assistant\",\"content\":\"Half\"},\"done\":false}\n".to_vec();
        let (url, server) = scripted_server(vec![
            (Duration::ZERO, chunk),
            (Duration::from_secs(5), b"never".to_vec()),
        ])
        .await;
        let client = OllamaClient::with_model_and_timeouts(
            url,
            "any-model",
            Duration::from_secs(5),
            Duration::from_millis(200),
        )
        .unwrap();
        let error = client
            .complete(&crate::application::ports::llm_port::CompletionRequest::default())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("sent nothing"), "{error}");
        server.abort();
    }
}
