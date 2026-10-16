//! LLM interaction port for the application layer.
//!
//! One call shape for every provider: a typed [`CompletionRequest`] in, a
//! [`CompletionResponse`] out. The request carries role-bearing messages,
//! native tool calls and results, a response schema, sampling, an output cap,
//! a time budget and the scheduling fields (priority, cancellation, cache
//! key); the response carries the answer, any displayable reasoning, tool
//! calls, usage, the finish reason and the items that replay this turn.
//!
//! Callers with only a prompt and "Role: content" context use
//! [`complete_text`](crate::application::services::completion_input::complete_text).
//!
//! # Implementations
//!
//! - `LlamaCppLlm` - llama-server, bundled or remote
//! - `OllamaClient` - Ollama's native `/api/chat`
//! - `CloudLlm` - OpenAI Responses and Anthropic Messages
//! - `ScheduledLlm` - the decorator that admits each call through its backend's scheduler

use crate::domain::models::model_ref::ModelRef;
use crate::shared::error::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Lazily resolves an optional model without exposing the application's container.
pub type OptionalLlmLoader = std::sync::Arc<
    dyn Fn() -> futures::future::BoxFuture<'static, Result<Option<std::sync::Arc<dyn LLMPort>>>>
        + Send
        + Sync,
>;

/// Wall-clock allowance for one completion, across retries, when the caller sets none.
pub const DEFAULT_COMPLETION_TIME_BUDGET: Duration = Duration::from_secs(10 * 60);

/// Characters per token assumed until a backend has reported real usage.
/// English prose tokenizes near four; code, numbers and other scripts run
/// lower, which is what calibration against reported usage corrects.
pub const DEFAULT_CHARS_PER_TOKEN: f64 = 4.0;

/// Tokens in `chars` characters at `chars_per_token`, rounded up.
pub fn tokens_for_chars(chars: usize, chars_per_token: f64) -> usize {
    if chars == 0 {
        return 0;
    }
    let ratio = if chars_per_token.is_finite() && chars_per_token > 0.0 {
        chars_per_token
    } else {
        DEFAULT_CHARS_PER_TOKEN
    };
    (chars as f64 / ratio).ceil() as usize
}

/// Estimated tokens in `text`, for code with no model port in reach. With a
/// port, use [`LLMPort::count_tokens`]: it is calibrated against the backend.
pub fn estimate_tokens(text: &str) -> usize {
    tokens_for_chars(text.chars().count(), DEFAULT_CHARS_PER_TOKEN)
}

/// Characters a `tokens` budget can safely hold: three quarters of what the
/// ratio predicts, so a character budget under-fills on densely tokenized text
/// rather than overflowing the window it was derived from.
pub fn chars_within_tokens(tokens: usize, chars_per_token: f64) -> usize {
    const UNDER_FILL: f64 = 0.75;
    let ratio = if chars_per_token.is_finite() && chars_per_token > 0.0 {
        chars_per_token
    } else {
        DEFAULT_CHARS_PER_TOKEN
    };
    (tokens as f64 * ratio * UNDER_FILL) as usize
}

/// Who is waiting on a model call. When a backend is busy, a higher priority
/// is admitted first; equal priorities are served in arrival order.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum InferencePriority {
    /// Batch generation nobody is watching token by token: lessons,
    /// assessments, practice, study material.
    #[default]
    Background,
    /// Upkeep that keeps later turns good: memory consolidation, summaries,
    /// corpus labels, chat starters.
    Maintenance,
    /// Checking claims an answer or a lesson already made.
    Verification,
    /// A person is waiting on this call: a chat turn and everything inside it.
    Interactive,
}

/// Tool definition for LLM function calling.
///
/// Provider-agnostic representation of a callable function.
/// Infrastructure adapters convert this to provider-specific formats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Function name (e.g., "semantic_search")
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// JSON Schema for parameters
    pub parameters: serde_json::Value,
}

/// Port for LLM generation.
///
/// Implementations must be thread-safe, report the window they were started
/// with, and fail a request rather than truncate it silently.
#[async_trait]
pub trait LLMPort: Send + Sync {
    /// One completion. Native tool IDs and results, usage, the finish reason
    /// and the provider's replay items all survive the round trip.
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse>;

