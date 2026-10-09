//! `SidecarLLMClient` — HTTP client for the bundled `llama-server` sidecar.
//!
//! Implements the `LLMClient` trait by talking to a local llama-server
//! process over loopback HTTP, using the OpenAI-compatible
//! `/v1/chat/completions` endpoint. Holds a share of the `SidecarHandle`, so
//! the process dies once no client is left on it — one server can be serving
//! several roles.
//!
//! # Architecture
//!
//! ```text
//! LLMClient trait               SidecarLLMClient                llama-server
//! ───────────────────────────► ───────────────────────────► ────────────
//!   generate(...)               POST /v1/chat/completions      Metal/Vulkan/CPU
//!   generate_stream(...)        POST /v1/chat/completions      ggml backend
//!                               (stream=true, SSE)
//!   health_check()              GET  /health
//! ```
//!
//! The SSE wire format is OpenAI-standard: `data: {json}\n\n` lines,
//! terminated by `data: [DONE]`. Each chunk's
//! `choices[0].delta.content` is the next token piece.
//!
//! # Authentication
//!
//! The sidecar is launched with a per-process `--api-key`, so every request —
//! `/health` included — carries it as a bearer token. It goes on as a default
//! header rather than at each call site so a route added later cannot forget it.
//!
//! # Why we don't use llama.cpp's native `/completion`
//!
//! The native endpoint is older, has a different streaming format
//! (newline-delimited JSON, no `data:` prefix, no `[DONE]` sentinel),
//! and changes more often than the OpenAI-compat layer. Standardizing
//! on `/v1/chat/completions` lets us swap llama-server for any
//! OpenAI-compatible backend later (vLLM, mistralrs-server, etc.)
//! without changing this file.

use crate::application::ports::llm_port::{CompletionResponse, SamplingOverride};
use crate::features::llm::engine::sidecar_manager::SidecarHandle;
use crate::features::llm::engine::traits::{ChatMessage, GenerationConfig, LLMClient};
use crate::features::llm::engine::types::LLMError;
use crate::features::llm::llama_cpp::{parse_completion, streaming::Decoder};
use crate::shared::error::{AppError, Result as AppResult};
use async_stream::stream;
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use tokio_stream::Stream;

#[cfg(test)]
mod live_smoke;

/// Floor on how long to wait for the next frame of a streamed response
/// before the model has generated anything. The server may be loading the
/// model into VRAM or running the prompt prefill; on a 7B Q4_K_M GGUF that
/// is sub-second on Apple Silicon but can be 10s+ on cold Windows Vulkan.
/// [`prefill_allowance`] adds time for the prompt on top of this.
const FIRST_TOKEN_TIMEOUT: Duration = Duration::from_secs(60);

/// llama-server's default logical batch. With `return_progress` it reports
/// after each batch it processes, so one batch is the most prompt that can
/// pass between two frames however long the prompt is.
const PREFILL_BATCH_TOKENS: usize = 2048;

/// Slowest prefill rate the allowance plans for: a CPU-only box runs a
/// near-full RAG prompt at roughly 30-60 tok/s, so a batch there takes over a
/// minute and a flat 60 s failed the turn before its first token.
const PREFILL_FLOOR_TOKENS_PER_SEC: usize = 20;

/// Per-chunk timeout once a stream is active. If the model goes silent
/// for this long mid-generation, the connection is dead and we should
/// surface an error rather than hang.
const CHUNK_TIMEOUT: Duration = Duration::from_secs(30);

/// Total timeout for a non-streaming call whose caller set no budget. A
/// 1024-token completion at ~30 tok/s is ~30s; doubled for safety on slow
/// hardware.
const NON_STREAM_TIMEOUT: Duration = Duration::from_secs(120);

/// The caller's budget when it sets one, whatever its length: memory
/// extraction asking for 8192 tokens at 20-30 tok/s needs minutes, and a cap
/// here cut it off while the caller still had budget. A query rewrite allowed
/// twenty seconds still gets twenty. The constant only covers callers that
/// set no budget.
fn non_stream_timeout(time_budget: Option<Duration>) -> Duration {
    time_budget.unwrap_or(NON_STREAM_TIMEOUT)
}

/// How long to wait for a frame before generation starts: the floor plus one
/// batch of this prompt at the slowest prefill rate. Progress frames reset the
/// wait, so it only has to cover the gap between two of them — or, from a
/// server that sends none, the whole prompt, which is why short prompts are
/// not given the full batch. Tokens are estimated at the default ratio.
fn prefill_allowance(messages: &[Value]) -> Duration {
    let chars: usize = messages.iter().map(|m| m.to_string().chars().count()).sum();
    let tokens = crate::application::ports::llm_port::tokens_for_chars(
        chars,
        crate::application::ports::llm_port::DEFAULT_CHARS_PER_TOKEN,
    )
    .min(PREFILL_BATCH_TOKENS);
    FIRST_TOKEN_TIMEOUT + Duration::from_secs((tokens / PREFILL_FLOOR_TOKENS_PER_SEC) as u64)
}

/// The wait for each frame of a stream. Before the first generated delta the
/// server is still processing the prompt and every frame it sends (progress
/// included) restarts the prefill allowance; once it is generating, silence
/// for [`CHUNK_TIMEOUT`] is a stall.
struct FrameClock {
    watch: crate::features::llm::llama_cpp::ProgressWatch,
    generating: bool,
    prefill: Duration,
}

impl FrameClock {
    fn new(prefill: Duration) -> Self {
        Self {
            watch: Default::default(),
            generating: false,
            prefill,
        }
    }

    fn wait(&self) -> Duration {
        if self.generating {
            CHUNK_TIMEOUT
        } else {
            self.prefill
        }
    }

    fn saw(&mut self, chunk: &[u8]) {
        self.generating |= self.watch.carries_generated_delta(chunk);
    }
}

/// Health check timeout — should be milliseconds; if it isn't, the
/// sidecar is in trouble and we should report unhealthy fast.
const HEALTH_TIMEOUT: Duration = Duration::from_secs(2);

