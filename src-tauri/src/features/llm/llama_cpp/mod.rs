//! llama-server over its OpenAI-compatible Chat Completions routes.
//!
//! One client for the bundled sidecar and for a server the user runs: the
//! same messages, tool calls, reasoning, streaming, retry, `/props`,
//! `/tokenize` and slot pinning. The two differ only in where the server is
//! (the base URL), how it authenticates (default headers) and who owns its
//! process: a sidecar's client holds the handle that keeps it running.
use crate::application::{
    contracts::settings::{LLMSettingsDto, LlamaCppSettingsDto},
    ports::llm_port::{
        CompletionInput, CompletionRequest, CompletionResponse, LLMPort, ToolDefinition,
    },
};
use crate::features::llm::engine::sidecar_manager::SidecarHandle;
use crate::features::llm::engine::GenerationConfig;
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

mod retry;
pub(crate) mod streaming;
#[cfg(test)]
mod tests;

/// How long a bundled server may go silent once it has started answering.
const SIDECAR_STALL_TIMEOUT: Duration = Duration::from_secs(30);

/// `/health` answers in milliseconds on a live server.
const HEALTH_TIMEOUT: Duration = Duration::from_secs(2);

/// What one client asks of its server.
#[derive(Debug, Clone)]
pub(crate) struct ServerConfig {
    /// The model named in each request and reported by the port.
    pub model: String,
    /// The user's sampling defaults and output ceiling.
    pub generation: GenerationConfig,
    /// The window prompt and answer share.
    pub context_window: usize,
    /// Longest silence tolerated once a response has started streaming.
    pub stall_timeout: Duration,
    /// Whether the model's chat template carries a native tool-call format.
    pub supports_tools: bool,
    /// Hold the server to [`retry::prefill_allowance`] before it starts
    /// answering. Set for a bundled server: every request reaching it went
    /// through its scheduler, so nothing of anyone else's queues ahead and
    /// silence means it has hung. A server the user runs may be busy with
    /// other clients' work; waiting for it is bounded only by the time budget.
    pub prefill_guard: bool,
}

pub struct LlamaCppLlm {
    client: reqwest::Client,
    /// The `/v1` root the chat routes live under.
    base_url: String,
    config: ServerConfig,
    /// The bundled server this client keeps running, when it owns one.
    process: Option<Arc<SidecarHandle>>,
}

impl LlamaCppLlm {
    /// A server the user runs, as Chat settings describe it.
    pub fn new(settings: &LLMSettingsDto) -> Result<Self> {
        let connection = &settings.llama_cpp;
        let base_url = validate_connection(connection)?;
        if connection.model.trim().is_empty() {
            return Err(AppError::InvalidConfig(
                "Choose a llama.cpp model in Chat settings".into(),
            ));
        }
        Ok(Self {
            client: http_client(remote_headers(connection)?)?,
            base_url,
            config: ServerConfig {
                model: connection.model.clone(),
                generation: GenerationConfig {
                    temperature: settings.temperature,
                    top_p: settings.top_p,
                    top_k: settings.top_k,
                    max_tokens: settings.max_tokens as usize,
                    repeat_penalty: settings.repeat_penalty,
                },
                context_window: settings.context_window as usize,
                stall_timeout: Duration::from_secs(u64::from(settings.timeout_seconds.max(1))),
                supports_tools: true,
                prefill_guard: false,
            },
            process: None,
        })
    }

    /// The bundled server behind `process`, which this client keeps running.
    ///
    /// Every request carries the server's bearer token as a default header, so
    /// a route added later cannot forget it.
    pub(crate) fn sidecar(
        process: Arc<SidecarHandle>,
        model: String,
        generation: GenerationConfig,
        supports_tools: bool,
    ) -> Result<Self> {
        let config = ServerConfig {
            model,
            generation,
            context_window: process.context_size() as usize,
            stall_timeout: SIDECAR_STALL_TIMEOUT,
            supports_tools,
            prefill_guard: true,
        };
        let mut client = Self::connect(process.endpoint(), process.api_token(), config)?;
        client.process = Some(process);
        Ok(client)
    }

