//! Type definitions for Ollama API.
//!
//! Provides strongly-typed Rust equivalents of the Ollama API request and response structures.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Request for generating text with Ollama.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaGenerateRequest {
    /// Model name to use for generation
    pub model: String,

    /// Prompt text to generate from
    pub prompt: String,

    /// System prompt to set context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,

    /// Prompt template override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,

    /// Context from previous request
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<Vec<i32>>,

    /// Enable streaming response
    #[serde(default)]
    pub stream: bool,

    /// Disable prompt formatting
    #[serde(default)]
    pub raw: bool,

    /// Response format (e.g., "json")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,

    /// Model-specific options
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<HashMap<String, serde_json::Value>>,

    /// Duration to keep model loaded
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_alive: Option<String>,

    /// Base64-encoded images (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
}

impl OllamaGenerateRequest {
    /// Create a new request with just model and prompt.
    pub fn new(model: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            prompt: prompt.into(),
            system: None,
            template: None,
            context: None,
            stream: false,
            raw: false,
            format: None,
            options: None,
            keep_alive: None,
            images: None,
        }
    }

    /// Set images.
    pub fn with_images(mut self, images: Vec<String>) -> Self {
        self.images = Some(images);
        self
    }

    /// Set system prompt.
    pub fn with_system(mut self, system: impl Into<String>) -> Self {
        self.system = Some(system.into());
        self
    }

    /// Enable streaming.
    pub fn with_stream(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }

    /// Set response format.
    pub fn with_format(mut self, format: impl Into<String>) -> Self {
        self.format = Some(format.into());
        self
    }
}

/// Response from Ollama generation (non-streaming).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaGenerateResponse {
    /// Model used for generation
    pub model: String,

    /// Timestamp of generation
    pub created_at: String,

    /// Generated text response
    pub response: String,

    /// Whether generation is complete
    pub done: bool,

    /// Context for continuation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<Vec<i32>>,

    /// Total duration in nanoseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_duration: Option<i64>,

    /// Model load duration in nanoseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_duration: Option<i64>,

    /// Number of tokens in prompt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_eval_count: Option<i32>,

    /// Prompt evaluation duration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_eval_duration: Option<i64>,

    /// Number of tokens generated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eval_count: Option<i32>,

    /// Generation duration in nanoseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eval_duration: Option<i64>,
}

/// Streaming chunk response from Ollama.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaStreamResponse {
    /// Model used for generation
    pub model: String,

    /// Timestamp of chunk
    pub created_at: String,

    /// Generated text chunk
    #[serde(default)]
    pub response: String,

    /// Whether generation is complete
    pub done: bool,
}

/// Information about an Ollama model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModel {
    /// Model name
    pub name: String,

    /// Last modification timestamp
    pub modified_at: String,

    /// Model size in bytes
    pub size: i64,

    /// Model digest hash
    pub digest: String,

    /// Additional model details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<HashMap<String, serde_json::Value>>,
}

/// Response from listing available models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaListResponse {
    /// Available models
    pub models: Vec<OllamaModel>,
}

/// Error response from Ollama API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaError {
    /// Error message from Ollama
    pub error: String,
}

/// Error types for LLM operations.
#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "type", content = "message")]
pub enum LLMError {
    #[error("Model not loaded")]
    ModelNotLoaded,

    #[error("Generation failed: {0}")]
    GenerationFailed(String),

    #[error("Client unavailable: {0}")]
    ClientUnavailable(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Timeout: operation took too long")]
    Timeout,

    /// Returned by the safetensors loader when the model's on-disk size
    /// would exceed available system memory by more than a safe margin.
    /// Loading anyway would cause an OS-level OOM kill that looks like
    /// a Lattice crash to the user. The message includes the specific
    /// model + RAM numbers so the UI can render a clean warning.
    #[error("Insufficient memory: {0}")]
    InsufficientMemory(String),

    #[error("Local LLM inference is not supported in this context: {0}")]
    PlatformNotSupported(String),

    #[error(transparent)]
    #[serde(serialize_with = "serialize_io_error")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    #[serde(serialize_with = "serialize_reqwest_error")]
    Reqwest(#[from] reqwest::Error),

    #[error("Other error: {0}")]
    Other(String),
}

fn serialize_io_error<S>(error: &std::io::Error, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&error.to_string())
}

fn serialize_reqwest_error<S>(error: &reqwest::Error, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&error.to_string())
}

