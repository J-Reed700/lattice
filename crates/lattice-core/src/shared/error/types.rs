//! Centralized error handling for the Lattice application.
//!
//! This module provides a unified error type that can be used throughout
//! the application to handle errors gracefully without panicking.

use std::fmt;
use std::io;

use serde::{Deserialize, Serialize};

use super::{ApplicationError, DomainError};

/// Main error type for the Lattice application.
///
/// Provides user-friendly error messages and automatic conversion
/// from common error types.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub enum AppError {
    /// I/O errors (file not found, permission denied, etc.)
    /// Stores error message and kind as strings for serializability
    Io { message: String, kind: String },

    /// JSON serialization/deserialization errors
    /// Stores error message and location for serializability
    Json {
        message: String,
        line: usize,
        column: usize,
    },

    /// Database errors
    Database(String),

    /// Another connection kept the work out of the database; a later attempt
    /// gets the lock.
    DatabaseBusy(String),

    /// Configuration file errors
    InvalidConfig(String),

    /// The configured context window cannot hold what the work needs.
    WindowTooSmall(String),

    /// The work was stopped on request.
    Cancelled(String),

    /// A lesson was held back for review; the text says why.
    LessonHeld(String),

    /// No Learning model is set; the text says where to choose one
    LearningModelNotSet(String),

    /// The bundled model server cannot run at all (binary missing or blocked,
    /// a library missing, exiting at once); reinstalling or another model fixes
    /// it, waiting does not. The text is what the learner reads.
    ModelServerCannotStart(String),

    /// Resource not found
    NotFound(String),

    /// Permission denied
    PermissionDenied(String),

    /// Invalid input from user
    InvalidInput(String),

    /// The request got no answer: DNS, connect, TLS, a reset or a timeout.
    Network(String),

    /// A web server answered with an error status instead of what was asked
    /// for; the connection worked. `message` says what it answered.
    HttpStatus { status: u16, message: String },

    /// File too large to process
    FileTooLarge {
        path: String,
        size_bytes: u64,
        max_size_bytes: u64,
    },

    /// File storage errors
    FileStorage(String),

    /// File not found (specific path)
    FileNotFound { path: String },

    /// File read error
    FileRead { path: String, reason: String },

    /// Embedding generation failed
    EmbeddingFailed { reason: String },

    /// Unsupported file type
    UnsupportedFileType { path: String, detected_type: String },

    /// Content extraction failed
    ContentExtraction { path: String, reason: String },

    /// Tokenization error
    TokenizationError { reason: String },

    /// Rate limit exceeded
    RateLimitExceeded(String),

    /// Invalid or corrupted data
    InvalidData(String),

    /// File system operation failed
    FileSystem(String),

    /// Invalid URL provided
    InvalidUrl(String),

    /// Invalid application state
    InvalidState(String),

    /// Storage backend error
    Storage(String),

    /// Required service is not available
    ServiceNotAvailable(String),

    /// A model server refused a request in a way that retrying cannot change:
    /// it rejected the request itself, or failed the same request the same way
    /// twice. The text carries the server's own words, sanitized, and the
    /// request's size against the configured context window.
    ModelRejected(String),

    /// A model answered, but the answer could not be used even when it was
    /// asked for once more: it did not parse, or broke a rule the caller
    /// checks. The text says what was wrong with the answer.
    ModelAnswerUnusable(String),

    /// A model's answer ended before it was complete and was not asked for again;
    /// the same work may succeed in smaller parts.
    AnswerCutOff(String),

    /// A request whose prompt alone fills the configured context window, so
    /// it leaves no room for an answer and was never sent. The numbers let a
    /// caller send less and ask again: `prompt_tokens` is the prompt as the
    /// backend's tokenizer counted it when `counted_exactly`, otherwise as
    /// estimated, and `window` is the configured window. `message` is what
    /// the user is told.
    PromptExceedsWindow {
        prompt_tokens: usize,
        window: usize,
        counted_exactly: bool,
        message: String,
    },

    /// Serialization error
    Serialization(String),

    /// Security violation or check failed
    Security(String),

    /// Keyring/credential storage error
    KeyringError(String),

    /// Validation failed
    ValidationFailed(String),

    /// Internal application error
    InternalError(String),

    /// Parsing error
    Parsing(String),

    /// Deserialization error
    Deserialization(String),

    /// Backup creation failed
    BackupCreationFailed(String),

    /// Backup restore failed
    BackupRestoreFailed(String),

    /// Backup file is corrupted
    BackupCorrupted(String),

    /// Database migration failed
    Migration(String),

    /// AI models not installed
    AiModelsNotInstalled(String),

    /// Model loading failed
    ModelLoadFailed(String),

    /// SECURITY FIX (CWE-367): Concurrent modification detected
    /// Prevents TOCTOU race conditions in multi-step operations
    ConcurrentModification { resource: String, details: String },

    /// Conflict error (e.g. revision mismatch)
    Conflict(String),

    /// Domain layer error - business rule violations
    /// BOXED to prevent stack overflow (DomainError is ~160 variants)
    Domain(Box<DomainError>),

    /// Application layer error - use case orchestration failures
    Application(Box<ApplicationError>),

    /// Generic error with custom message
    Other(String),
}