    /// A client for the llama-server at `endpoint` that authenticates with a
    /// bearer `token`, as a bundled server does.
    fn connect(endpoint: &str, token: &str, config: ServerConfig) -> Result<Self> {
        let mut headers = HeaderMap::new();
        let mut bearer = HeaderValue::from_str(&format!("Bearer {token}")).map_err(|_| {
            AppError::InvalidConfig("Sidecar API token is not a valid header".into())
        })?;
        bearer.set_sensitive(true);
        headers.insert(AUTHORIZATION, bearer);
        Ok(Self {
            client: http_client(headers)?,
            base_url: format!("{}/v1", endpoint.trim_end_matches('/')),
            config,
            process: None,
        })
    }

    pub async fn test_connection(connection: &LlamaCppSettingsDto) -> Result<Vec<String>> {
        let base_url = validate_connection(connection)?;
        let client = http_client(remote_headers(connection)?)?;
        let probe_timeout = Duration::from_secs(30);
        let response = client
            .get(format!("{base_url}/models"))
            .timeout(probe_timeout)
            .send()
            .await
            .map_err(network_error)?;
        let response = check_status(response).await?;
        let value: Value = response.json().await.map_err(network_error)?;
        let models: Vec<String> = value
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|m| m.get("id").and_then(Value::as_str))
            .filter(|m| !m.trim().is_empty())
            .map(str::to_owned)
            .collect();
        let model = if connection.model.trim().is_empty() {
            models.first()
        } else {
            models.iter().find(|model| *model == &connection.model)
        }
        .ok_or_else(|| {
            AppError::InvalidConfig("The llama.cpp server did not list the selected model".into())
        })?;
        // A model list alone does not prove that the generation route works.
        let response = client
            .post(format!("{base_url}/chat/completions"))
            .timeout(probe_timeout)
            .json(
                &json!({"model": model, "messages":[{"role":"user","content":"Reply with OK."}],
                "max_tokens": 2048, "stream": false}),
            )
            .send()
            .await
            .map_err(network_error)?;
        let response = check_status(response).await?;
        parse_completion(response.json().await.map_err(network_error)?)?;
        Ok(models)
    }

    /// What the server can still be asked to generate once this prompt is in
    /// its context.
    ///
    /// `max_tokens` is a *reservation*: llama.cpp holds prompt + n_predict in
    /// one slot, so asking for the whole window on every turn works only while
    /// prompts stay small. A turn carrying an attachment and six web pages ran
    /// 75k prompt tokens into a 163k slot, asked for 131k more on top, and the
    /// server failed the batch — a 500 forty-eight seconds in, after it had
    /// already done the work.
    ///
    /// The prompt is measured with the same rough estimate the budgeter uses,
    /// which reads low on dense text, so the estimate is inflated before it is
    /// subtracted. `MIN_OUTPUT_TOKENS` keeps a nearly-full window answerable
    /// rather than silently truncated to nothing; a prompt that leaves less
    /// room than that is over budget for reasons this clamp cannot fix.
    fn output_room_for(&self, messages: &[Value], requested: u32) -> u32 {
        /// Estimated prompt tokens are scaled by this before being subtracted.
        const ESTIMATE_SAFETY: usize = 3;
        const ESTIMATE_SAFETY_DIVISOR: usize = 2;
        const MIN_OUTPUT_TOKENS: u32 = 1024;

        let window = self.config.context_window;
        let prompt_chars: usize = messages
            .iter()
            .map(|message| message.to_string().chars().count())
            .sum();
        let prompt_tokens = crate::application::ports::llm_port::tokens_for_chars(
            prompt_chars,
            crate::application::ports::llm_port::DEFAULT_CHARS_PER_TOKEN,
        ) * ESTIMATE_SAFETY
            / ESTIMATE_SAFETY_DIVISOR;
        let room = u32::try_from(window.saturating_sub(prompt_tokens)).unwrap_or(u32::MAX);
        // The floor guards against the window clamp, never against a cap
        // the caller asked for: a 512-token verdict stays 512.
        let allowed = requested.min(room.max(MIN_OUTPUT_TOKENS));
        if allowed < requested {
            tracing::debug!(
                requested,
                allowed,
                prompt_tokens,
                window,
                "Capped the llama.cpp output reservation to what the window has left"
            );
        }
        allowed
    }

    /// The streamed Chat Completions body for `request`, refusing tool traffic
    /// a model without a tool-call template would render as text.
    fn body(&self, request: &CompletionRequest) -> Result<Value> {
        let carries_tools = !request.tools.is_empty()
            || request.input.iter().any(|item| {
                matches!(
                    item,
                    CompletionInput::ToolCall { .. } | CompletionInput::ToolResult { .. }
                )
            });
        if carries_tools && !self.config.supports_tools {
            return Err(AppError::InvalidConfig(
                "This local model does not support tool calling".into(),
            ));
        }
        let messages = chat_messages(&request.input)?;
        let generation = &self.config.generation;
        let configured_output = u32::try_from(generation.max_tokens).unwrap_or(u32::MAX);
        let requested_output = request.effective_max_output_tokens(configured_output);
        let max_tokens = self.output_room_for(&messages, requested_output);
        let sampling = request.sampling.unwrap_or_default();
        let mut body = serde_json::Map::from_iter([
            ("model".into(), json!(self.config.model)),
            ("messages".into(), json!(messages)),
            ("stream".into(), json!(true)),
            // A final usage frame, so a streamed completion reports tokens.
            ("stream_options".into(), json!({"include_usage": true})),
            // Prompt-processing events keep a long or queued prompt exempt from
            // stall detection rather than arming it, so a slow first batch is not
            // mistaken for a stalled server. Servers without support ignore the field.
            ("return_progress".into(), json!(true)),
            (
                "temperature".into(),
                json!(sampling.temperature.unwrap_or(generation.temperature)),
            ),
            (
                "top_p".into(),
                json!(sampling.top_p.unwrap_or(generation.top_p)),
            ),
            (
                "top_k".into(),
                json!(sampling.top_k.unwrap_or(generation.top_k)),
            ),
            ("repeat_penalty".into(), json!(generation.repeat_penalty)),
            ("max_tokens".into(), json!(max_tokens)),
        ]);
        // With no level, the template keeps its normal reasoning behaviour.
        if let Some(effort) = request
            .reasoning_effort
            .as_deref()
            .filter(|effort| *effort != "none")
        {
            // Recent builds read the field; the template kwarg is what older
            // ones honour.
            body.insert("reasoning_effort".into(), json!(effort));
            body.insert(
                "chat_template_kwargs".into(),
                json!({"reasoning_effort": effort}),
            );
        }
        if !request.tools.is_empty() {
            body.insert("tools".into(), tool_specs(&request.tools));
            // llama-server lets a reply hold one call unless asked for more,
            // so a round that needs three pages would take three generations.
            body.insert("parallel_tool_calls".into(), json!(true));
        }
        if request.want_logprobs {
            body.insert("logprobs".into(), json!(true));
            body.insert(
                "top_logprobs".into(),
                json!(crate::application::ports::llm_port::FIRST_TOKEN_TOP_LOGPROBS),
            );
        }
        if let Some(schema) = &request.json_schema {
            body.insert(
                "response_format".into(),
                json!({"type":"json_schema","json_schema":{"name":"response","schema":schema}}),
            );
        }
        // The scheduler's slot: a conversation returns to the slot whose KV
        // cache holds its prefix. The retry path may still turn reuse off.
        if let Some(slot) = request.assigned_slot {
            body.insert("id_slot".into(), json!(slot));
            body.insert("cache_prompt".into(), json!(true));
        }
        Ok(Value::Object(body))
    }

    /// The server's root URL, without the `/v1` the chat routes live under.
    fn server_root(&self) -> &str {
        self.base_url.strip_suffix("/v1").unwrap_or(&self.base_url)
    }

    /// Whether the bundled server this client owns has exited. A server the
    /// user runs is never known to be gone.
    fn process_exited(&self) -> bool {
        self.process
            .as_ref()
            .is_some_and(|process| !process.is_running())
    }

    /// Behind this server's shared scheduler, with an exact tokenizer when it
    /// answers like a llama-server.
    ///
    /// A bundled server carries its scheduler on its process handle, sized
    /// from `/props` slots over the `--ctx-size` it was launched with. A
    /// remote one is keyed by URL and sized from `/props` alone; one that does
    /// not answer it gets one slot and no pinning.
    pub(crate) async fn schedule(self) -> Arc<dyn LLMPort> {
        use crate::features::llm::scheduler::{
            llama_server_capacity, remote_llama_cpp_scheduler, BackendCapacity, InferenceScheduler,
            ScheduledLlm, ServerTokenizer,
        };
        let (scheduler, is_llama_server) = match &self.process {
            Some(process) => {
                let scheduler = process
                    .scheduler(|| async {
                        let window = self.config.context_window;
                        let slots = match llama_server_capacity(&self.client, self.server_root())
                            .await
                        {
                            Some((slots, _)) => slots,
                            None => {
                                tracing::warn!(
                                    "llama-server did not report its slots; scheduling one request at a time"
                                );
                                1
                            }
                        };
                        tracing::info!(slots, window, "Local llama-server scheduler sized");
                        InferenceScheduler::new(
                            format!("llama-server {}", self.server_root()),
                            BackendCapacity::llama_server(slots, window),
                        )
                    })
                    .await;
                (scheduler, true)
            }
            None => {
                remote_llama_cpp_scheduler(
                    &self.client,
                    self.server_root(),
                    self.config.context_window,
                )
                .await
            }
        };
        let tokenizer =
            is_llama_server.then(|| ServerTokenizer::new(self.client.clone(), self.server_root()));
        let output_limit = u32::try_from(self.config.generation.max_tokens).unwrap_or(u32::MAX);
        let scheduled = ScheduledLlm::new(Arc::new(self), scheduler, output_limit);
        Arc::new(match tokenizer {
            Some(tokenizer) => scheduled.with_tokenizer(tokenizer),
            None => scheduled,
        })
    }
}

