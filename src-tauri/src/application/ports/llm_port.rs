//! LLM interaction port for the application layer.
//!
//! This port defines the interface for Large Language Model interactions.
//! Infrastructure implementations can use Ollama, OpenAI, Anthropic, or other
//! LLM providers.
//!
//! # Purpose
//!
//! - Abstracts LLM provider implementation details
//! - Allows switching between local and remote LLMs
//! - Enables testing with mock responses
//! - Supports both synchronous and streaming generation
//!
//! # Infrastructure Implementations
//!
//! - `OllamaAdapter` - Local Ollama models (llama2, mistral, etc.)
//! - `AnthropicAdapter` - Claude API (Sonnet, Opus)
//! - `OpenAIAdapter` - GPT models via OpenAI API
//! - `MockLLMAdapter` - Test implementation returning fixed responses
//!
//! # Example Usage
//!
//! ```rust
//! use crate::application::ports::LLMPort;
//!
//! async fn answer_question(
//!     llm: &impl LLMPort,
//!     question: &str,
//!     context: &[String],
//! ) -> Result<String> {
//!     llm.generate(question, context).await
//! }
//! ```

use crate::shared::error::Result;
use async_trait::async_trait;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Lazily resolves an optional model without exposing the application's container.
pub type OptionalLlmLoader = std::sync::Arc<
    dyn Fn() -> futures::future::BoxFuture<'static, Result<Option<std::sync::Arc<dyn LLMPort>>>>
        + Send
        + Sync,
>;

/// Wall-clock allowance for one completion, across retries, when the caller sets none.
pub const DEFAULT_COMPLETION_TIME_BUDGET: Duration = Duration::from_secs(10 * 60);

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

/// A tool call returned by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    #[serde(default)]
    pub id: Option<String>,
    /// Function name being called
    pub name: String,
    /// Arguments as a JSON object
    pub arguments: serde_json::Value,
}

/// Streaming chunk that can be either content or a tool call.
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// Text content chunk
    Content(String),
    /// Tool call request from the model
    ToolCalls(Vec<ToolCall>),
    /// Stream is complete
    Done,
}

/// Port for LLM generation operations.
///
/// Implementations must:
/// - Support both synchronous and streaming generation
/// - Handle context window limits gracefully
/// - Provide model identification
/// - Be thread-safe (`Send + Sync`)
#[async_trait]
pub trait LLMPort: Send + Sync {
    fn supports_typed_completions(&self) -> bool {
        false
    }

    /// Typed completion preserves native tool IDs/results, usage, and finish state.
    async fn complete(&self, _request: &CompletionRequest) -> Result<CompletionResponse> {
        Err(crate::shared::error::AppError::InvalidConfig(
            "This provider does not support typed completions".into(),
        ))
    }

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

    /// Generate a response to a prompt with optional context.
    ///
    /// Performs synchronous generation, waiting for the complete response
    /// before returning. For streaming responses, use `generate_streaming`.
    ///
    /// # Arguments
    ///
    /// * `prompt` - The user's question or instruction
    /// * `context` - Retrieved context chunks to condition the response (e.g., from RAG)
    ///
    /// # Returns
    ///
    /// The complete generated text response.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if prompt is empty or too long
    /// - `AppError::Network` if using remote API and network fails
    /// - `AppError::LLMFailed` if generation fails or times out
    /// - `AppError::RateLimitExceeded` if API rate limit is hit
    ///
    /// # Example
    ///
    /// ```rust
    /// let context = vec![
    ///     "Rust is a systems programming language.".into(),
    ///     "Rust has a strong type system.".into(),
    /// ];
    /// let response = llm.generate("What is Rust?", &context, None).await?;
    /// println!("Answer: {}", response);
    /// ```
    async fn generate(
        &self,
        prompt: &str,
        context: &[String],
        images: Option<Vec<String>>,
    ) -> Result<String>;

    /// Generate a response with streaming output.
    ///
    /// Returns a stream of text chunks as they are generated. Useful for
    /// providing real-time feedback in UI applications.
    ///
    /// # Arguments
    ///
    /// * `prompt` - The user's question or instruction
    /// * `context` - Retrieved context chunks to condition the response
    /// * `images` - Optional list of base64-encoded images for multimodal models
    ///
    /// # Returns
    ///
    /// A stream of text chunks. Chunks should be concatenated to form the
    /// complete response.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if prompt is empty or too long
    /// - `AppError::Network` if using remote API and network fails
    /// - `AppError::LLMFailed` if generation fails or stream is interrupted
    ///
    /// # Example
    ///
    /// ```rust
    /// use futures::StreamExt;
    ///
    /// let mut stream = llm.generate_streaming("Explain Rust", &context, None).await?;
    /// while let Some(chunk) = stream.next().await {
    ///     print!("{}", chunk);
    /// }
    /// ```
    async fn generate_streaming(
        &self,
        prompt: &str,
        context: &[String],
        images: Option<Vec<String>>,
    ) -> Result<Box<dyn Stream<Item = Result<String>> + Send + Unpin + '_>>;