/// The refusal for a bundled server that cannot run: an install problem the
/// learner fixes by reinstalling or choosing another model, never by waiting.
pub fn server_cannot_start(reason: &str) -> AppError {
    AppError::ModelServerCannotStart(format!(
        "The model server could not start: {}. Reinstall Lattice, or choose another model in Settings.",
        reason.trim().trim_end_matches('.')
    ))
}

impl AppError {
    /// Create a NotFound error with resource type and ID
    pub fn not_found(resource_type: &str, resource_id: &str) -> Self {
        AppError::NotFound(format!("{} '{}' not found", resource_type, resource_id))
    }

    /// Create a validation error with field and reason
    pub fn validation_failed(field: &str, reason: &str) -> Self {
        AppError::ValidationFailed(format!("Field '{}': {}", field, reason))
    }

    /// Create a rate limit error with retry information
    pub fn rate_limited(retry_after_seconds: Option<u64>) -> Self {
        match retry_after_seconds {
            Some(seconds) => AppError::RateLimitExceeded(format!(
                "Rate limit exceeded. Retry after {} seconds",
                seconds
            )),
            None => AppError::RateLimitExceeded("Rate limit exceeded".to_string()),
        }
    }

    /// Create a file not found error
    pub fn file_not_found(path: &str) -> Self {
        AppError::FileNotFound {
            path: path.to_string(),
        }
    }

    /// Create a service unavailable error
    pub fn service_unavailable(service: &str, reason: &str) -> Self {
        AppError::ServiceNotAvailable(format!("{}: {}", service, reason))
    }

    /// Create an invalid input error
    pub fn invalid_input(description: &str) -> Self {
        AppError::InvalidInput(description.to_string())
    }

    /// Create a database error
    pub fn database(message: &str) -> Self {
        AppError::Database(message.to_string())
    }

    /// Create a network error
    pub fn network(message: &str) -> Self {
        AppError::Network(message.to_string())
    }

    /// Create an internal error
    pub fn internal(message: &str) -> Self {
        AppError::InternalError(message.to_string())
    }

    /// Add context to an error (creates a new error with additional context)
    pub fn with_context(self, context: &str) -> Self {
        let base_message = self.to_string();
        AppError::Other(format!("{}: {}", context, base_message))
    }

    /// Add a suggestion for recovery
    pub fn with_suggestion(self, suggestion: &str) -> Self {
        let base_message = self.to_string();
        AppError::Other(format!("{}. Suggestion: {}", base_message, suggestion))
    }

