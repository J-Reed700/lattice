//! llama-server over its OpenAI-compatible Chat Completions routes.
//!
//! One client for the bundled sidecar and for a server the user runs: the
//! same messages, tool calls, reasoning, streaming, retry, `/props`,
//! `/tokenize` and slot pinning. The two differ only in where the server is
//! (the base URL), how it authenticates (default headers) and who owns its
//! process: a sidecar's client holds the handle that keeps it running.
use crate::features::llm::backend::{
    ignore_retry, ignore_text, read_bounded_json, ModelBackend, Progress,
};
use crate::features::llm::engine::sidecar_manager::SidecarHandle;
use crate::features::llm::engine::GenerationConfig;
use crate::features::llm::fitting::{Fitted, OutputLimits, ResponseBudget, Thinking};
use crate::features::llm::scheduler::SchedulerRegistry;
use async_trait::async_trait;
use futures::StreamExt;
use lattice_core::application::{
    contracts::settings::{LLMSettingsDto, LlamaCppSettingsDto},
    ports::llm_port::{
        CompletionInput, CompletionRequest, CompletionResponse, LLMPort, RemoteHeader,
        ThinkingPolicy,
    },
};
use lattice_core::domain::models::vision::VisionGeometry;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::http::{
    auth_header_map, loopback_client_builder, reqwest_client_builder,
};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

mod completion;
mod messages;
#[cfg(test)]
#[path = "native_thinking_tests.rs"]
mod native_thinking_tests;
#[cfg(test)]
mod props_tests;
mod repetition;
mod request;
#[cfg(test)]
mod request_tests;
mod retry;
#[cfg(test)]
mod stream_guard_tests;
pub(crate) mod streaming;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use completion::parse_completion;
pub(crate) use completion::parse_completion_with_budget;
pub(crate) use messages::chat_messages;

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
    /// Whether the model supports native thinking (`<think>` / `enable_thinking`).
    pub supports_thinking: bool,
    /// Whether the model itself pins thinking; the user's override beats it.
    pub model_thinking: ThinkingPolicy,
    /// Hold the server to [`retry::prefill_allowance`] before it starts
    /// answering. Set for a bundled server: every request reaching it went
    /// through its scheduler, so nothing of anyone else's queues ahead and
    /// silence means it has hung. A server the user runs may be busy with
    /// other clients' work; waiting for it is bounded only by the time budget.
    pub prefill_guard: bool,
    /// The geometry the model was declared to see with; `None` refuses
    /// pictures.
    pub vision: Option<VisionGeometry>,
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
    /// A server the user runs, as Chat settings describe it, reached with
    /// `header` (see `remote_auth::header`).
    pub fn new(settings: &LLMSettingsDto, header: Option<RemoteHeader>) -> Result<Self> {
        let connection = &settings.llama_cpp;
        let base_url = connection.validated_base_url()?;
        if connection.model.trim().is_empty() {
            return Err(AppError::InvalidConfig(
                "Choose a llama.cpp model in Chat settings".into(),
            ));
        }
        Ok(Self {
            client: remote_client(header)?,
            base_url,
            config: ServerConfig {
                model: connection.model.clone(),
                generation: GenerationConfig::from_settings(settings),
                context_window: settings.context_window as usize,
                stall_timeout: Duration::from_secs(u64::from(settings.timeout_seconds.max(1))),
                supports_tools: true,
                // Determined from `/props` `chat_template` in `schedule`; never guessed by model name.
                supports_thinking: false,
                // A server the user runs names no file to read a header from.
                model_thinking: ThinkingPolicy::PerRequest,
                prefill_guard: false,
                vision: None,
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
        supports_thinking: bool,
        model_thinking: ThinkingPolicy,
        vision: Option<VisionGeometry>,
    ) -> Result<Self> {
        let config = ServerConfig {
            model,
            generation,
            context_window: process.context_size() as usize,
            stall_timeout: SIDECAR_STALL_TIMEOUT,
            supports_tools,
            supports_thinking,
            model_thinking,
            prefill_guard: true,
            vision,
        };
        let mut client = Self::connect(process.endpoint(), process.api_token(), config)?;
        client.process = Some(process);
        Ok(client)
    }

    /// This client with the geometry its model was declared to see with.
    pub fn with_vision(mut self, vision: Option<VisionGeometry>) -> Self {
        self.config.vision = vision;
        self
    }

    /// Whether this llama.cpp client can think natively on demand
    /// (`ThinkingMode::On`), taking the model's thinking capability, its
    /// policy, and the user's thinking override into account.
    pub fn supports_native_thinking(&self) -> bool {
        match self
            .config
            .generation
            .thinking_override
            .or(self.config.model_thinking)
        {
            ThinkingPolicy::AlwaysOn => true,
            ThinkingPolicy::PerRequest => self.config.supports_thinking,
            ThinkingPolicy::AlwaysOff => false,
        }
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
            client: http_client(loopback_client_builder(), headers)?,
            base_url: format!("{}/v1", endpoint.trim_end_matches('/')),
            config,
            process: None,
        })
    }

    pub async fn test_connection(
        connection: &LlamaCppSettingsDto,
        header: Option<RemoteHeader>,
    ) -> Result<Vec<String>> {
        let base_url = connection.validated_base_url()?;
        let client = remote_client(header)?;
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
        let probe_budget = ResponseBudget::for_tokens(2048);
        let value = read_bounded_json(
            response,
            probe_budget.max_whole_bytes,
            || AppError::InvalidState("llama.cpp response exceeds output budget size limit".into()),
            network_error,
            |_| AppError::Network("llama.cpp returned invalid JSON".into()),
        )
        .await?;
        parse_completion_with_budget(value, probe_budget.max_tool_calls)?;
        Ok(models)
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

    /// This client as a port, behind the scheduler its server would have on a
    /// registry of its own.
    pub async fn into_port(self) -> Arc<dyn LLMPort> {
        self.schedule(&SchedulerRegistry::default()).await
    }

    /// Behind this server's shared scheduler, with an exact tokenizer when it
    /// answers like a llama-server.
    ///
    /// A bundled server carries its scheduler on its process handle, sized
    /// from `/props` slots over the `--ctx-size` it was launched with. A
    /// remote one is keyed by URL in `schedulers` and sized from `/props`
    /// alone; one that does not answer it gets one slot and no pinning.
    /// Requests are sized to the window the scheduler was, so a remote
    /// server's own window replaces the configured one.
    pub(crate) async fn schedule(mut self, schedulers: &SchedulerRegistry) -> Arc<dyn LLMPort> {
        use crate::features::llm::scheduler::{
            llama_server_capacity, BackendCapacity, ScheduledLlm,
        };
        use crate::infrastructure::llama_server::ServerTokenizer;
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
                        schedulers.scheduler(
                            format!("llama-server {}", self.server_root()),
                            BackendCapacity::llama_server(slots, window),
                        )
                    })
                    .await;
                (scheduler, true)
            }
            None => {
                let (scheduler, is_llama_server, supports_thinking) = schedulers
                    .remote_llama_cpp(&self.client, self.server_root(), self.config.context_window)
                    .await;
                self.config.supports_thinking = supports_thinking;
                (scheduler, is_llama_server)
            }
        };
        if let Some(window) = scheduler.capacity().shared_window {
            if window != self.config.context_window {
                tracing::info!(
                    server = self.server_root(),
                    configured = self.config.context_window,
                    window,
                    "Sizing requests to the window the server reports"
                );
                self.config.context_window = window;
            }
        }
        let tokenizer =
            is_llama_server.then(|| ServerTokenizer::new(self.client.clone(), self.server_root()));
        let scheduled = ScheduledLlm::new(Arc::new(self), scheduler);
        Arc::new(match tokenizer {
            Some(tokenizer) => scheduled.with_tokenizer(tokenizer),
            None => scheduled,
        })
    }
}