/// A typed request's input as OpenAI chat messages.
///
/// Shared with the bundled sidecar, which is the same llama-server behind the
/// same `/v1/chat/completions` route: one translation means a tool round
/// replays identically whichever of the two the model is reached through.
pub(crate) fn chat_messages(input: &[CompletionInput]) -> Result<Vec<Value>> {
    let mut messages: Vec<Value> = Vec::with_capacity(input.len());
    let mut previous_was_call = false;
    for item in input {
        let is_call = matches!(item, CompletionInput::ToolCall { .. });
        match item {
            CompletionInput::Native { value } => messages.push(value.clone()),
            // Chat templates know system, user, assistant and tool; a
            // developer instruction is a system one to every one of them.
            CompletionInput::Message { role, content } if role == "developer" => {
                messages.push(json!({"role":"system","content":content}))
            }
            CompletionInput::Message { role, content } => {
                messages.push(json!({"role":role,"content":content}))
            }
            CompletionInput::ToolCall {
                id,
                name,
                arguments,
            } => {
                let call = json!({"id":id,"type":"function",
                    "function":{"name":name,"arguments":arguments.to_string()}});
                // Calls made in one round are one assistant message. Replayed
                // as one message each, they break the role alternation that
                // strict chat templates enforce.
                match messages
                    .last_mut()
                    .filter(|_| previous_was_call)
                    .and_then(|last| last.get_mut("tool_calls"))
                    .and_then(Value::as_array_mut)
                {
                    Some(calls) => calls.push(call),
                    None => messages
                        .push(json!({"role":"assistant","content":null,"tool_calls":[call]})),
                }
            }
            CompletionInput::ToolResult { id, output } => {
                messages.push(json!({"role":"tool","tool_call_id":id,"content":output}))
            }
        }
        previous_was_call = is_call;
    }
    coalesce_system_messages(messages)
}

