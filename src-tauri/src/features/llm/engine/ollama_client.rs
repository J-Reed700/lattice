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

use crate::features::llm::engine::circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError,
};
use crate::features::llm::engine::types::{
    OllamaChatMessage, OllamaChatRequest, OllamaChatStreamResponse, OllamaTool, OllamaToolCall,
    OllamaToolCallFunction,
};
use crate::features::llm::engine::GenerationConfig;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse, ThinkingMode,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::http::reqwest_client_builder;

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
        self.reach().await.is_ok()
    }

    /// [`Self::health_check`] with the reason it failed: a server that cannot
    /// be connected to, refused or unreachable, is [`AppError::Network`], an
    /// outage; one that answers without the Ollama API is not.
    pub async fn reach(&self) -> Result<()> {
        let url = format!("{}/api/tags", self.base_url);
        debug!("Performing Ollama health check at {}", url);
        let response = self
            .client
            .get(&url)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|error| {
                warn!(error = %error, "Ollama health check failed");
                network_error(error)
            })?;
        let status = response.status();
        if status == StatusCode::OK {
            return Ok(());
        }
        warn!(%status, "Ollama health check failed");
        Err(AppError::ServiceNotAvailable(format!(
            "the server answered the Ollama model list with HTTP {status}"
        )))
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
        // Thinking is a template switch on Ollama as on llama.cpp, decided the
        // same way: the user's pin, or the request with tool rounds held off.
        let thinking = self.config.thinking_override.mode_for(request);
        let profile = self.config.profile(thinking);
        let options = HashMap::from([
            (
                "temperature".to_string(),
                serde_json::json!(sampling.temperature.unwrap_or(profile.temperature)),
            ),
            (
                "top_p".to_string(),
                serde_json::json!(sampling.top_p.unwrap_or(profile.top_p)),
            ),
            (
                "top_k".to_string(),
                serde_json::json!(sampling.top_k.unwrap_or(profile.top_k)),
            ),
            (
                "presence_penalty".to_string(),
                serde_json::json!(sampling
                    .presence_penalty
                    .unwrap_or(profile.presence_penalty)),
            ),
            (
                "repeat_penalty".to_string(),
                serde_json::json!(profile.repeat_penalty),
            ),
            (
                "num_predict".to_string(),
                serde_json::json!(request.effective_max_output_tokens(
                    u32::try_from(self.config.max_tokens).unwrap_or(u32::MAX)
                )),
            ),
        ]);

        // `think: false` is sent every time thinking is off: Ollama rejects
        // only a truthy `think` on a model without thinking, and leaving it out
        // let Qwen 3.5 think by its template's default. Turned on, a level
        // goes through for models that grade it (gpt-oss) and `true` otherwise.
        let think = match (thinking, request.reasoning_effort.as_deref()) {
            (ThinkingMode::Off, _) => serde_json::Value::Bool(false),
            (ThinkingMode::On, None | Some("none")) => serde_json::Value::Bool(true),
            (ThinkingMode::On, Some(effort @ ("low" | "medium" | "high"))) => {
                serde_json::Value::String(effort.to_string())
            }
            (ThinkingMode::On, Some(_)) => {
                return Err(AppError::InvalidInput(
                    "Ollama supports thinking on or off, or low, medium, and high levels".into(),
                ));
            }
        };
        // Ollama's chat API has no `tool_choice`; a round that forbids calls
        // still offers the tools, and the tool loop ignores any call made in it.

        Ok(OllamaChatRequest {
            model: self.model_name.clone(),
            messages,
            stream: true,
            options: Some(options),
            keep_alive: None,
            tools,
            format: request.json_schema.clone(),
            think: Some(think),
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
mod tests;