/// OpenAI-shaped chat completion request. We use `serde_json::Value`
/// for fields llama-server's OpenAI compat layer accepts but our trait
/// doesn't expose, to keep the surface minimal.
#[derive(Debug, Serialize)]
struct ChatCompletionRequest<'a> {
    /// Model identifier. `SidecarManager` spawns single-model servers
    /// (one `-m <path>` flag at startup), so llama-server serves
    /// whatever weights it loaded regardless of what the request
    /// specifies. We send a meaningful value (the GGUF basename) so
    /// server logs stay readable; switching active models is done by
    /// killing and respawning the sidecar, matching LM Studio's
    /// per-runtime model lifecycle.
    model: &'a str,
    /// OpenAI chat messages. Values rather than role/content pairs because a
    /// tool round carries `tool_calls` on assistant turns and `tool_call_id`
    /// on tool turns.
    messages: Vec<Value>,
    stream: bool,
    temperature: f32,
    /// Nucleus sampling.
    top_p: f32,
    /// Top-k sampling. llama-server-specific extension to OpenAI shape.
    top_k: i32,
    /// Repetition penalty. llama-server-specific extension.
    repeat_penalty: f32,
    max_tokens: usize,
    /// Mirrors what the remote llama.cpp adapter sends, so a model behaves the
    /// same whether it is reached through the bundled sidecar or a server the
    /// user runs themselves. Recent llama-server builds read it; older ones
    /// ignore unknown fields, which is why the template kwarg below carries the
    /// actual guarantee.
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<&'a str>,
    /// Forward a requested reasoning level to the model's own template.
    /// With no level, keep the template's normal reasoning behavior.
    #[serde(skip_serializing_if = "Option::is_none")]
    chat_template_kwargs: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<Value>,
    /// Rendered through the GGUF's own chat template (the server runs with
    /// `--jinja`), which is what gives each model its native call format.
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<&'a Value>,
    /// Sent with tools: llama-server lets a reply hold one call unless asked
    /// for more, so a round that needs three pages would take three
    /// generations. The remote adapter sends the same.
    #[serde(skip_serializing_if = "Option::is_none")]
    parallel_tool_calls: Option<bool>,
    /// Asks for a final usage frame on a stream, so a streamed typed
    /// completion reports tokens the way a non-streamed one does.
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<Value>,
    /// First-token log-probabilities, for a caller that reads its one-word
    /// answer's probability rather than the word alone.
    #[serde(skip_serializing_if = "Option::is_none")]
    logprobs: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_logprobs: Option<u8>,
    /// Streams only: prompt-processing frames while the prompt is prefilled,
    /// so a long prompt shows the server working instead of looking stalled.
    /// The remote adapter sends the same.
    #[serde(skip_serializing_if = "Option::is_none")]
    return_progress: Option<bool>,
    /// The slot the scheduler admitted this request to, so a conversation
    /// returns to the slot whose KV cache already holds its prefix.
    #[serde(skip_serializing_if = "Option::is_none")]
    id_slot: Option<u32>,
    /// Sent with a pinned slot: reuse whatever prefix that slot still holds.
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_prompt: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChatChoiceMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChunk {
    choices: Vec<ChatChunkChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChunkChoice {
    #[serde(default)]
    delta: ChatDelta,
}

#[derive(Debug, Deserialize, Default)]
struct ChatDelta {
    /// Absent on the first/last chunks, `null` on prompt-progress frames,
    /// and the next token piece on content chunks.
    #[serde(default)]
    content: Option<String>,
}

/// Per-request knobs the `LLMClient` trait has no room for.
///
/// The typed `LLMPort` path fills these in from a `CompletionRequest`; the
/// legacy text API leaves them empty and the fields then stay off the wire.
#[derive(Debug, Default, Clone, Copy)]
pub struct RequestTuning<'a> {
    pub reasoning_effort: Option<&'a str>,
    pub json_schema: Option<&'a Value>,
    /// Wall-clock allowance for the whole exchange, when the caller sets one.
    pub time_budget: Option<Duration>,
    pub no_time_limit: bool,
    /// Sampling for this request, over the client's configured defaults.
    pub sampling: Option<SamplingOverride>,
    /// Output ceiling for this request. Only ever tightens the configured one,
    /// so a caller cannot generate past what the user allowed.
    pub max_output_tokens: Option<u32>,
    /// OpenAI-shaped tool definitions, already built by the caller.
    pub tools: Option<&'a Value>,
    /// Ask for the first token's top alternatives with their log-probabilities.
    pub want_logprobs: bool,
    /// The llama-server slot the scheduler assigned, sent as `id_slot`.
    pub slot: Option<u32>,
}

/// LLM client that talks to a bundled llama-server sidecar.
///
/// Shares the `SidecarHandle`; the process dies with its last holder.
pub struct SidecarLLMClient {
    /// Process handle. `Arc` because `SidecarHandle`'s drop kills the
    /// process, so the count is the process's lifetime: dropping one holder
    /// while another still has it would orphan that one.
    ///
    /// The other holders are not just clones of this client. Roles that
    /// resolve to the same model share one server through
    /// [`SidecarManager::start_shared`](super::sidecar_manager::SidecarManager::start_shared),
    /// each with its own client and generation settings over it, so dropping
    /// this client stops the process only if no other role is on it.
    sidecar: Arc<SidecarHandle>,

    /// HTTP client. Reused across calls for connection pooling.
    http: Client,

    /// Display name surfaced via `LLMClient::model_name()`. Typically
    /// the GGUF filename without extension.
    model_name: String,

    /// Mutable generation config for `generation_config_mut()`.
    config: GenerationConfig,
}

/// A failed status as an error that is safe to show and log. A template error
/// quotes the prompt back in its body; the remote adapter's check keeps the
/// status and the causes it recognises and drops the free text.
async fn redacted_status(response: reqwest::Response) -> Result<reqwest::Response, LLMError> {
    crate::features::llm::llama_cpp::check_status(response)
        .await
        .map_err(|err| LLMError::GenerationFailed(err.to_string()))
}

fn typed_error(err: LLMError) -> AppError {
    AppError::Other(format!("LLM completion failed: {err}"))
}

/// Incremental parser for the server's SSE frames.
///
/// Extracted from `parse_sse_stream` so the byte handling is testable
/// without constructing a `reqwest::Response`. Two things it must get
/// right, both of which the previous inline version got wrong:
///
/// 1. **UTF-8 across chunk boundaries.** Bytes arrive in network-sized
///    chunks with no regard for character boundaries. Decoding each chunk
///    independently with `from_utf8_lossy` turned every split character
///    into U+FFFD, so non-English replies came back peppered with
///    replacement characters at random offsets.
///
/// 2. **The final frame.** When the server closes without sending
///    `data: [DONE]`, a frame still sitting in the buffer without its
///    trailing `\n\n` was silently dropped — losing the last tokens of
///    the reply.
#[derive(Default)]
pub(crate) struct SseDecoder {
    /// Text decoded so far, awaiting frame termination.
    buffer: String,
    /// Bytes that do not yet form complete characters (a split sequence).
    pending: Vec<u8>,
    /// Whether the server's `[DONE]` sentinel has been seen.
    done: bool,
}

