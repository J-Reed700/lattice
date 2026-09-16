//! No-operation LLM client for graceful degradation.
//!
//! This module provides a fallback LLM client that returns errors instead of
//! panicking when the real LLM service is unavailable.
//!
//! # Purpose
//!
//! - Allows the application to boot even when Ollama is not running
//! - Provides helpful error messages explaining why LLM features are unavailable
//! - Enables graceful degradation of LLM-dependent features
//! - Prevents application crashes due to missing LLM services
//!
//! # Use Cases
//!
//! - Ollama service not installed or not running
//! - Network connectivity issues preventing LLM access
//! - Model loading failures during initialization
//! - Testing scenarios where LLM is intentionally disabled
//!
//! # Example
//!
//! ```rust
//! use crate::features::llm::engine::NoOpLLMClient;
//! use crate::features::llm::engine::traits::LLMClient;
//!
//! // Create NoOp client with error message
//! let client = NoOpLLMClient::new(
//!     "Ollama service not running. Please start Ollama to enable AI features.".to_string()
//! );
//!
//! // All operations return ClientUnavailable error
//! let result = client.generate("test", None).await;
//! assert!(result.is_err());
//!
//! // health_check() returns false
//! assert_eq!(client.health_check().await, false);
//! ```

use async_trait::async_trait;
use std::pin::Pin;
use tokio_stream::Stream;

use crate::features::llm::engine::traits::{ChatMessage, GenerationConfig, LLMClient};
use crate::features::llm::engine::types::LLMError;

/// No-operation LLM client that returns errors instead of panicking.
///
/// This client is used as a fallback when the real LLM service is unavailable.
/// It allows the application to boot with degraded LLM functionality rather than
/// crashing.
///
/// All generation methods return `ClientUnavailable` errors with a helpful
/// message explaining why the LLM is unavailable (stored in `error_message`).
///
/// # Fields
///
/// * `error_message` - Human-readable explanation of why LLM is unavailable
///   (e.g., "Ollama not running", "Model failed to load", etc.)
/// * `config` - Default generation configuration (unused but required by trait)
///
/// # Example
///
/// ```rust
/// let client = NoOpLLMClient::new(
///     "Ollama service is not running. Start Ollama to enable AI features.".to_string()
/// );
///
/// // Client is never ready
/// assert_eq!(client.health_check().await, false);
///
/// // All operations fail with helpful error
/// let result = client.generate("question", None).await;
/// assert!(matches!(result, Err(LLMError::ClientUnavailable(_))));
/// ```
#[derive(Debug, Clone)]
pub struct NoOpLLMClient {
    /// Human-readable error message explaining why LLM is unavailable
    error_message: String,
    /// Default generation configuration (unused)
    config: GenerationConfig,
}

impl NoOpLLMClient {
    /// Create a new NoOp LLM client with a custom error message.
    ///
    /// # Arguments
    ///
    /// * `error_message` - Human-readable explanation of why LLM is unavailable.
    ///   This message will be returned to users when they attempt to use LLM features.
    ///
    /// # Returns
    ///
    /// A new NoOpLLMClient that returns the provided error message for all operations.
    ///
    /// # Example
    ///
    /// ```rust
    /// let client = NoOpLLMClient::new(
    ///     "Ollama not installed. Visit https://ollama.ai to install.".to_string()
    /// );
    /// ```
    pub fn new(error_message: String) -> Self {
        Self {
            error_message,
            config: GenerationConfig::default(),
        }
    }
}

#[async_trait]
impl LLMClient for NoOpLLMClient {
    /// Returns error indicating LLM service is not available.
    ///
    /// # Returns
    ///
    /// `Err(LLMError::ClientUnavailable)` with the error message provided
    /// during construction.
    async fn generate(
        &self,
        _prompt: &str,
        _system: Option<&str>,
        _images: Option<Vec<String>>,
    ) -> Result<String, LLMError> {
        Err(LLMError::ClientUnavailable(self.error_message.clone()))
    }

    /// Returns error indicating LLM service is not available.
    ///
    /// # Returns
    ///
    /// `Err(LLMError::ClientUnavailable)` with the error message provided
    /// during construction.
    async fn generate_stream(
        &self,
        _prompt: &str,
        _system: Option<&str>,
        _images: Option<Vec<String>>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LLMError>> + Send + '_>>, LLMError> {
        Err(LLMError::ClientUnavailable(self.error_message.clone()))
    }