/// Tool definitions in the OpenAI `tools` shape llama-server renders through
/// the model's chat template.
pub(crate) fn tool_specs(tools: &[ToolDefinition]) -> Value {
    json!(tools
        .iter()
        .map(|t| json!({"type":"function",
            "function":{"name":t.name,"description":t.description,"parameters":t.parameters}}))
        .collect::<Vec<_>>())
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

#[async_trait]
impl LLMPort for LlamaCppLlm {
    fn supports_tool_calling(&self) -> bool {
        self.config.supports_tools
    }
    fn provider_name(&self) -> &str {
        if self.process.is_some() {
            "local-sidecar"
        } else {
            "llamacpp"
        }
    }
    fn model_name(&self) -> &str {
        &self.config.model
    }
    fn max_context_tokens(&self) -> usize {
        self.config.context_window
    }
    /// A crashed bundled server leaves this client cached with a dead port;
    /// saying so lets the role cache drop it and start a fresh server.
    fn is_alive(&self) -> bool {
        !self.process_exited()
    }
    /// A bundled server is ready once `/health` answers. A remote one must
    /// also list the configured model, since it may serve several.
    async fn is_ready(&self) -> Result<bool> {
        if self.process.is_some() {
            let health = self
                .client
                .get(format!("{}/health", self.server_root()))
                .timeout(HEALTH_TIMEOUT)
                .send()
                .await;
            return Ok(health.is_ok_and(|response| response.status().is_success()));
        }
        let response = self
            .client
            .get(format!("{}/models", self.base_url))
            .timeout(self.config.stall_timeout)
            .send()
            .await
            .map_err(network_error)?;
        let response = check_status(response).await?;
        let value: Value = response.json().await.map_err(network_error)?;
        Ok(value
            .get("data")
            .and_then(Value::as_array)
            .is_some_and(|models| {
                models
                    .iter()
                    .any(|model| model.get("id").and_then(Value::as_str) == Some(self.model_name()))
            }))
    }
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.complete_reliably(request, &|_| Ok(()), None, Some(&|_| Ok(())))
            .await
    }
    async fn complete_with_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        // Text-only consumers cannot retract an already delivered draft.
        self.complete_reliably(request, on_text, None, None).await
    }

    async fn complete_with_retry_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_retry: &(dyn Fn(usize) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        self.complete_reliably(request, on_text, None, Some(on_retry))
            .await
    }

    async fn complete_with_reasoning_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_reasoning: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_retry: &(dyn Fn(usize) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        self.complete_reliably(request, on_text, Some(on_reasoning), Some(on_retry))
            .await
    }
}

