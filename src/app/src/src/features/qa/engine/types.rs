/// Q&A Module Types
///
/// Data structures and error types for the Q&A engine.
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Q&A engine errors
#[derive(Error, Debug)]
pub enum QAError {
    #[error("Ollama is unavailable: {0}")]
    OllamaUnavailable(String),

    #[error("Search failed: {0}")]
    SearchFailed(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Context too large: {0}")]
    ContextTooLarge(String),

    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Internal error: {0}")]
    InternalError(String),
}

/// Convert QAError to String for Tauri commands
impl From<QAError> for String {
    fn from(error: QAError) -> Self {
        error.to_string()
    }
}

/// Auto-convert from LLMError to QAError
impl From<crate::infrastructure::llm::types::LLMError> for QAError {
    fn from(e: crate::infrastructure::llm::types::LLMError) -> Self {
        QAError::OllamaUnavailable(e.to_string())
    }
}

/// Reference to a source document used in generating an answer
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceReference {
    /// Path to the source document
    pub file_path: String,

    /// Relevance score (0.0 to 1.0)
    pub score: f32,

    /// Relevant text excerpt
    pub snippet: String,
}

/// Stream chunk types
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum StreamChunk {
    /// Answer token chunk
    Token { content: String },

    /// Source references (sent after streaming completes)
    Sources { sources: Vec<SourceReference> },

    /// Streaming completed
    Done,

    /// Error occurred
    Error { message: String },
}