    /// Always returns false indicating the LLM service is not ready.
    ///
    /// This is used for health checks and allows the application to detect
    /// that LLM features are unavailable without panicking.
    ///
    /// # Returns
    ///
    /// `false` indicating the service is not operational
    async fn health_check(&self) -> bool {
        false
    }

    /// Returns "noop" as the model name identifier.
    ///
    /// # Returns
    ///
    /// The string "noop" indicating this is a no-operation client.
    fn model_name(&self) -> &str {
        "noop"
    }

    /// Returns the generation configuration.
    ///
    /// # Returns
    ///
    /// Default configuration (unused for NoOp client)
    fn generation_config(&self) -> &GenerationConfig {
        &self.config
    }

    /// Returns mutable reference to generation configuration.
    ///
    /// # Returns
    ///
    /// Default configuration (unused for NoOp client)
    fn generation_config_mut(&mut self) -> &mut GenerationConfig {
        &mut self.config
    }

    /// Chat API not supported by NoOp client.
    ///
    /// # Returns
    ///
    /// `Err(LLMError::ClientUnavailable)` with the error message
    async fn generate_chat(&self, _messages: Vec<ChatMessage>) -> Result<String, LLMError> {
        Err(LLMError::ClientUnavailable(self.error_message.clone()))
    }

    /// Chat streaming not supported by NoOp client.
    ///
    /// # Returns
    ///
    /// `Err(LLMError::ClientUnavailable)` with the error message
    async fn generate_chat_stream(
        &self,
        _messages: Vec<ChatMessage>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LLMError>> + Send + '_>>, LLMError> {
        Err(LLMError::ClientUnavailable(self.error_message.clone()))
    }

    /// NoOp client does not support chat API.
    ///
    /// # Returns
    ///
    /// `false`
    fn supports_chat(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_noop_client_generate_returns_error() {
        let client = NoOpLLMClient::new("Test error message".to_string());

        let result = client.generate("test prompt", None, None).await;

        assert!(result.is_err());
        match result {
            Err(LLMError::ClientUnavailable(msg)) => {
                assert_eq!(msg, "Test error message");
            }
            _ => panic!("Expected ClientUnavailable error"),
        }
    }

    #[tokio::test]
    async fn test_noop_client_generate_stream_returns_error() {
        let client = NoOpLLMClient::new("Streaming unavailable".to_string());

        let result = client.generate_stream("test", None, None).await;

        assert!(result.is_err());
        match result {
            Err(LLMError::ClientUnavailable(msg)) => {
                assert_eq!(msg, "Streaming unavailable");
            }
            _ => panic!("Expected ClientUnavailable error"),
        }
    }

    #[test]
    fn test_noop_client_model_name() {
        let client = NoOpLLMClient::new("error".to_string());
        assert_eq!(client.model_name(), "noop");
    }

    #[test]
    fn test_noop_client_generation_config() {
        let client = NoOpLLMClient::new("error".to_string());
        let config = client.generation_config();
        assert_eq!(config.temperature, 0.7);
    }

    #[tokio::test]
    async fn test_noop_client_health_check() {
        let client = NoOpLLMClient::new("error".to_string());
        let healthy = client.health_check().await;
        assert!(!healthy);
    }

    #[tokio::test]
    async fn test_noop_client_with_helpful_message() {
        let client = NoOpLLMClient::new(
            "Ollama service not running. Start Ollama to enable AI features.".to_string(),
        );

        let result = client.generate("test", None, None).await;

        assert!(result.is_err());
        match result {
            Err(LLMError::ClientUnavailable(msg)) => {
                assert!(msg.contains("Ollama"));
                assert!(msg.contains("AI features"));
            }
            _ => panic!("Expected ClientUnavailable error with helpful message"),
        }
    }

    #[tokio::test]
    async fn test_noop_client_generate_chat_returns_error() {
        let client = NoOpLLMClient::new("Chat unavailable".to_string());

        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "test".to_string(),
        }];

        let result = client.generate_chat(messages).await;

        assert!(result.is_err());
        match result {
            Err(LLMError::ClientUnavailable(msg)) => {
                assert_eq!(msg, "Chat unavailable");
            }
            _ => panic!("Expected ClientUnavailable error"),
        }
    }

    #[test]
    fn test_noop_client_supports_chat() {
        let client = NoOpLLMClient::new("error".to_string());
        assert!(!client.supports_chat());
    }
}