impl SseDecoder {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// True once `data: [DONE]` has been observed.
    pub(crate) fn is_done(&self) -> bool {
        self.done
    }

    /// Feed a network chunk; returns any content deltas it completed.
    pub(crate) fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        if self.done {
            return Vec::new();
        }

        self.pending.extend_from_slice(bytes);
        self.decode_pending();
        self.drain_frames()
    }

    /// Flush at clean EOF, emitting any residual frame.
    pub(crate) fn finish(&mut self) -> Vec<String> {
        if self.done {
            return Vec::new();
        }

        if !self.pending.is_empty() {
            // Whatever is left cannot be completed; surface it lossily
            // rather than discarding it.
            let tail = std::mem::take(&mut self.pending);
            self.buffer.push_str(&String::from_utf8_lossy(&tail));
        }

        if !self.buffer.trim().is_empty() && !self.buffer.ends_with("\n\n") {
            self.buffer.push_str("\n\n");
        }

        self.drain_frames()
    }

    /// Move as much of `pending` into `buffer` as forms whole characters.
    fn decode_pending(&mut self) {
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(text) => {
                    self.buffer.push_str(text);
                    self.pending.clear();
                    return;
                }
                Err(err) => {
                    let valid = err.valid_up_to();
                    if valid > 0 {
                        if let Some(valid_bytes) = self.pending.get(..valid) {
                            if let Ok(text) = std::str::from_utf8(valid_bytes) {
                                self.buffer.push_str(text);
                            }
                        }
                        self.pending.drain(..valid);
                    }
                    match err.error_len() {
                        // Genuinely invalid bytes: drop them and mark the
                        // damage rather than stalling the stream forever.
                        Some(len) => {
                            let len = len.min(self.pending.len()).max(1);
                            self.pending.drain(..len.min(self.pending.len()));
                            self.buffer.push('\u{FFFD}');
                        }
                        // Truncated tail: wait for the next chunk.
                        None => return,
                    }
                }
            }
        }
    }

    /// Consume every complete `\n\n`-terminated frame in the buffer.
    fn drain_frames(&mut self) -> Vec<String> {
        let mut out = Vec::new();

        while let Some(idx) = self.buffer.find("\n\n") {
            let frame = self.buffer[..idx].to_string();
            self.buffer.drain(..idx + 2);

            for line in frame.lines() {
                let Some(payload) = line.strip_prefix("data:") else {
                    continue;
                };
                let payload = payload.trim();

                if payload == "[DONE]" {
                    self.done = true;
                    return out;
                }

                // Skip empty / malformed payloads — better to drop one bad
                // frame than abort the whole stream.
                match serde_json::from_str::<ChatCompletionChunk>(payload) {
                    Ok(chunk) => {
                        if let Some(choice) = chunk.choices.into_iter().next() {
                            if let Some(content) = choice.delta.content.filter(|c| !c.is_empty()) {
                                out.push(content);
                            }
                        }
                    }
                    Err(err) => {
                        // The payload is generated text, and serde's message
                        // can quote it; the category and position are enough.
                        tracing::warn!(
                            target: "sidecar_client",
                            category = ?err.classify(),
                            column = err.column(),
                            "Skipping malformed SSE chunk"
                        );
                    }
                }
            }
        }

        out
    }
}

impl SidecarLLMClient {
    /// Whether the server this client talks to is still running. Reads the
    /// handle's child slot, which the event drain empties when the process
    /// exits on its own — so a crashed server reads as dead without a request.
    pub fn is_alive(&self) -> bool {
        self.sidecar.is_running()
    }

    /// Create a client wrapping the given sidecar handle.
    pub fn new(
        sidecar: Arc<SidecarHandle>,
        model_name: impl Into<String>,
        config: GenerationConfig,
    ) -> Result<Self, LLMError> {
        // The sidecar's token goes on as a default header rather than at each
        // call site: llama-server rejects every route but `/health` without it,
        // and a request added later must not be able to forget it. Marked
        // sensitive so reqwest keeps it out of debug output.
        let mut headers = HeaderMap::new();
        let mut bearer = HeaderValue::from_str(&format!("Bearer {}", sidecar.api_token()))
            .map_err(|_| LLMError::Other("Sidecar API token is not a valid header".to_string()))?;
        bearer.set_sensitive(true);
        headers.insert(AUTHORIZATION, bearer);

        let http = Client::builder()
            // Streaming responses can be long. Don't set a global timeout;
            // we apply per-call timeouts via `tokio::time::timeout` instead.
            .pool_idle_timeout(Some(Duration::from_secs(90)))
            .default_headers(headers)
            .build()
            .map_err(|err| {
                LLMError::Other(format!("Failed to build HTTP client for sidecar: {err}"))
            })?;

        Ok(Self {
            sidecar,
            http,
            model_name: model_name.into(),
            config,
        })
    }

    /// Context window this client's sidecar was launched with.
    pub fn context_size(&self) -> u32 {
        self.sidecar.context_size()
    }

