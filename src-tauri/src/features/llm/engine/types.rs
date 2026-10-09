//! LLM engine errors and the Ollama `/api/chat` wire types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

    /// The bundled `llama-server` executable cannot run on this machine
    /// (a missing or mismatched shared library, the wrong architecture,
    /// CPU instructions it lacks). Unlike a model or GPU startup failure,
    /// no model or setting change helps: the install is broken. The
    /// message is complete and user-facing.
    #[error("{0}")]
    SidecarBinaryUnusable(String),

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
    /// Explicit reasoning returned by thinking-capable Ollama models. Kept
    /// separate from public answer content, matching Ollama's wire format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
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
    /// JSON or JSON Schema constrained output (`format` in Ollama's API).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<serde_json::Value>,
    /// Per-request thinking level; omitted keeps the model's default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub think: Option<serde_json::Value>,
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
    /// Reason generation stopped, sent with the final chunk by some versions.
    #[serde(default)]
    pub done_reason: Option<String>,
    /// Token counts sent with the final chunk.
    #[serde(default)]
    pub prompt_eval_count: Option<u32>,
    #[serde(default)]
    pub eval_count: Option<u32>,
}

use crate::shared::error::AppError;

/// Auto-convert from LLMError to AppError
impl From<crate::features::llm::engine::types::LLMError> for AppError {
    fn from(e: crate::features::llm::engine::types::LLMError) -> Self {
        use crate::features::llm::engine::types::LLMError;
        match e {
            LLMError::ModelNotLoaded => AppError::ModelLoadFailed("Model not loaded".to_string()),
            LLMError::GenerationFailed(msg) => {
                AppError::Other(format!("LLM generation failed: {}", msg))
            }
            LLMError::ClientUnavailable(msg) => AppError::ServiceNotAvailable(msg),
            LLMError::Network(msg) => AppError::Network(msg),
            LLMError::InvalidConfig(msg) => AppError::InvalidConfig(msg),
            LLMError::Timeout => AppError::Other("LLM request timed out".to_string()),
            LLMError::InsufficientMemory(msg) => AppError::ModelLoadFailed(msg),
            LLMError::PlatformNotSupported(msg) => AppError::ServiceNotAvailable(msg),
            // Not `ModelLoadFailed`: nothing is wrong with the model.
            LLMError::SidecarBinaryUnusable(msg) => AppError::ServiceNotAvailable(msg),
            LLMError::Io(e) => AppError::Io {
                message: e.to_string(),
                kind: format!("{:?}", e.kind()),
            },
            LLMError::Reqwest(e) => AppError::Network(e.to_string()),
            LLMError::Other(msg) => AppError::Other(format!("LLM error: {}", msg)),
        }
    }
}