    /// Get the name/identifier of the underlying model.
    ///
    /// Used for logging, debugging, and model-specific configuration.
    ///
    /// # Returns
    ///
    /// A string identifying the model (e.g., "llama2:13b", "claude-3-sonnet", "gpt-4")
    ///
    /// # Example
    ///
    /// ```rust
    /// let model = llm.model_name();
    /// println!("Using model: {}", model);
    /// ```
    fn model_name(&self) -> &str;

    /// Get the maximum context window size in tokens.
    ///
    /// This is the total number of tokens (prompt + context + response) that
    /// the model can handle. Implementations should enforce this limit.
    ///
    /// # Returns
    ///
    /// Maximum context size in tokens (e.g., 4096, 8192, 100000)
    ///
    /// # Example
    ///
    /// ```rust
    /// let max_tokens = llm.max_context_tokens();
    /// if total_tokens > max_tokens {
    ///     // Truncate context
    /// }
    /// ```
    fn max_context_tokens(&self) -> usize;

    /// Estimate the number of tokens in a text string.
    ///
    /// Used for context window management. This is an approximation
    /// and may not match exact tokenization.
    ///
    /// # Arguments
    ///
    /// * `text` - The text to count tokens for
    ///
    /// # Returns
    ///
    /// Estimated token count
    ///
    /// # Example
    ///
    /// ```rust
    /// let token_count = llm.count_tokens("Hello world");
    /// assert!(token_count > 0);
    /// ```
    fn count_tokens(&self, text: &str) -> usize;

    /// Check if the LLM service is ready and operational.
    ///
    /// This method is used for health checks and system diagnostics.
    ///
    /// # Returns
    ///
    /// - `Ok(true)` if the service is ready to generate responses
    /// - `Ok(false)` if the service is not ready (model not loaded, API unavailable, etc.)
    /// - `Err` if the health check itself fails
    ///
    /// # Example
    ///
    /// ```rust
    /// if llm.is_ready().await? {
    ///     println!("LLM service is operational");
    /// } else {
    ///     println!("LLM service is not ready");
    /// }
    /// ```
    async fn is_ready(&self) -> Result<bool>;

    /// Check if this LLM backend supports native tool/function calling.
    ///
    /// When `true`, the backend can process `tools` in `generate_streaming_with_tools`
    /// and may yield `StreamChunk::ToolCalls`. When `false`, tools are ignored and
    /// the conversation chat should rely exclusively on prompt-injected RAG context.
    ///
    /// # Returns
    ///
    /// `true` if the backend supports tool calling, `false` otherwise.
    ///
    /// Default: `false` (conservative -- local models and mocks don't support tools).
    fn supports_tool_calling(&self) -> bool {
        false
    }

    /// Get the name of the LLM provider (e.g., "ollama", "local", "mock").
    ///
    /// Used for logging, diagnostics, and provider-specific behavior.
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

    /// Generate a streaming response with optional tool definitions.
    ///
    /// When tools are provided, the stream may yield `StreamChunk::ToolCalls`
    /// instead of (or in addition to) `StreamChunk::Content`. The caller is
    /// responsible for executing tool calls and feeding results back.
    ///
    /// # Arguments
    ///
    /// * `prompt` - The user's question or instruction
    /// * `context` - Retrieved context chunks (formatted as "Role: content")
    /// * `images` - Optional base64-encoded images
    /// * `tools` - Optional tool definitions for function calling
    ///
    /// # Returns
    ///
    /// A stream of `StreamChunk` items (content text, tool calls, or done signal).
    ///
    /// Default implementation delegates to `generate_streaming` (ignoring tools).
    async fn generate_streaming_with_tools(
        &self,
        prompt: &str,
        context: &[String],
        images: Option<Vec<String>>,
        tools: Option<&[ToolDefinition]>,
    ) -> Result<Box<dyn Stream<Item = Result<StreamChunk>> + Send + Unpin + '_>> {
        // Default: ignore tools and wrap content in StreamChunk::Content
        let _ = tools;
        let inner = self.generate_streaming(prompt, context, images).await?;
        use futures::StreamExt;
        let mapped = inner.map(|r| r.map(StreamChunk::Content));
        Ok(Box::new(Box::pin(mapped)))
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
}