    /// Deliver public answer text as it arrives while retaining the complete
    /// native tool response. Providers without streaming keep the default.
    async fn complete_with_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        let response = self.complete(request).await?;
        if !response.text.is_empty() {
            on_text(response.text.clone())?;
        }
        Ok(response)
    }

    /// A retry discards the previous attempt's public draft. Callers must reset
    /// their display before accepting subsequent text; tool calls remain provisional
    /// until the returned completion has been validated.
    async fn complete_with_retry_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        _on_retry: &(dyn Fn(usize) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        self.complete_with_progress(request, on_text).await
    }

    /// Deliver public answer text and provider-supplied reasoning as they
    /// arrive. The reasoning callback receives text deltas, never encrypted or
    /// otherwise opaque provider state. Implementations without a reasoning
    /// stream fall back to delivering the completed reasoning once.
    async fn complete_with_reasoning_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_reasoning: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_retry: &(dyn Fn(usize) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        let response = self
            .complete_with_retry_progress(request, on_text, on_retry)
            .await?;
        if let Some(reasoning) = response.reasoning.as_ref().filter(|text| !text.is_empty()) {
            on_reasoning(reasoning.clone())?;
        }
        Ok(response)
    }

    /// The name of the underlying model (e.g. "llama3.1:8b", "claude-sonnet"),
    /// for logs and diagnostics.
    fn model_name(&self) -> &str;

    /// The context window, in tokens, that prompt and answer share.
    fn max_context_tokens(&self) -> usize;

    /// Characters per token this backend's tokenizer averages.
    ///
    /// Starts at [`DEFAULT_CHARS_PER_TOKEN`]; a scheduled backend calibrates it
    /// against the prompt tokens the server reports.
    fn chars_per_token(&self) -> f64 {
        DEFAULT_CHARS_PER_TOKEN
    }

    /// Estimate the number of tokens in a text string.
    ///
    /// Used for context window management: `ceil(chars / chars_per_token)`.
    /// An approximation; [`Self::count_tokens_exact`] asks the backend's own
    /// tokenizer where one is reachable.
    fn count_tokens(&self, text: &str) -> usize {
        tokens_for_chars(text.chars().count(), self.chars_per_token())
    }

    /// Tokens in `text` as the backend's own tokenizer counts them, where it
    /// can be asked; otherwise the estimate. [`Self::counts_tokens_exactly`]
    /// says which.
    ///
    /// For budget decisions that are final, not for every count: it may cost
    /// a round trip to the server.
    async fn count_tokens_exact(&self, text: &str) -> Result<usize> {
        Ok(self.count_tokens(text))
    }

    /// Whether [`Self::count_tokens_exact`] reaches a real tokenizer.
    fn counts_tokens_exactly(&self) -> bool {
        false
    }

    /// Whether the service is ready to answer: `Ok(false)` when it is not
    /// (model not loaded, server unreachable), `Err` when the check itself
    /// failed.
    async fn is_ready(&self) -> Result<bool>;

    /// Whether this backend accepts `tools` and returns native tool calls.
    ///
    /// Default: `false` (conservative -- an unknown model template may render
    /// tool traffic as text).
    fn supports_tool_calling(&self) -> bool {
        false
    }

    /// The provider behind this port (e.g. "ollama", "llamacpp",
    /// "local-sidecar"), for logs and diagnostics.
    ///
    /// Default: "unknown"
    fn provider_name(&self) -> &str {
        "unknown"
    }

    /// Whether the process behind this port is still there to answer. Cheap:
    /// no network round trip. The role caches drop a port that says no, so a
    /// local server that crashed is started again on the next request instead
    /// of failing every turn until restart. Remote and in-process ports have
    /// nothing of their own to die and stay `true`.
    fn is_alive(&self) -> bool {
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CompletionInput {
    /// Opaque provider output replayed unchanged, including signed reasoning state.
    Native {
        value: serde_json::Value,
    },
    Message {
        role: String,
        content: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    ToolResult {
        id: String,
        output: String,
    },
}

/// Sampling for one request, overriding the provider's configured defaults.
///
/// Most callers want the model the user tuned. A classifier does not: a verdict
/// or a label is a decision about evidence, and sampling one from a soft
/// distribution makes the same input answerable two ways on two turns. Fields
/// left `None` keep the provider's own setting.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct SamplingOverride {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<i32>,
    /// A flat penalty on every token already in the recent window. Greedy
    /// decoding still takes the argmax of the penalised logits, so a verdict
    /// whose label word the prompt just listed would be pushed off it.
    #[serde(default)]
    pub presence_penalty: Option<f32>,
}

impl SamplingOverride {
    /// Greedy decoding: the same prompt returns the same answer every time.
    pub fn deterministic() -> Self {
        Self {
            temperature: Some(0.0),
            top_p: Some(1.0),
            top_k: Some(1),
            presence_penalty: Some(0.0),
        }
    }

    /// Whether this override asks for anything at all.
    pub fn is_empty(&self) -> bool {
        self.temperature.is_none()
            && self.top_p.is_none()
            && self.top_k.is_none()
            && self.presence_penalty.is_none()
    }
}

/// Whether a model with a switchable thinking phase thinks before it answers.
///
/// Local chat templates disagree on their own default (the official Qwen 3.5
/// 9B template thinks unless told not to, the 2B does not, Laguna does not),
/// so a request always says which it wants rather than inheriting whichever
/// default the GGUF happens to carry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingMode {
    /// Answer directly. Every request that can call tools stays here: Qwen
    /// 3.5 9B writes its tool calls inside the thinking block, where the
    /// tool-call grammar ends generation (llama.cpp #20837). A provider that
    /// grades reasoning still applies an explicit `reasoning_effort` level.
    #[default]
    Off,
    /// Think first. Only for requests that offer no callable tools.
    On,
}