    /// The scheduler every role on this server shares. Slots come from the
    /// server's `/props` (the pinned build picks four over a unified cache
    /// when `--parallel` is left on auto); the window is the `--ctx-size` it
    /// was launched with, which those slots share.
    pub(crate) async fn scheduler(
        &self,
    ) -> Arc<crate::features::llm::scheduler::InferenceScheduler> {
        use crate::features::llm::scheduler::{
            llama_server_capacity, BackendCapacity, InferenceScheduler,
        };
        self.sidecar
            .scheduler(|| async {
                let window = self.context_size() as usize;
                let slots = match llama_server_capacity(&self.http, self.sidecar.endpoint()).await
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
                    format!("llama-server {}", self.sidecar.endpoint()),
                    BackendCapacity::llama_server(slots, window),
                )
            })
            .await
    }

    /// Exact token counts from this server's own tokenizer.
    pub(crate) fn tokenizer(&self) -> crate::features::llm::scheduler::ServerTokenizer {
        crate::features::llm::scheduler::ServerTokenizer::new(
            self.http.clone(),
            self.sidecar.endpoint(),
        )
    }

    /// The output ceiling this client's settings allow.
    pub(crate) fn output_limit(&self) -> u32 {
        u32::try_from(self.config.max_tokens).unwrap_or(u32::MAX)
    }

    fn endpoint_for(&self, path: &str) -> String {
        format!("{}{}", self.sidecar.endpoint(), path)
    }

    fn build_messages(system: Option<&str>, prompt: &str) -> Vec<Value> {
        let mut msgs = Vec::with_capacity(2);
        if let Some(sys) = system {
            msgs.push(json!({"role": "system", "content": sys}));
        }
        msgs.push(json!({"role": "user", "content": prompt}));
        msgs
    }

    /// Takes the config rather than `&self` so the wire shape can be asserted
    /// without a live sidecar process behind it.
    fn build_request<'a>(
        config: &GenerationConfig,
        messages: Vec<Value>,
        stream: bool,
        tuning: RequestTuning<'a>,
    ) -> ChatCompletionRequest<'a> {
        let sampling = tuning.sampling.unwrap_or_default();
        let reasoning_effort = tuning.reasoning_effort.filter(|effort| *effort != "none");
        ChatCompletionRequest {
            model: "local",
            messages,
            stream,
            temperature: sampling.temperature.unwrap_or(config.temperature),
            top_p: sampling.top_p.unwrap_or(config.top_p),
            top_k: sampling.top_k.unwrap_or(config.top_k),
            repeat_penalty: config.repeat_penalty,
            max_tokens: match tuning.max_output_tokens {
                Some(requested) if requested > 0 => (requested as usize).min(config.max_tokens),
                _ => config.max_tokens,
            },
            reasoning_effort,
            chat_template_kwargs: reasoning_effort.map(|effort| json!({"reasoning_effort": effort})),
            response_format: tuning.json_schema.map(|schema| {
                json!({"type":"json_schema","json_schema":{"name":"response","schema":schema}})
            }),
            tools: tuning.tools,
            parallel_tool_calls: tuning.tools.map(|_| true),
            stream_options: stream.then(|| json!({"include_usage": true})),
            return_progress: stream.then_some(true),
            logprobs: tuning.want_logprobs.then_some(true),
            top_logprobs: tuning
                .want_logprobs
                .then_some(crate::application::ports::llm_port::FIRST_TOKEN_TOP_LOGPROBS),
            id_slot: tuning.slot,
            cache_prompt: tuning.slot.map(|_| true),
        }
    }

    async fn post_chat_completion(
        &self,
        messages: Vec<Value>,
        stream: bool,
        tuning: RequestTuning<'_>,
    ) -> Result<(reqwest::Response, Duration), LLMError> {
        let prefill = prefill_allowance(&messages);
        let body = Self::build_request(&self.config, messages, stream, tuning);
        let request_timeout = if stream {
            // The server may hold the headers until its first frame.
            prefill
        } else {
            non_stream_timeout(tuning.time_budget)
        };

        let send = self
            .http
            .post(self.endpoint_for("/v1/chat/completions"))
            .json(&body)
            .send();
        let response = if tuning.no_time_limit && !stream {
            send.await
        } else {
            timeout(request_timeout, send)
                .await
                .map_err(|_| LLMError::Timeout)?
        }
        .map_err(|err| LLMError::Network(format!("HTTP error to sidecar: {err}")))?;

        Ok((redacted_status(response).await?, prefill))
    }

    async fn extract_completion_text(response: reqwest::Response) -> Result<String, LLMError> {
        let parsed: ChatCompletionResponse = response.json().await.map_err(|err| {
            LLMError::GenerationFailed(format!("Failed to parse sidecar response: {err}"))
        })?;
        parsed
            .choices
            .into_iter()
            .next()
            .map(|choice| choice.message.content)
            .ok_or_else(|| LLMError::GenerationFailed("Sidecar returned zero choices".into()))
    }

    /// Typed completion for the `LLMPort` adapter, in one response.
    ///
    /// Separate from `generate_chat` because the trait cannot carry a reasoning
    /// effort, a response schema or tools, and dropping the first two is what
    /// let a reasoning model spend minutes thinking about a one-line query
    /// rewrite. The response is parsed by the remote llama.cpp adapter's own
    /// parser: same server, same tool-call shape.
    pub async fn complete_typed(
        &self,
        messages: Vec<Value>,
        tuning: RequestTuning<'_>,
    ) -> AppResult<CompletionResponse> {
        let (response, _) = self
            .post_chat_completion(messages, false, tuning)
            .await
            .map_err(typed_error)?;
        let value: Value = response
            .json()
            .await
            .map_err(|err| AppError::Other(format!("Failed to parse sidecar response: {err}")))?;
        parse_completion(value)
    }

    /// Typed completion that hands answer text to `on_text` as it streams,
    /// while tool-call fragments are assembled into whole calls.
    ///
    /// The tool loop takes this path whenever it offers tools; without it a
    /// tool-capable local model would write its whole final answer before the
    /// user saw a word of it.
    pub async fn complete_typed_streaming(
        &self,
        messages: Vec<Value>,
        tuning: RequestTuning<'_>,
        on_text: &(dyn Fn(String) -> AppResult<()> + Send + Sync),
    ) -> AppResult<CompletionResponse> {
        let (response, prefill) = self
            .post_chat_completion(messages, true, tuning)
            .await
            .map_err(typed_error)?;
        Self::drain_typed_stream(response.bytes_stream(), prefill, on_text).await
    }

    /// The chat surface can reveal explicit reasoning while it streams. The
    /// text-only helper above suppresses only live reasoning callbacks; both
    /// paths still return completed reasoning in `CompletionResponse`.
    pub async fn complete_typed_streaming_with_reasoning(
        &self,
        messages: Vec<Value>,
        tuning: RequestTuning<'_>,
        on_text: &(dyn Fn(String) -> AppResult<()> + Send + Sync),
        on_reasoning: &(dyn Fn(String) -> AppResult<()> + Send + Sync),
    ) -> AppResult<CompletionResponse> {
        let (response, prefill) = self
            .post_chat_completion(messages, true, tuning)
            .await
            .map_err(typed_error)?;
        Self::drain_typed_stream_with_reasoning(
            response.bytes_stream(),
            prefill,
            on_text,
            on_reasoning,
        )
        .await
    }

    /// Split from the request so the frame timing can be driven by a scripted
    /// stream on a paused clock.
    async fn drain_typed_stream<S, B, E>(
        bytes: S,
        prefill: Duration,
        on_text: &(dyn Fn(String) -> AppResult<()> + Send + Sync),
    ) -> AppResult<CompletionResponse>
    where
        S: Stream<Item = Result<B, E>> + Unpin,
        B: AsRef<[u8]>,
        E: std::fmt::Display,
    {
        Self::drain_typed_stream_with_reasoning(bytes, prefill, on_text, &|_| Ok(())).await
    }

    async fn drain_typed_stream_with_reasoning<S, B, E>(
        bytes: S,
        prefill: Duration,
        on_text: &(dyn Fn(String) -> AppResult<()> + Send + Sync),
        on_reasoning: &(dyn Fn(String) -> AppResult<()> + Send + Sync),
    ) -> AppResult<CompletionResponse>
    where
        S: Stream<Item = Result<B, E>> + Unpin,
        B: AsRef<[u8]>,
        E: std::fmt::Display,
    {
        let mut bytes = bytes;
        let mut decoder = Decoder::for_completion();
        let mut clock = FrameClock::new(prefill);
        while let Some(chunk) = timeout(clock.wait(), bytes.next())
            .await
            .map_err(|_| typed_error(LLMError::Timeout))?
        {
            let chunk = chunk.map_err(|err| {
                typed_error(LLMError::Network(format!("SSE byte stream error: {err}")))
            })?;
            let chunk = chunk.as_ref();
            clock.saw(chunk);
            for text in decoder.push(chunk)? {
                on_text(text)?;
            }
            let reasoning = decoder.take_reasoning_delta();
            if !reasoning.is_empty() {
                on_reasoning(reasoning)?;
            }
            if decoder.done() {
                break;
            }
        }
        decoder.into_response()
    }

    fn normalize_chat_messages(messages: Vec<ChatMessage>) -> Vec<Value> {
        messages
            .into_iter()
            .map(|m| {
                let role = match m.role.as_str() {
                    "system" => "system",
                    "assistant" => "assistant",
                    _ => "user",
                };
                json!({"role": role, "content": m.content})
            })
            .collect()
    }

    /// Drive the SSE stream into a token-string stream. Centralized so
    /// `generate_stream` and `generate_chat_stream` share the parser.
    fn parse_sse_stream<S, B, E>(
        bytes: S,
        prefill: Duration,
    ) -> Pin<Box<dyn Stream<Item = Result<String, LLMError>> + Send>>
    where
        S: Stream<Item = Result<B, E>> + Send + Unpin + 'static,
        B: AsRef<[u8]> + Send,
        E: std::fmt::Display + Send,
    {
        Box::pin(stream! {
            let mut bytes = bytes;
            let mut decoder = SseDecoder::new();
            let mut clock = FrameClock::new(prefill);

            loop {
                let next = match timeout(clock.wait(), bytes.next()).await {
                    Ok(next) => next,
                    Err(_) => {
                        yield Err(LLMError::Timeout);
                        return;
                    }
                };

                match next {
                    Some(Ok(b)) => {
                        clock.saw(b.as_ref());
                        for content in decoder.push(b.as_ref()) {
                            yield Ok(content);
                        }
                        if decoder.is_done() {
                            return;
                        }
                    }
                    Some(Err(err)) => {
                        yield Err(LLMError::Network(format!("SSE byte stream error: {err}")));
                        return;
                    }
                    None => {
                        // Server closed: flush any frame left without its
                        // trailing "\n\n" instead of discarding it.
                        for content in decoder.finish() {
                            yield Ok(content);
                        }
                        return;
                    }
                }
            }
        })
    }
}