impl SamplingOverride {
    /// Greedy decoding: the same prompt returns the same answer every time.
    pub fn deterministic() -> Self {
        Self {
            temperature: Some(0.0),
            top_p: Some(1.0),
            top_k: Some(1),
        }
    }

    /// Whether this override asks for anything at all.
    pub fn is_empty(&self) -> bool {
        self.temperature.is_none() && self.top_p.is_none() && self.top_k.is_none()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub input: Vec<CompletionInput>,
    pub tools: Vec<ToolDefinition>,
    pub json_schema: Option<serde_json::Value>,
    pub reasoning_effort: Option<String>,
    /// Ask providers that support it for displayable reasoning. This controls
    /// summaries or explicit thinking text only; opaque provider state used for
    /// replay remains separate in `CompletionResponse::provider_output`.
    #[serde(default)]
    pub include_reasoning: bool,
    /// Sampling for this request. `None` keeps the provider's configuration.
    #[serde(default)]
    pub sampling: Option<SamplingOverride>,
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
    /// Opaque native output items for replaying provider-specific reasoning state.
    pub provider_output: serde_json::Value,
    /// The first generated token's top alternatives as `(token, logprob)`,
    /// most likely first. Present only when the request asked for them and the
    /// provider reports them.
    #[serde(default)]
    pub first_token_logprobs: Option<Vec<(String, f32)>>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn interactive_completions_can_opt_out_without_changing_default_limits() {
        let request = CompletionRequest {
            time_budget: Some(Duration::from_secs(1)),
            ..Default::default()
        };
        assert!(request
            .within_time_budget(tokio::time::sleep(Duration::from_secs(2)))
            .await
            .is_err());
        let request = CompletionRequest {
            no_time_limit: true,
            ..request
        };
        request
            .within_time_budget(tokio::time::sleep(Duration::from_secs(3600)))
            .await
            .unwrap();
        assert_eq!(
            CompletionRequest::default().wall_clock_budget(),
            Some(DEFAULT_COMPLETION_TIME_BUDGET)
        );
    }

    #[test]
    fn a_request_output_cap_can_tighten_the_providers_limit_but_never_raise_it() {
        let provider_limit = 4096;
        let uncapped = CompletionRequest::default();
        assert_eq!(
            uncapped.effective_max_output_tokens(provider_limit),
            provider_limit,
            "no request cap leaves the configured limit alone"
        );

        let tighter = CompletionRequest {
            max_output_tokens: Some(512),
            ..Default::default()
        };
        assert_eq!(tighter.effective_max_output_tokens(provider_limit), 512);

        // A budget that reserved more room than the user configured must not
        // silently grant it: the configured ceiling is the user's decision.
        let looser = CompletionRequest {
            max_output_tokens: Some(100_000),
            ..Default::default()
        };
        assert_eq!(
            looser.effective_max_output_tokens(provider_limit),
            provider_limit
        );

        // Zero is meaningless as a generation cap and would produce an empty
        // response rather than an error, so it is treated as "unset".
        let zero = CompletionRequest {
            max_output_tokens: Some(0),
            ..Default::default()
        };
        assert_eq!(
            zero.effective_max_output_tokens(provider_limit),
            provider_limit
        );
    }

    #[test]
    fn first_token_alternatives_are_read_from_the_openai_logprobs_shape() {
        let logprobs = serde_json::json!({"content":[
            {"token":"supported","logprob":-0.1,"top_logprobs":[
                {"token":"uns","logprob":-2.5},
                {"token":"supported","logprob":-0.1}
            ]},
            {"token":"!","logprob":-3.0}
        ]});
        let alternatives = first_token_logprobs(&logprobs).expect("first token present");
        assert_eq!(alternatives[0], ("supported".to_string(), -0.1));
        assert_eq!(alternatives[1], ("uns".to_string(), -2.5));

        // No alternatives offered: the chosen token alone still counts.
        let bare = serde_json::json!({"content":[{"token":"contr","logprob":-0.3}]});
        assert_eq!(
            first_token_logprobs(&bare),
            Some(vec![("contr".to_string(), -0.3)])
        );
        assert_eq!(
            first_token_logprobs(&serde_json::json!({"content":[]})),
            None
        );
        assert_eq!(first_token_logprobs(&serde_json::Value::Null), None);
    }
}