/// Who decides whether a model thinks: each request, or the model itself.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingPolicy {
    /// Each request's [`ThinkingMode`], held off while tools can be called.
    #[default]
    PerRequest,
    /// Every request thinks, tool rounds and utility calls included. For a
    /// model whose output breaks with thinking off: Laguna XS 2.1 repeats one
    /// special token until the output limit.
    AlwaysOn,
    /// No request thinks.
    AlwaysOff,
}

impl ThinkingPolicy {
    /// This policy, unless it leaves the choice to each request; then `fallback`.
    pub fn or(self, fallback: Self) -> Self {
        match self {
            Self::PerRequest => fallback,
            pinned => pinned,
        }
    }

    /// The mode `request` is sent with. Per request, a round that can still
    /// call tools never thinks: Qwen 3.5 9B writes its calls inside the
    /// thinking block, where the tool-call grammar ends generation
    /// (llama.cpp #20837).
    pub fn mode_for(self, request: &CompletionRequest) -> ThinkingMode {
        match self {
            Self::AlwaysOn => ThinkingMode::On,
            Self::AlwaysOff => ThinkingMode::Off,
            Self::PerRequest
                if request.thinking == ThinkingMode::On && !request.may_call_tools() =>
            {
                ThinkingMode::On
            }
            Self::PerRequest => ThinkingMode::Off,
        }
    }
}