pub fn validate_connection(connection: &LlamaCppSettingsDto) -> Result<String> {
    let raw = connection.url.trim().trim_end_matches('/');
    let parsed = url::Url::parse(raw)
        .map_err(|_| AppError::InvalidConfig("Invalid llama.cpp server URL".into()))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(AppError::InvalidConfig("Use an HTTP(S) server URL without credentials, query, or fragment; configure authentication in the header fields".into()));
    }
    if connection.auth_header_name.trim().is_empty()
        ^ connection.auth_header_value.trim().is_empty()
    {
        return Err(AppError::InvalidConfig(
            "Set both authentication header fields, or clear both".into(),
        ));
    }
    Ok(if raw.ends_with("/v1") {
        raw.to_owned()
    } else {
        format!("{raw}/v1")
    })
}

/// The authentication header Chat settings configure for a remote server.
fn remote_headers(connection: &LlamaCppSettingsDto) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    if !connection.auth_header_name.trim().is_empty() {
        let name = HeaderName::from_bytes(connection.auth_header_name.trim().as_bytes())
            .map_err(|_| AppError::InvalidConfig("Invalid authentication header name".into()))?;
        let mut value = HeaderValue::from_str(connection.auth_header_value.trim())
            .map_err(|_| AppError::InvalidConfig("Invalid authentication header value".into()))?;
        value.set_sensitive(true);
        headers.insert(name, value);
    }
    Ok(headers)
}