/// Auto-convert from anyhow::Error to LLMError
impl From<anyhow::Error> for LLMError {
    fn from(e: anyhow::Error) -> Self {
        LLMError::Other(e.to_string())
    }
}

/// Chat message for Ollama Chat API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaChatMessage {
    /// Role: "system", "user", "assistant", or "tool"
    pub role: String,
    /// Message content
    #[serde(default)]
    pub content: String,
    /// Base64-encoded images (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
    /// Tool calls made by the assistant (present when role="assistant" and model invokes tools)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OllamaToolCall>>,
    /// Name of the tool this message is a result for (present when role="tool")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
}

/// Tool definition for Ollama's function calling API.
///
/// Matches the Ollama API format:
/// ```json
/// { "type": "function", "function": { "name": "...", "description": "...", "parameters": {...} } }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaTool {
    /// Tool type (always "function")
    #[serde(rename = "type")]
    pub tool_type: String,
    /// Function definition
    pub function: OllamaToolFunction,
}

/// Function definition within an Ollama tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaToolFunction {
    /// Function name (e.g., "semantic_search")
    pub name: String,
    /// Human-readable description of what the function does
    pub description: String,
    /// JSON Schema describing the function's parameters
    pub parameters: serde_json::Value,
}

/// Tool call made by the model in a response.
///
/// Appears in `OllamaChatMessage.tool_calls` when the model decides to invoke a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaToolCall {
    /// Tool call type (usually "function", but some providers omit it)
    #[serde(rename = "type", default = "default_tool_call_type")]
    pub call_type: String,
    /// Function call details
    pub function: OllamaToolCallFunction,
}

fn default_tool_call_type() -> String {
    "function".to_string()
}

/// Function call details within an Ollama tool call response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaToolCallFunction {
    /// Function name being called
    pub name: String,
    /// Arguments as a JSON object
    pub arguments: serde_json::Value,
    /// Index of the tool call (for parallel tool calls)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
}

impl OllamaTool {
    /// Create a new tool definition from a function name, description, and JSON Schema parameters.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: serde_json::Value,
    ) -> Self {
        Self {
            tool_type: "function".to_string(),
            function: OllamaToolFunction {
                name: name.into(),
                description: description.into(),
                parameters,
            },
        }
    }
}

/// Chat API request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaChatRequest {
    /// Model name to use
    pub model: String,
    /// Conversation messages
    pub messages: Vec<OllamaChatMessage>,
    /// Enable streaming response
    #[serde(default)]
    pub stream: bool,
    /// Model-specific options
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<HashMap<String, serde_json::Value>>,
    /// Duration to keep model loaded
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_alive: Option<String>,
    /// Tool definitions for function calling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<OllamaTool>>,
}

/// Chat API response (non-streaming)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaChatResponse {
    /// Model used for generation
    pub model: String,
    /// Timestamp of generation
    pub created_at: String,
    /// Assistant's response message
    pub message: OllamaChatMessage,
    /// Whether generation is complete
    pub done: bool,
    /// Total duration in nanoseconds
    #[serde(default)]
    pub total_duration: Option<u64>,
    /// Model load duration in nanoseconds
    #[serde(default)]
    pub load_duration: Option<u64>,
    /// Number of tokens in prompt
    #[serde(default)]
    pub prompt_eval_count: Option<u32>,
    /// Number of tokens generated
    #[serde(default)]
    pub eval_count: Option<u32>,
}

/// Chat API streaming response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaChatStreamResponse {
    /// Model used for generation
    pub model: String,
    /// Timestamp of chunk
    pub created_at: String,
    /// Assistant's response message chunk
    pub message: OllamaChatMessage,
    /// Whether generation is complete
    pub done: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_request_builder() {
        let req = OllamaGenerateRequest::new("llama2", "Hello")
            .with_system("You are helpful")
            .with_stream(true);

        assert_eq!(req.model, "llama2");
        assert_eq!(req.prompt, "Hello");
        assert_eq!(req.system, Some("You are helpful".to_string()));
        assert!(req.stream);
    }

    #[test]
    fn test_generate_request_serialization() {
        let req = OllamaGenerateRequest::new("llama2", "test");
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"model\":\"llama2\""));
        assert!(json.contains("\"prompt\":\"test\""));
    }
}