impl ThinkingMode {
    /// The mode for one answering round: thinking only when the user asked
    /// for it and nothing in `request` can be called this round.
    pub fn for_answer(answer_thinking: bool, request: &CompletionRequest) -> Self {
        if answer_thinking && !request.may_call_tools() {
            Self::On
        } else {
            Self::Off
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub input: Vec<CompletionInput>,
    pub tools: Vec<ToolDefinition>,
    pub json_schema: Option<serde_json::Value>,
    pub reasoning_effort: Option<String>,
    /// How the model may use the offered tools. `None` leaves the provider's
    /// default (auto). `Some(ToolChoice::None)` keeps the tools in the prompt,
    /// so the cached prefix stays valid, while forbidding calls this round.
    pub tool_choice: Option<ToolChoice>,
    /// Ask providers that support it for displayable reasoning. This controls
    /// summaries or explicit thinking text only; opaque provider state used for
    /// replay remains separate in `CompletionResponse::replay`.
    #[serde(default)]
    pub include_reasoning: bool,
    /// Sampling for this request. `None` keeps the provider's configuration.
    #[serde(default)]
    pub sampling: Option<SamplingOverride>,
    /// Whether the model thinks before answering. Off unless the caller asks:
    /// see [`ThinkingMode`] for why tool rounds never think.
    #[serde(default)]
    pub thinking: ThinkingMode,
    /// Caps generated tokens for this request, overriding the provider's own
    /// configured ceiling when it is lower.
    ///
    /// The context assembler reserves output room out of the model's window, and
    /// a reservation nothing enforces is only bookkeeping: without this the model
    /// may generate past what the budget set aside and overrun the window it was
    /// measured against. `None` keeps the provider's configured limit.
    pub max_output_tokens: Option<u32>,
    /// Caps wall-clock time across all attempts. Stalls are detected separately,
    /// so this only needs to exceed the longest legitimate generation.
    #[serde(skip)]
    pub time_budget: Option<Duration>,
    /// Interactive, cancellable workflows may opt out of elapsed-time limits.
    /// Transport errors, stall detection, and retry limits still apply.
    #[serde(skip)]
    pub no_time_limit: bool,
    /// Ask for the log-probabilities of the first generated token and its top
    /// alternatives. A classifier reads its answer's probability from these
    /// rather than trusting one sampled word. Providers that cannot report
    /// them ignore the flag and return `None`.
    #[serde(default)]
    pub want_logprobs: bool,
    /// Who is waiting on this call. The backend's scheduler admits higher
    /// priorities first and keeps a slot free for interactive work.
    #[serde(default)]
    pub priority: InferencePriority,
    /// Fires when the caller no longer wants the answer. A queued request
    /// leaves the queue and an in-flight one is aborted, both with an error.
    #[serde(skip)]
    pub cancel: Option<CancellationToken>,
    /// Requests sharing a key share a prompt prefix (one conversation, one
    /// course). A llama-server backend sends them to the slot that last served
    /// the key, so the server reuses that prefix from its KV cache.
    #[serde(default)]
    pub cache_key: Option<String>,
    /// The llama-server slot the backend's scheduler admitted this request to.
    /// Set by the scheduler on the copy it forwards; callers leave it unset.
    #[serde(skip)]
    pub assigned_slot: Option<u32>,
}

/// Tool-use policy for one request, mirroring the OpenAI `tool_choice` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolChoice {
    Auto,
    None,
    Required,
}

impl CompletionRequest {
    pub fn effective_time_budget(&self) -> Duration {
        self.time_budget.unwrap_or(DEFAULT_COMPLETION_TIME_BUDGET)
    }

    pub fn wall_clock_budget(&self) -> Option<Duration> {
        (!self.no_time_limit).then(|| self.effective_time_budget())
    }

    pub async fn within_time_budget<F: std::future::Future>(
        &self,
        work: F,
    ) -> std::result::Result<F::Output, tokio::time::error::Elapsed> {
        match self.wall_clock_budget() {
            Some(budget) => tokio::time::timeout(budget, work).await,
            None => Ok(work.await),
        }
    }

    /// The output ceiling to send, given the provider's own configured limit.
    ///
    /// The smaller of the two always wins: a request-level cap must be able to
    /// tighten the provider's default, and must never be able to raise it past
    /// what the user configured.
    pub fn effective_max_output_tokens(&self, provider_limit: u32) -> u32 {
        match self.max_output_tokens {
            Some(requested) if requested > 0 => requested.min(provider_limit),
            _ => provider_limit,
        }
    }

    /// Whether the model could answer this request with a tool call: tools
    /// are offered and the round does not forbid calling them.
    pub fn may_call_tools(&self) -> bool {
        !self.tools.is_empty() && self.tool_choice != Some(ToolChoice::None)
    }
}

impl ToolChoice {
    /// The OpenAI wire value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::None => "none",
            Self::Required => "required",
        }
    }
}

// Written by hand rather than derived so the enum's declaration stays the one
// shared with the other tracks that add it; requests serialize with it.
impl Serialize for ToolChoice {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ToolChoice {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "auto" => Ok(Self::Auto),
            "none" => Ok(Self::None),
            "required" => Ok(Self::Required),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["auto", "none", "required"],
            )),
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
impl CompletionRequest {
    /// The last user message: the prompt a scripted test double answers.
    pub fn user_text(&self) -> &str {
        self.input
            .iter()
            .rev()
            .find_map(|item| match item {
                CompletionInput::Message { role, content } if role == "user" => {
                    Some(content.as_str())
                }
                _ => None,
            })
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub text: String,
    /// Provider-supplied displayable reasoning, when available. This is a
    /// summary for providers such as OpenAI and explicit thinking text for
    /// local models that return it as part of their response.
    #[serde(default)]
    pub reasoning: Option<String>,
    pub tool_calls: Vec<CompletionInput>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub finish_reason: String,
    /// This response as input: appended to the next request, these items
    /// replay the model's turn in the provider's own shape — its text, its
    /// tool calls and any signed or opaque reasoning state — so a tool loop
    /// continues a conversation without knowing which provider it talks to.
    #[serde(default)]
    pub replay: Vec<CompletionInput>,
    /// The first generated token's top alternatives as `(token, logprob)`,
    /// most likely first. Present only when the request asked for them and the
    /// provider reports them.
    #[serde(default)]
    pub first_token_logprobs: Option<Vec<(String, f32)>>,
    /// How the provider read the prompt, when it says: the tokens it had to
    /// evaluate and the time that took, without queueing or writing. A call
    /// with a deadline sizes its next prompt from this.
    #[serde(default)]
    pub prefill: Option<PrefillTiming>,
}

/// One request's prompt reading, as the provider measured it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct PrefillTiming {
    /// Prompt tokens evaluated, not counting any reused from a cache.
    pub tokens: u64,
    /// Seconds spent evaluating them.
    pub seconds: f64,
}

