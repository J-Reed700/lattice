//! LLM client trait abstraction for polymorphic LLM backends.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use tokio_stream::Stream;

use crate::features::llm::engine::types::LLMError;

/// Configuration for text generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationConfig {
    /// Temperature for sampling (0.0 = deterministic, 2.0 = very random).
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// Top-p (nucleus) sampling.
    #[serde(default = "default_top_p")]
    pub top_p: f32,

    /// Top-k sampling.
    #[serde(default = "default_top_k")]
    pub top_k: i32,

    /// Maximum number of tokens to generate.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,

    /// Penalty for repeating tokens.
    #[serde(default = "default_repeat_penalty")]
    pub repeat_penalty: f32,
}

impl Default for GenerationConfig {
    fn default() -> Self {
        Self {
            temperature: default_temperature(),
            top_p: default_top_p(),
            top_k: default_top_k(),
            max_tokens: default_max_tokens(),
            repeat_penalty: default_repeat_penalty(),
        }
    }
}

fn default_temperature() -> f32 {
    0.7
}
fn default_top_p() -> f32 {
    0.9
}
fn default_top_k() -> i32 {
    40
}
fn default_max_tokens() -> usize {
    131072
}
fn default_repeat_penalty() -> f32 {
    1.1
}

/// Shared chat message type for all LLM clients
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Role: "system", "user", or "assistant"
    pub role: String,
    /// Message content
    pub content: String,
}

/// Trait for LLM text generation clients.
///
/// This trait provides a unified interface for different LLM backends
/// (local inference, Ollama, Anthropic, etc.), enabling dependency injection
/// and testing.
#[async_trait]
pub trait LLMClient: Send + Sync {
    /// Generate text completion (non-streaming).
    ///
    /// # Arguments
    /// * `prompt` - The user prompt/question
    /// * `system` - Optional system message to set context
    /// * `images` - Optional list of base64-encoded images
    ///
    /// # Returns
    /// Generated text response.
    async fn generate(
        &self,
        prompt: &str,
        system: Option<&str>,
        images: Option<Vec<String>>,
    ) -> Result<String, LLMError>;

    /// Generate text with streaming responses.
    ///
    /// # Arguments
    /// * `prompt` - The user prompt/question
    /// * `system` - Optional system message to set context
    /// * `images` - Optional list of base64-encoded images
    ///
    /// # Returns
    /// A stream of text chunks as they are generated.
    async fn generate_stream(
        &self,
        prompt: &str,
        system: Option<&str>,
        images: Option<Vec<String>>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LLMError>> + Send + '_>>, LLMError>;

    /// Check if the client/model is available and healthy.
    ///
    /// # Returns
    /// `true` if the client can generate text, `false` otherwise.
    async fn health_check(&self) -> bool;

    /// Get the model name or identifier.
    fn model_name(&self) -> &str;

    /// Get the generation configuration.
    fn generation_config(&self) -> &GenerationConfig;

    /// Get a mutable reference to the generation configuration.
    fn generation_config_mut(&mut self) -> &mut GenerationConfig;

    /// Generate text using chat API with message history.
    ///
    /// # Arguments
    /// * `messages` - Conversation messages (system, user, assistant)
    ///
    /// # Returns
    /// Generated text response.
    ///
    /// # Default Implementation
    /// Returns error indicating chat API not supported.
    async fn generate_chat(&self, _messages: Vec<ChatMessage>) -> Result<String, LLMError> {
        Err(LLMError::InvalidConfig(
            "Chat API not supported by this client".into(),
        ))
    }

    /// Generate text using chat API with streaming.
    ///
    /// # Arguments
    /// * `messages` - Conversation messages (system, user, assistant)
    ///
    /// # Returns
    /// A stream of text chunks as they are generated.
    ///
    /// # Default Implementation
    /// Returns error indicating chat streaming not supported.
    async fn generate_chat_stream(
        &self,
        _messages: Vec<ChatMessage>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LLMError>> + Send + '_>>, LLMError> {
        Err(LLMError::InvalidConfig(
            "Chat streaming not supported by this client".into(),
        ))
    }

    /// Check if this client supports the chat API.
    ///
    /// # Returns
    /// `true` if chat API is supported, `false` otherwise.
    fn supports_chat(&self) -> bool {
        false
    }
}
