/// Q&A Module Types
///
/// Data structures and error types for the Q&A engine.
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
impl From<crate::features::llm::engine::types::LLMError> for QAError {
    fn from(e: crate::features::llm::engine::types::LLMError) -> Self {
        QAError::OllamaUnavailable(e.to_string())
    }
}