    /// Get a user-friendly error message
    pub fn to_user_friendly_message(&self) -> String {
        match self {
            AppError::FileTooLarge { .. } => {
                "This file is too large to process. Try a smaller file.".to_string()
            }
            AppError::UnsupportedFileType { .. } => "This file type is not supported.".to_string(),
            AppError::PermissionDenied(_) => {
                "Access denied. Please check file permissions.".to_string()
            }
            AppError::Network(_) => {
                "Network connection issue. Please check your internet connection.".to_string()
            }
            _ => self.to_string(),
        }
    }

    /// Check if the error is fatal (requires restart or intervention)
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            AppError::Database(_) | AppError::Migration(_) | AppError::BackupCorrupted(_)
        )
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AppError::Io { message, .. } => write!(
                f,
                "File operation failed: {}. Please check file permissions and path.",
                message
            ),
            AppError::Json { message, line, column } => write!(
                f,
                "Invalid JSON format at line {}, column {}: {}. Please check file syntax.",
                line, column, message
            ),
            AppError::Database(e) => write!(
                f,
                "Database error: {}. Try restarting the application.",
                e
            ),
            AppError::InvalidConfig(e) | AppError::WindowTooSmall(e) => write!(
                f,
                "Configuration error: {}. Please check your settings.",
                e
            ),
            AppError::Cancelled(e) | AppError::LessonHeld(e) => write!(f, "{}", e),
            AppError::LearningModelNotSet(e) => write!(f, "{}", e),
            AppError::ModelServerCannotStart(e) => write!(f, "{}", e),
            AppError::DatabaseBusy(e) => write!(f, "Database busy: {}", e),
            AppError::NotFound(e) => write!(f, "Not found: {}", e),
            AppError::PermissionDenied(e) => write!(
                f,
                "Permission denied: {}. Please check file permissions.",
                e
            ),
            AppError::InvalidInput(e) => write!(f, "Invalid input: {}", e),
            AppError::Network(e) => write!(f, "Network error: {}", e),
            AppError::HttpStatus { message, .. } => write!(f, "{}", message),
            AppError::FileTooLarge { path, size_bytes, max_size_bytes } => write!(
                f,
                "File is too large to index: {} ({} MB). Maximum size is {} MB. Consider splitting the file or excluding it from indexing.",
                path,
                size_bytes / 1024 / 1024,
                max_size_bytes / 1024 / 1024
            ),
            AppError::FileStorage(e) => write!(
                f,
                "File storage error: {}",
                e
            ),
            AppError::FileNotFound { path } => write!(
                f,
                "File not found: {}. Please check the path exists.",
                path
            ),
            AppError::FileRead { path, reason } => write!(
                f,
                "Failed to read file '{}': {}",
                path, reason
            ),
            AppError::EmbeddingFailed { reason } => write!(
                f,
                "Embedding generation failed: {}",
                reason
            ),
            AppError::UnsupportedFileType { path, detected_type } => write!(
                f,
                "Unsupported file type for '{}': {}. Please convert to a supported format.",
                path, detected_type
            ),
            AppError::ContentExtraction { path, reason } => write!(
                f,
                "Failed to extract content from '{}': {}",
                path, reason
            ),
            AppError::TokenizationError { reason } => write!(
                f,
                "Text tokenization failed: {}",
                reason
            ),
            AppError::RateLimitExceeded(e) => write!(
                f,
                "Rate limit exceeded: {}. Please wait before retrying.",
                e
            ),
            AppError::InvalidData(e) => write!(f, "Invalid data: {}", e),
            AppError::FileSystem(e) => write!(f, "File system error: {}", e),
            AppError::InvalidUrl(e) => write!(f, "Invalid URL: {}", e),
            AppError::InvalidState(e) => write!(f, "Invalid state: {}", e),
            AppError::Storage(e) => write!(f, "Storage error: {}", e),
            AppError::ServiceNotAvailable(e) => write!(f, "Service not available: {}", e),
            AppError::ModelRejected(e) => write!(f, "{}", e),
            AppError::ModelAnswerUnusable(e) => {
                write!(f, "The model's answer could not be used: {}", e)
            }
            AppError::AnswerCutOff(e) => {
                write!(f, "The model's answer could not be used: {}", e)
            }
            AppError::PromptExceedsWindow { message, .. } => write!(f, "{}", message),
            AppError::Serialization(e) => write!(f, "Serialization error: {}", e),
            AppError::Security(e) => write!(f, "Security error: {}", e),
            AppError::KeyringError(e) => write!(f, "Keyring error: {}", e),
            AppError::ValidationFailed(e) => write!(f, "Validation failed: {}", e),
            AppError::InternalError(e) => write!(f, "Internal error: {}", e),
            AppError::Parsing(e) => write!(f, "Parsing error: {}", e),
            AppError::Deserialization(e) => write!(f, "Deserialization error: {}", e),
            AppError::BackupCreationFailed(e) => write!(
                f,
                "Backup creation failed: {}. Please check disk space and permissions.",
                e
            ),
            AppError::BackupRestoreFailed(e) => write!(
                f,
                "Backup restore failed: {}. The original database has been preserved.",
                e
            ),
            AppError::BackupCorrupted(e) => write!(
                f,
                "Backup file is corrupted: {}. Please use a different backup.",
                e
            ),
            AppError::Migration(e) => write!(
                f,
                "Database migration failed: {}. Please restart the application or restore from backup.",
                e
            ),
            AppError::AiModelsNotInstalled(e) => write!(
                f,
                "AI models not installed: {}. Download models from Settings → Models to enable AI features.",
                e
            ),
            AppError::ModelLoadFailed(e) => write!(f, "{}", e),
            AppError::ConcurrentModification { resource, details } => write!(
                f,
                "Concurrent modification detected for {}: {}. Please retry the operation.",
                resource, details
            ),
            AppError::Conflict(e) => write!(f, "Conflict: {}", e),
            AppError::Domain(e) => write!(f, "Domain error: {}", e),
            AppError::Application(e) => write!(f, "Application error: {}", e),
            AppError::Other(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for AppError {}

/// Convert AppError to String for Tauri command return types.
///
/// This allows using Result<T, AppError> in Tauri commands, which
/// will automatically convert to Result<T, String> for the frontend.
impl From<AppError> for String {
    fn from(err: AppError) -> String {
        err.to_string()
    }
}

/// Auto-convert from std::io::Error
impl From<io::Error> for AppError {
    fn from(err: io::Error) -> Self {
        AppError::Io {
            message: err.to_string(),
            kind: format!("{:?}", err.kind()),
        }
    }
}

/// Auto-convert from serde_json::Error
impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::Json {
            message: err.to_string(),
            line: err.line(),
            column: err.column(),
        }
    }
}

/// Auto-convert from String to AppError
impl From<String> for AppError {
    fn from(msg: String) -> Self {
        AppError::Other(msg)
    }
}

/// Auto-convert from &str to AppError
impl From<&str> for AppError {
    fn from(msg: &str) -> Self {
        AppError::Other(msg.to_string())
    }
}

/// Auto-convert from anyhow::Error
impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::Other(err.to_string())
    }
}

/// Auto-convert from DomainTypeError to AppError
impl From<crate::shared::types::DomainTypeError> for AppError {
    fn from(e: crate::shared::types::DomainTypeError) -> Self {
        AppError::ValidationFailed(e.to_string())
    }
}

impl From<DomainError> for AppError {
    fn from(err: DomainError) -> Self {
        AppError::Domain(Box::new(err))
    }
}

impl From<ApplicationError> for AppError {
    fn from(err: ApplicationError) -> Self {
        AppError::Application(Box::new(err))
    }
}

/// Convenience type alias for Results using AppError by default
pub type Result<T, E = AppError> = std::result::Result<T, E>;

#[cfg(test)]
mod tests;
