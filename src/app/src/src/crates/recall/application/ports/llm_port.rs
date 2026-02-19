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

use crate::shared::result::Result;
use async_trait::async_trait;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};

// ============================================================================
// Tool/Function Calling Types (Application Layer - Provider-Agnostic)
// ============================================================================

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