#[async_trait]
impl LLMClient for SidecarLLMClient {
    async fn generate(
        &self,
        prompt: &str,
        system: Option<&str>,
        _images: Option<Vec<String>>,
    ) -> Result<String, LLMError> {
        let messages = Self::build_messages(system, prompt);
        let (response, _) = self
            .post_chat_completion(messages, false, RequestTuning::default())
            .await?;
        Self::extract_completion_text(response).await
    }

    async fn generate_stream(
        &self,
        prompt: &str,
        system: Option<&str>,
        _images: Option<Vec<String>>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LLMError>> + Send + '_>>, LLMError> {
        let messages = Self::build_messages(system, prompt);
        let (response, prefill) = self
            .post_chat_completion(messages, true, RequestTuning::default())
            .await?;
        Ok(Self::parse_sse_stream(response.bytes_stream(), prefill))
    }

    async fn health_check(&self) -> bool {
        let url = self.endpoint_for("/health");
        match timeout(HEALTH_TIMEOUT, self.http.get(url).send()).await {
            Ok(Ok(resp)) => resp.status().is_success(),
            _ => false,
        }
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    fn generation_config(&self) -> &GenerationConfig {
        &self.config
    }

    fn generation_config_mut(&mut self) -> &mut GenerationConfig {
        &mut self.config
    }

    async fn generate_chat(&self, messages: Vec<ChatMessage>) -> Result<String, LLMError> {
        let pairs = Self::normalize_chat_messages(messages);
        let (response, _) = self
            .post_chat_completion(pairs, false, RequestTuning::default())
            .await?;
        Self::extract_completion_text(response).await
    }

    async fn generate_chat_stream(
        &self,
        messages: Vec<ChatMessage>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LLMError>> + Send + '_>>, LLMError> {
        let pairs = Self::normalize_chat_messages(messages);
        let (response, prefill) = self
            .post_chat_completion(pairs, true, RequestTuning::default())
            .await?;
        Ok(Self::parse_sse_stream(response.bytes_stream(), prefill))
    }

    fn supports_chat(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    //! These tests target the request-shaping and SSE-parsing logic.
    //!
    //! The SSE byte handling is exercised through `SseDecoder`, which exists
    //! precisely so this logic can be tested without a live HTTP response.
    //! Lifecycle integration tests (real sidecar process) live in
    //! `tests/sidecar_integration.rs` — they require a llama-server
    //! binary on disk and a tiny GGUF model.
    //!
    //! Note: full end-to-end tests on Windows are blocked by the
    //! esaxx-rs/cxx CRT-mismatch linker error; these tests pass on
    //! macOS and Linux where linking works.

    fn content_frame(text: &str) -> String {
        format!(
            "data: {}\n\n",
            serde_json::json!({ "choices": [ { "delta": { "content": text } } ] })
        )
    }

    /// A multi-byte character split across two network chunks must survive.
    /// Decoding each chunk with `from_utf8_lossy` independently turned the
    /// split character into U+FFFD.
    #[test]
    fn sse_decoder_reassembles_characters_split_across_chunks() {
        let frame = content_frame("你好世界");
        let bytes = frame.as_bytes();

        // Split at every possible offset; none may corrupt the output.
        for split in 1..bytes.len() {
            let mut decoder = SseDecoder::new();
            let mut out = String::new();
            for chunk in decoder.push(&bytes[..split]) {
                out.push_str(&chunk);
            }
            for chunk in decoder.push(&bytes[split..]) {
                out.push_str(&chunk);
            }
            for chunk in decoder.finish() {
                out.push_str(&chunk);
            }

            assert_eq!(
                out, "你好世界",
                "split at byte {} corrupted the stream",
                split
            );
            assert!(
                !out.contains('\u{FFFD}'),
                "split at byte {} produced a replacement character",
                split
            );
        }
    }

    /// Emoji are 4-byte sequences — the widest split window.
    #[test]
    fn sse_decoder_handles_emoji_split_across_chunks() {
        let frame = content_frame("ship it 🚀🔥");
        let bytes = frame.as_bytes();

        for split in 1..bytes.len() {
            let mut decoder = SseDecoder::new();
            let mut out = String::new();
            for chunk in decoder.push(&bytes[..split]) {
                out.push_str(&chunk);
            }
            for chunk in decoder.push(&bytes[split..]) {
                out.push_str(&chunk);
            }
            for chunk in decoder.finish() {
                out.push_str(&chunk);
            }
            assert_eq!(
                out, "ship it 🚀🔥",
                "split at byte {} corrupted output",
                split
            );
        }
    }

    /// A frame left in the buffer when the server closes without `[DONE]`
    /// used to be discarded, losing the last tokens of the reply.
    #[test]
    fn sse_decoder_flushes_the_final_frame_without_done_sentinel() {
        let mut decoder = SseDecoder::new();
        let mut out = String::new();

        for chunk in decoder.push(content_frame("hello ").as_bytes()) {
            out.push_str(&chunk);
        }

        // Final frame arrives with no trailing blank line and no [DONE].
        let tail = format!(
            "data: {}",
            serde_json::json!({ "choices": [ { "delta": { "content": "world" } } ] })
        );
        for chunk in decoder.push(tail.as_bytes()) {
            out.push_str(&chunk);
        }

        assert_eq!(out, "hello ", "unterminated frame must not emit early");

        for chunk in decoder.finish() {
            out.push_str(&chunk);
        }
        assert_eq!(out, "hello world", "final frame must be flushed on EOF");
    }

    #[test]
    fn sse_decoder_stops_at_done_sentinel() {
        let mut decoder = SseDecoder::new();
        let mut out = String::new();

        let stream = format!(
            "{}data: [DONE]\n\n{}",
            content_frame("kept"),
            content_frame("ignored")
        );
        for chunk in decoder.push(stream.as_bytes()) {
            out.push_str(&chunk);
        }

        assert_eq!(out, "kept");
        assert!(decoder.is_done());
        assert!(decoder.finish().is_empty(), "nothing may follow [DONE]");
    }

    #[test]
    fn sse_decoder_skips_malformed_frames_without_aborting() {
        let mut decoder = SseDecoder::new();
        let mut out = String::new();

        let stream = format!(
            "{}data: {{not json}}\n\n{}",
            content_frame("before "),
            content_frame("after")
        );
        for chunk in decoder.push(stream.as_bytes()) {
            out.push_str(&chunk);
        }

        assert_eq!(
            out, "before after",
            "one bad frame must not kill the stream"
        );
    }

    use super::*;

    #[test]
    fn build_request_serializes_to_openai_shape() {
        let messages = SidecarLLMClient::build_messages(Some("You are helpful."), "Hi.");
        let body = SidecarLLMClient::build_request(
            &GenerationConfig::default(),
            messages,
            false,
            RequestTuning::default(),
        );

        let json = serde_json::to_value(&body).expect("serialize");
        assert_eq!(json["model"], "local");
        assert_eq!(json["stream"], false);
        let messages_json = json["messages"].as_array().expect("messages array");
        assert_eq!(messages_json.len(), 2);
        assert_eq!(messages_json[0]["role"], "system");
        assert_eq!(messages_json[0]["content"], "You are helpful.");
        assert_eq!(messages_json[1]["role"], "user");
        assert_eq!(messages_json[1]["content"], "Hi.");
        assert!(json.get("tools").is_none(), "{json}");
        assert!(json.get("parallel_tool_calls").is_none(), "{json}");
        assert!(json.get("stream_options").is_none(), "{json}");
        assert!(json.get("id_slot").is_none(), "{json}");
        assert!(json.get("cache_prompt").is_none(), "{json}");
    }

    /// The scheduler's slot goes on the wire, with prompt reuse, so the
    /// conversation lands on the slot whose KV cache holds its prefix.
    #[test]
    fn a_scheduled_request_is_pinned_to_its_slot_with_prompt_reuse() {
        let messages = SidecarLLMClient::build_messages(None, "Hi.");
        let body = SidecarLLMClient::build_request(
            &GenerationConfig::default(),
            messages,
            true,
            RequestTuning {
                slot: Some(2),
                ..RequestTuning::default()
            },
        );
        let json = serde_json::to_value(&body).expect("serialize");
        assert_eq!(json["id_slot"], 2);
        assert_eq!(json["cache_prompt"], true);
    }

    /// A tool round has to reach llama-server in the same shape the remote
    /// adapter sends: `tools` on the request, the assistant's call and the
    /// tool's result as their own typed messages, not flattened into text.
    #[test]
    fn a_tool_round_goes_out_in_openai_tool_shape() {
        use crate::application::ports::llm_port::{CompletionInput, ToolDefinition};

        let input = [
            CompletionInput::Message {
                role: "system".into(),
                content: "Answer from the library.".into(),
            },
            CompletionInput::Message {
                role: "user".into(),
                content: "What does the lease say?".into(),
            },
            CompletionInput::ToolCall {
                id: "call_1".into(),
                name: "semantic_search".into(),
                arguments: json!({"query": "lease term"}),
            },
            CompletionInput::ToolResult {
                id: "call_1".into(),
                output: "The term is 12 months.".into(),
            },
        ];
        let tools = crate::features::llm::llama_cpp::tool_specs(&[ToolDefinition {
            name: "semantic_search".into(),
            description: "Search the library".into(),
            parameters: json!({"type": "object"}),
        }]);
        let messages = crate::features::llm::llama_cpp::chat_messages(&input).expect("messages");
        let body = SidecarLLMClient::build_request(
            &GenerationConfig::default(),
            messages,
            true,
            RequestTuning {
                tools: Some(&tools),
                ..RequestTuning::default()
            },
        );

        let json = serde_json::to_value(&body).expect("serialize");
        assert_eq!(json["tools"][0]["function"]["name"], "semantic_search");
        assert_eq!(json["parallel_tool_calls"], true);
        assert_eq!(json["stream_options"]["include_usage"], true);
        let messages = json["messages"].as_array().expect("messages");
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[2]["tool_calls"][0]["id"], "call_1");
        assert_eq!(
            messages[2]["tool_calls"][0]["function"]["arguments"],
            r#"{"query":"lease term"}"#
        );
        assert_eq!(messages[3]["role"], "tool");
        assert_eq!(messages[3]["tool_call_id"], "call_1");
    }

    /// Tool-call fragments arrive spread over several frames; the shared
    /// decoder must hand back one whole call and no answer text for it.
    #[test]
    fn a_streamed_tool_call_is_reassembled_into_one_call() {
        use crate::application::ports::llm_port::CompletionInput;

        let frames = [
            json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_7","type":"function","function":{"name":"semantic_search","arguments":"{\"query\":"}}]}}]}),
            json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"lease\"}"}}]}}]}),
            json!({"choices":[{"delta":{},"finish_reason":"tool_calls"}]}),
            json!({"choices":[],"usage":{"prompt_tokens":40,"completion_tokens":9}}),
        ];
        let mut decoder = Decoder::for_completion();
        let mut text = Vec::new();
        for frame in frames {
            text.extend(
                decoder
                    .push(format!("data: {frame}\n\n").as_bytes())
                    .expect("frame"),
            );
        }
        decoder.push(b"data: [DONE]\n\n").expect("done");
        assert!(text.is_empty());

        let response = decoder.into_response().expect("response");
        assert_eq!(response.finish_reason, "tool_calls");
        assert_eq!(response.input_tokens, 40);
        match response.tool_calls.as_slice() {
            [CompletionInput::ToolCall {
                id,
                name,
                arguments,
            }] => {
                assert_eq!(id, "call_7");
                assert_eq!(name, "semantic_search");
                assert_eq!(arguments, &json!({"query": "lease"}));
            }
            other => panic!("expected one tool call, got {other:?}"),
        }
    }

    /// A legacy internal "none" hint must not disable model reasoning.
    #[test]
    fn legacy_none_keeps_the_models_default_reasoning() {
        let messages = SidecarLLMClient::build_messages(None, "Rewrite this.");
        let body = SidecarLLMClient::build_request(
            &GenerationConfig::default(),
            messages,
            false,
            RequestTuning {
                reasoning_effort: Some("none"),
                json_schema: None,
                time_budget: None,
                no_time_limit: false,
                sampling: None,
                max_output_tokens: None,
                tools: None,
                want_logprobs: false,
                slot: None,
            },
        );

        let json = serde_json::to_value(&body).expect("serialize");
        assert!(json.get("reasoning_effort").is_none());
        assert!(json.get("chat_template_kwargs").is_none());
    }

    #[test]
    fn a_real_reasoning_effort_reaches_the_chat_template_as_itself() {
        let messages = SidecarLLMClient::build_messages(None, "Think about this.");
        let body = SidecarLLMClient::build_request(
            &GenerationConfig::default(),
            messages,
            false,
            RequestTuning {
                reasoning_effort: Some("low"),
                json_schema: None,
                time_budget: None,
                no_time_limit: false,
                sampling: None,
                max_output_tokens: None,
                tools: None,
                want_logprobs: false,
                slot: None,
            },
        );

        let json = serde_json::to_value(&body).expect("serialize");
        assert_eq!(json["reasoning_effort"], "low");
        assert_eq!(json["chat_template_kwargs"]["reasoning_effort"], "low");
    }

    #[test]
    fn a_logprobs_request_asks_for_the_first_tokens_alternatives() {
        let messages = SidecarLLMClient::build_messages(None, "Answer in one word.");
        let body = SidecarLLMClient::build_request(
            &GenerationConfig::default(),
            messages.clone(),
            false,
            RequestTuning {
                want_logprobs: true,
                ..RequestTuning::default()
            },
        );
        let json = serde_json::to_value(&body).expect("serialize");
        assert_eq!(json["logprobs"], true);
        assert!(json["top_logprobs"].as_u64().expect("count") > 1);

        let plain = SidecarLLMClient::build_request(
            &GenerationConfig::default(),
            messages,
            false,
            RequestTuning::default(),
        );
        let json = serde_json::to_value(&plain).expect("serialize");
        assert!(json.get("logprobs").is_none());
        assert!(json.get("top_logprobs").is_none());
    }

    /// The legacy text API has no reasoning hint; those fields must then stay
    /// off the wire entirely rather than going out as nulls.
    #[test]
    fn an_untuned_request_sends_no_reasoning_fields() {
        let messages = SidecarLLMClient::build_messages(None, "Hi.");
        let body = SidecarLLMClient::build_request(
            &GenerationConfig::default(),
            messages,
            false,
            RequestTuning::default(),
        );

        let json = serde_json::to_value(&body).expect("serialize");
        assert!(json.get("reasoning_effort").is_none(), "{json}");
        assert!(json.get("chat_template_kwargs").is_none(), "{json}");
        assert!(json.get("response_format").is_none(), "{json}");
    }

    #[test]
    fn deserialize_chunk_handles_empty_delta() {
        // First and last SSE chunks often have empty/missing content.
        let payload = r#"{"choices":[{"delta":{}}]}"#;
        let chunk: ChatCompletionChunk = serde_json::from_str(payload).expect("parse");
        assert_eq!(chunk.choices.len(), 1);
        assert_eq!(chunk.choices[0].delta.content, None);
    }

    /// llama-server's prompt-progress frame carries `content: null`; the old
    /// `String` field rejected it and logged every one as malformed.
    #[test]
    fn a_prompt_progress_frame_is_not_malformed_and_not_text() {
        let mut decoder = SseDecoder::new();
        let out = decoder.push(PROGRESS_FRAME.as_bytes());
        assert!(out.is_empty());
    }

    const PROGRESS_FRAME: &str = "data: {\"choices\":[{\"finish_reason\":null,\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":null}}],\"prompt_progress\":{\"total\":4000,\"cache\":0,\"processed\":2048,\"time_ms\":1}}\n\n";

    /// Frames delivered at virtual times, for driving the stream clocks on a
    /// paused runtime.
    fn scripted(
        frames: Vec<(u64, String)>,
    ) -> impl Stream<Item = Result<Vec<u8>, std::io::Error>> + Send + Unpin + 'static {
        Box::pin(futures::stream::unfold(
            frames.into_iter(),
            |mut frames| async move {
                let (delay, frame) = frames.next()?;
                tokio::time::sleep(Duration::from_secs(delay)).await;
                Some((Ok(frame.into_bytes()), frames))
            },
        ))
    }

    /// Ninety seconds of prompt processing, reported every 45, then the answer.
    /// The flat 60 s first-token wait failed this turn; progress frames are
    /// liveness, so it must now succeed on both stream paths.
    fn slow_prefill() -> Vec<(u64, String)> {
        let answer = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Answer\"},\"finish_reason\":null}]}\n\n";
        let finish = "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
        vec![
            (45, PROGRESS_FRAME.to_string()),
            (45, PROGRESS_FRAME.to_string()),
            (10, answer.to_string()),
            (1, finish.to_string()),
        ]
    }

    #[tokio::test(start_paused = true)]
    async fn typed_stream_survives_ninety_seconds_of_prompt_progress() {
        let prefill = prefill_allowance(&[json!({"role":"user","content":"hi"})]);
        assert_eq!(
            prefill, FIRST_TOKEN_TIMEOUT,
            "a short prompt gets the floor"
        );
        let response =
            SidecarLLMClient::drain_typed_stream(scripted(slow_prefill()), prefill, &|_| Ok(()))
                .await
                .unwrap();
        assert_eq!(response.text, "Answer");
    }

    #[tokio::test(start_paused = true)]
    async fn legacy_stream_survives_ninety_seconds_of_prompt_progress() {
        let mut stream =
            SidecarLLMClient::parse_sse_stream(scripted(slow_prefill()), FIRST_TOKEN_TIMEOUT);
        let mut text = String::new();
        while let Some(piece) = stream.next().await {
            text.push_str(&piece.unwrap());
        }
        assert_eq!(text, "Answer");
    }

    /// Once the model is generating, silence is a stall again.
    #[tokio::test(start_paused = true)]
    async fn silence_after_generation_starts_is_still_a_stall() {
        let answer = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"A\"},\"finish_reason\":null}]}\n\n";
        let frames = vec![(1, answer.to_string()), (45, answer.to_string())];
        let result = SidecarLLMClient::drain_typed_stream(
            scripted(frames),
            FIRST_TOKEN_TIMEOUT,
            &|_| Ok(()),
        )
        .await;
        assert!(result.is_err());
    }

    /// A long prompt with no progress frames gets one batch at the floor rate.
    #[test]
    fn a_long_prompt_gets_a_batch_of_prefill_on_top_of_the_floor() {
        let long = json!({"role":"user","content":"x".repeat(40_000)});
        assert_eq!(
            prefill_allowance(&[long]),
            FIRST_TOKEN_TIMEOUT
                + Duration::from_secs((PREFILL_BATCH_TOKENS / PREFILL_FLOOR_TOKENS_PER_SEC) as u64)
        );
    }

    #[test]
    fn a_stream_asks_for_prompt_progress() {
        let stream = SidecarLLMClient::build_request(
            &GenerationConfig::default(),
            Vec::new(),
            true,
            RequestTuning::default(),
        );
        let json = serde_json::to_value(&stream).unwrap();
        assert_eq!(json["return_progress"], true);
        let once = SidecarLLMClient::build_request(
            &GenerationConfig::default(),
            Vec::new(),
            false,
            RequestTuning::default(),
        );
        assert!(serde_json::to_value(&once)
            .unwrap()
            .get("return_progress")
            .is_none());
    }

    /// Memory extraction asks for 8192 tokens under an 8-minute budget; the
    /// 120 s cap cut it off with most of that budget left.
    #[test]
    fn a_non_stream_call_gets_the_callers_whole_budget() {
        let eight_minutes = Duration::from_secs(8 * 60);
        assert_eq!(non_stream_timeout(Some(eight_minutes)), eight_minutes);
        assert_eq!(
            non_stream_timeout(Some(Duration::from_secs(20))),
            Duration::from_secs(20)
        );
        assert_eq!(non_stream_timeout(None), NON_STREAM_TIMEOUT);
    }

    /// A chat-template 500 quotes the prompt; none of it may reach the error.
    #[tokio::test]
    async fn a_server_error_body_is_not_copied_into_the_error() {
        use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500).set_body_string(
                r#"{"error":{"code":500,"message":"template failed near: my secret diary entry"}}"#,
            ))
            .mount(&server)
            .await;
        let response = reqwest::Client::new()
            .post(format!("{}/v1/chat/completions", server.uri()))
            .send()
            .await
            .unwrap();
        let err = redacted_status(response).await.unwrap_err().to_string();
        assert!(err.contains("500"), "{err}");
        assert!(!err.contains("secret diary"), "{err}");
    }

    #[test]
    fn deserialize_chunk_extracts_content() {
        let payload = r#"{"choices":[{"delta":{"content":"hello"}}]}"#;
        let chunk: ChatCompletionChunk = serde_json::from_str(payload).expect("parse");
        assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("hello"));
    }

    #[test]
    fn deserialize_response_extracts_message() {
        let payload = r#"{
            "choices":[{
                "message":{"role":"assistant","content":"hi there"}
            }]
        }"#;
        let resp: ChatCompletionResponse = serde_json::from_str(payload).expect("parse");
        assert_eq!(resp.choices[0].message.content, "hi there");
    }
}