impl PrefillTiming {
    /// Tokens read per second, when the measure says anything.
    pub fn tokens_per_second(&self) -> Option<f64> {
        (self.tokens > 0 && self.seconds > 0.0).then(|| self.tokens as f64 / self.seconds)
    }
}

impl CompletionResponse {
    /// An answer that stopped on its own, replayed as one assistant message.
    pub fn from_text(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            replay: vec![CompletionInput::Message {
                role: "assistant".into(),
                content: text.clone(),
            }],
            text,
            finish_reason: "stop".into(),
            ..Default::default()
        }
    }
}

/// How many alternatives to request for the first token. Enough to hold every
/// spelling of a one-word label a tokenizer might start it with.
pub const FIRST_TOKEN_TOP_LOGPROBS: u8 = 10;

/// Read the first token's alternatives from an OpenAI-shaped `logprobs` object
/// (`{"content":[{"token","logprob","top_logprobs":[{"token","logprob"}]}]}`),
/// as llama-server returns it on a choice or on a stream chunk's choice.
pub fn first_token_logprobs(logprobs: &serde_json::Value) -> Option<Vec<(String, f32)>> {
    let first = logprobs.get("content")?.as_array()?.first()?;
    let entry = |value: &serde_json::Value| -> Option<(String, f32)> {
        let token = value.get("token")?.as_str()?.to_string();
        let logprob = value.get("logprob")?.as_f64()? as f32;
        logprob.is_finite().then_some((token, logprob))
    };
    let mut alternatives: Vec<(String, f32)> = first
        .get("top_logprobs")
        .and_then(serde_json::Value::as_array)
        .map(|top| top.iter().filter_map(entry).collect())
        .unwrap_or_default();
    if alternatives.is_empty() {
        alternatives.push(entry(first)?);
    }
    alternatives.sort_by(|a, b| b.1.total_cmp(&a.1));
    Some(alternatives)
}

/// The work a model is asked for. A role is not part of a loaded model's
/// identity: two roles that resolve to the same model share one instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ModelRole {
    Chat,
    Utility,
    Router,
    Learning,
    Maintenance,
}

/// Why a turn or job runs on the model it runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ModelOrigin {
    /// The chat model set in Settings.
    Default,
    /// The chat's own choice.
    ChatOverride,
    /// This request only; nothing is persisted.
    RequestOverride,
    /// The Learning model set in Settings.
    LearningRole,
}

/// Whether the work needs the model to call tools.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToolSupport {
    Required,
    #[default]
    NotNeeded,
}

/// What the work needs from the model it is given.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModelRequirements {
    /// The smallest window this work can run in, measured (never a constant). `None` = any.
    pub min_context_tokens: Option<usize>,
    pub tools: ToolSupport,
}

/// A model and the reason it was chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedModel {
    pub model: ModelRef,
    pub origin: ModelOrigin,
}

/// A loaded model held for one turn or job. The holder keeps `llm` until the
/// work ends, whatever is chosen in the meantime.
pub struct PinnedModel {
    pub llm: std::sync::Arc<dyn LLMPort>,
    pub resolved: ResolvedModel,
    /// The loaded instance serving `llm`, shared by every holder of it.
    pub instance_id: u64,
    /// Why the instance runs degraded (on CPU), when it does.
    pub degraded: Option<String>,
}

#[cfg(test)]
mod tests;