/// No client-wide read or total timeout: generations are bounded by their time
/// budget and stall detection, probes by per-request timeouts.
fn http_client(headers: HeaderMap) -> Result<reqwest::Client> {
    crate::shared::http::reqwest_client_builder()
        .default_headers(headers)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(network_error)
}

fn network_error(error: reqwest::Error) -> AppError {
    // Do not include headers, bodies, or submitted conversation text in diagnostics.
    if error.is_timeout() {
        AppError::Network("llama.cpp request timed out".into())
    } else {
        AppError::Network("llama.cpp request failed; check the server connection".into())
    }
}

pub(crate) async fn check_status(mut response: reqwest::Response) -> Result<reqwest::Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let mut description = format!(
        "llama.cpp returned HTTP {} from {}",
        status,
        response.url().path()
    );
    // Read a bounded error envelope. Template errors can contain prompt fragments,
    // so expose recognized causes rather than copying arbitrary server bodies.
    let mut bytes = Vec::new();
    while let Ok(Some(chunk)) = response.chunk().await {
        if bytes.len() + chunk.len() > 16 * 1024 {
            break;
        }
        bytes.extend_from_slice(&chunk);
    }
    let error = serde_json::from_slice::<Value>(&bytes).ok();
    if let Some(message) = error
        .as_ref()
        .and_then(|e| e.pointer("/error/message"))
        .and_then(Value::as_str)
    {
        if message.contains("System message must be at the beginning") {
            description.push_str(": the model requires a single system message at the beginning");
        } else if message.contains("Unable to generate parser for this template") {
            description.push_str(": the model's chat template rejected the message format");
        }
    }
    if status.as_u16() == 429 {
        Err(AppError::RateLimitExceeded(description))
    } else if status.as_u16() == 408 {
        Err(AppError::Network(description))
    } else if status.is_client_error() {
        Err(AppError::InvalidState(description))
    } else {
        Err(AppError::ServiceNotAvailable(description))
    }
}

/// More calls than any round needs; past this a model is looping, not working.
const MAX_TOOL_CALLS_PER_ROUND: usize = 128;

/// Stands in for the name of a call that gave none, so the replayed history
/// still has a name the chat template can render.
const UNNAMED_TOOL: &str = "unnamed_tool";

/// The arguments of a tool call the model wrote but that cannot be run.
///
/// Kept as a JSON object so the replayed history stays parseable, carrying the
/// model's raw text so the error it is shown quotes what it actually wrote.
pub(crate) fn invalid_tool_arguments(raw: &str, problem: &str) -> Value {
    json!({"invalid": true, "raw": raw, "error": problem})
}

/// Why a call cannot be run, when its arguments are the marker
/// [`invalid_tool_arguments`] wrote.
pub(crate) fn invalid_tool_call_problem(arguments: &Value) -> Option<String> {
    let object = arguments.as_object()?;
    if object.len() != 3 || object.get("invalid") != Some(&Value::Bool(true)) {
        return None;
    }
    let raw = object.get("raw")?.as_str()?;
    let problem = object.get("error")?.as_str()?;
    Some(format!(
        "Tool call not run: {problem}. You wrote: {}. Call the tool again with a complete JSON object of arguments, or answer from what you have.",
        crate::shared::text::safe_truncate(raw, 400)
    ))
}

pub(crate) fn parse_completion(value: Value) -> Result<CompletionResponse> {
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
        .take(MAX_TOOL_CALLS_PER_ROUND)
        .collect();
    let mut calls = Vec::new();
    let mut ids = std::collections::HashSet::new();
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
    let dropped = message
        .get("tool_calls")
        .and_then(Value::as_array)
        .is_some_and(|all| all.len() > raw_calls.len());
    if repaired || dropped {
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
    let reasoning = ["reasoning_content", "reasoning"]
        .iter()
        .find_map(|key| message.get(key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned);
    Ok(CompletionResponse {
        text: message
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
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
            .and_then(crate::application::ports::llm_port::first_token_logprobs),
    })
}