#[async_trait]
impl ModelBackend for LlamaCppLlm {
    fn supports_tool_calling(&self) -> bool {
        self.config.supports_tools
    }
    fn keeps_read_prompts(&self) -> bool {
        true
    }
    fn enforces(
        &self,
        _schema: &lattice_core::application::services::model_contract::shape::Schema,
    ) -> bool {
        true
    }
    fn vision(&self) -> Option<VisionGeometry> {
        self.config.vision
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
    fn window(&self) -> usize {
        self.config.context_window
    }
    fn output_limits(&self) -> OutputLimits {
        OutputLimits::known(u32::try_from(self.config.generation.max_tokens).unwrap_or(u32::MAX))
    }
    fn thinking(&self, request: &CompletionRequest) -> Thinking {
        Thinking::from(self.thinking_mode(request))
    }
    fn supports_native_thinking(&self) -> bool {
        Self::supports_native_thinking(self)
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
    async fn send(
        &self,
        fitted: &Fitted<'_>,
        progress: Progress<'_>,
    ) -> Result<CompletionResponse> {
        match progress {
            Progress::None => {
                self.complete_reliably(fitted, &ignore_text, None, Some(&ignore_retry))
                    .await
            }
            // Text-only consumers cannot retract an already delivered draft.
            Progress::Streamed {
                on_text,
                on_reasoning,
                on_retry,
            } => {
                self.complete_reliably(fitted, on_text, on_reasoning, on_retry)
                    .await
            }
        }
    }
}

/// The client for a server the user runs, sending `header` with every request.
pub fn remote_client(header: Option<RemoteHeader>) -> Result<reqwest::Client> {
    http_client(reqwest_client_builder(), remote_headers(header)?)
}

/// The authentication header of a remote server.
fn remote_headers(header: Option<RemoteHeader>) -> Result<HeaderMap> {
    header.map_or_else(
        || Ok(HeaderMap::new()),
        |header| auth_header_map(&header.name, &header.value),
    )
}

/// No client-wide read or total timeout: generations are bounded by their time
/// budget and stall detection, probes by per-request timeouts.
fn http_client(builder: reqwest::ClientBuilder, headers: HeaderMap) -> Result<reqwest::Client> {
    builder
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

/// `response` when it succeeded; otherwise a typed error, with the server's
/// own words sanitized (see [`streaming::ServerFailure`]).
pub(crate) async fn check_status(response: reqwest::Response) -> Result<reqwest::Response> {
    server_status(response)
        .await
        .map_err(|failure| failure.error(&[]))
}

/// `response` when it succeeded; otherwise the server's account of why not,
/// read from a bounded error envelope.
async fn server_status(
    mut response: reqwest::Response,
) -> std::result::Result<reqwest::Response, streaming::ServerFailure> {
    /// Error envelopes are small; a body past this is not one worth reading.
    const ERROR_BODY_BYTES: usize = 16 * 1024;
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let path = response.url().path().to_owned();
    let retry_after = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs);
    let mut bytes = Vec::new();
    while let Ok(Some(chunk)) = response.chunk().await {
        if bytes.len() + chunk.len() > ERROR_BODY_BYTES {
            break;
        }
        bytes.extend_from_slice(&chunk);
    }
    let body = serde_json::from_slice::<Value>(&bytes).ok();
    Err(streaming::ServerFailure::http(
        status,
        &path,
        body.as_ref(),
        retry_after,
    ))
}
