//! Centralized error handling for the Recall application.
//!
//! This module provides a unified error type that can be used throughout
//! the application to handle errors gracefully without panicking.

use std::collections::HashMap;
use std::fmt;
use std::io;

use serde::{Deserialize, Serialize};

use crate::application::error::ApplicationError;
use crate::domain::error::DomainError;

/// Main error type for the Recall application.
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

    /// Configuration file errors
    InvalidConfig(String),

    /// Resource not found
    NotFound(String),

    /// Permission denied
    PermissionDenied(String),

    /// Invalid input from user
    InvalidInput(String),

    /// Network errors
    Network(String),

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

    /// Queue is full
    QueueFull,

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

    /// Domain layer error - business rule violations
    /// BOXED to prevent stack overflow (DomainError is ~160 variants)
    Domain(Box<DomainError>),

    /// Application layer error - use case orchestration failures
    Application(Box<ApplicationError>),

    /// Generic error with custom message
    Other(String),
}

impl AppError {
    // ==================== Builder Methods ====================

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

    // ==================== Context Methods ====================

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

    // ==================== Error Codes ====================

    /// Get the error code for this error type
    pub fn error_code(&self) -> &'static str {
        match self {
            AppError::Io { .. } => "IO_ERROR",
            AppError::Json { .. } => "JSON_ERROR",
            AppError::Database(_) => "DATABASE_ERROR",
            AppError::InvalidConfig(_) => "INVALID_CONFIG",
            AppError::NotFound(_) => "NOT_FOUND",
            AppError::PermissionDenied(_) => "PERMISSION_DENIED",
            AppError::InvalidInput(_) => "INVALID_INPUT",
            AppError::Network(_) => "NETWORK_ERROR",
            AppError::FileTooLarge { .. } => "FILE_TOO_LARGE",
            AppError::FileStorage(_) => "FILE_STORAGE_ERROR",
            AppError::FileNotFound { .. } => "FILE_NOT_FOUND",
            AppError::FileRead { .. } => "FILE_READ_ERROR",
            AppError::EmbeddingFailed { .. } => "EMBEDDING_FAILED",
            AppError::QueueFull => "QUEUE_FULL",
            AppError::UnsupportedFileType { .. } => "UNSUPPORTED_FILE_TYPE",
            AppError::ContentExtraction { .. } => "CONTENT_EXTRACTION_FAILED",
            AppError::TokenizationError { .. } => "TOKENIZATION_ERROR",
            AppError::RateLimitExceeded(_) => "RATE_LIMIT_EXCEEDED",
            AppError::InvalidData(_) => "INVALID_DATA",
            AppError::FileSystem(_) => "FILE_SYSTEM_ERROR",
            AppError::InvalidUrl(_) => "INVALID_URL",
            AppError::InvalidState(_) => "INVALID_STATE",
            AppError::Storage(_) => "STORAGE_ERROR",
            AppError::ServiceNotAvailable(_) => "SERVICE_UNAVAILABLE",
            AppError::Serialization(_) => "SERIALIZATION_ERROR",
            AppError::Security(_) => "SECURITY_ERROR",
            AppError::KeyringError(_) => "KEYRING_ERROR",
            AppError::ValidationFailed(_) => "VALIDATION_FAILED",
            AppError::InternalError(_) => "INTERNAL_ERROR",
            AppError::Parsing(_) => "PARSING_ERROR",
            AppError::Deserialization(_) => "DESERIALIZATION_ERROR",
            AppError::BackupCreationFailed(_) => "BACKUP_CREATION_FAILED",
            AppError::BackupRestoreFailed(_) => "BACKUP_RESTORE_FAILED",
            AppError::BackupCorrupted(_) => "BACKUP_CORRUPTED",
            AppError::Migration(_) => "MIGRATION_ERROR",
            AppError::AiModelsNotInstalled(_) => "AI_MODELS_NOT_INSTALLED",
            AppError::ModelLoadFailed(_) => "MODEL_LOAD_FAILED",
            AppError::ConcurrentModification { .. } => "CONCURRENT_MODIFICATION",
            AppError::Domain(_) => "DOMAIN_ERROR",
            AppError::Application(_) => "APPLICATION_ERROR",
            AppError::Other(_) => "UNKNOWN_ERROR",
        }
    }

    /// Get the HTTP status code for this error (for API responses)
    pub fn http_status_code(&self) -> u16 {
        match self {
            AppError::NotFound(_) | AppError::FileNotFound { .. } => 404,
            AppError::PermissionDenied(_) | AppError::Security(_) => 403,
            AppError::InvalidInput(_)
            | AppError::ValidationFailed(_)
            | AppError::InvalidUrl(_)
            | AppError::UnsupportedFileType { .. } => 400,
            AppError::RateLimitExceeded(_) => 429,
            AppError::ServiceNotAvailable(_) | AppError::AiModelsNotInstalled(_) => 503,
            AppError::FileTooLarge { .. } => 413,
            AppError::ConcurrentModification { .. } => 409,
            AppError::Domain(err) if err.is_not_found() => 404,
            AppError::Domain(err) if err.is_business_rule_violation() => 400,
            AppError::Application(err) if err.is_config_error() => 400,
            _ => 500,
        }
    }

    // ==================== Categorization Methods ====================

    /// Get a user-friendly error message
    pub fn to_user_friendly_message(&self) -> String {
        match self {
            AppError::QueueFull => {
                "The system is currently busy. Please try again in a moment.".to_string()
            }
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

    /// Check if the error is recoverable (user can retry)
    pub fn is_recoverable(&self) -> bool {
        match self {
            AppError::QueueFull
            | AppError::Network(_)
            | AppError::Io { .. }
            | AppError::RateLimitExceeded(_)
            | AppError::ConcurrentModification { .. } => true,
            AppError::Domain(err) => err.is_retryable(),
            AppError::Application(err) => err.is_recoverable(),
            _ => false,
        }
    }

    /// Check if the error is user-fixable (user action can resolve it)
    pub fn is_user_fixable(&self) -> bool {
        match self {
            AppError::InvalidInput(_)
            | AppError::InvalidConfig(_)
            | AppError::PermissionDenied(_)
            | AppError::FileTooLarge { .. }
            | AppError::UnsupportedFileType { .. }
            | AppError::ValidationFailed(_) => true,
            AppError::Domain(err) => err.is_business_rule_violation(),
            _ => false,
        }
    }

    /// Check if the error is fatal (requires restart or intervention)
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            AppError::Database(_) | AppError::Migration(_) | AppError::BackupCorrupted(_)
        )
    }

    /// Get structured context for the error
    pub fn get_context(&self) -> HashMap<String, serde_json::Value> {
        use serde_json::json;
        let mut context = HashMap::new();

        match self {
            AppError::Io { message, kind } => {
                context.insert("message".to_string(), json!(message));
                context.insert("kind".to_string(), json!(kind));
            }
            AppError::Json {
                message,
                line,
                column,
            } => {
                context.insert("message".to_string(), json!(message));
                context.insert("line".to_string(), json!(line));
                context.insert("column".to_string(), json!(column));
            }
            AppError::FileTooLarge {
                path,
                size_bytes,
                max_size_bytes,
            } => {
                context.insert("path".to_string(), json!(path));
                context.insert("size_bytes".to_string(), json!(size_bytes));
                context.insert("max_size_bytes".to_string(), json!(max_size_bytes));
                context.insert("size_mb".to_string(), json!(size_bytes / 1024 / 1024));
                context.insert(
                    "max_size_mb".to_string(),
                    json!(max_size_bytes / 1024 / 1024),
                );
            }
            AppError::FileNotFound { path } => {
                context.insert("path".to_string(), json!(path));
            }
            AppError::FileRead { path, reason } => {
                context.insert("path".to_string(), json!(path));
                context.insert("reason".to_string(), json!(reason));
            }
            AppError::UnsupportedFileType {
                path,
                detected_type,
            } => {
                context.insert("path".to_string(), json!(path));
                context.insert("detected_type".to_string(), json!(detected_type));
            }
            AppError::ContentExtraction { path, reason } => {
                context.insert("path".to_string(), json!(path));
                context.insert("reason".to_string(), json!(reason));
            }
            AppError::ConcurrentModification { resource, details } => {
                context.insert("resource".to_string(), json!(resource));
                context.insert("details".to_string(), json!(details));
            }
            _ => {}
        }

        context
    }

    /// Get recovery suggestions for the error
    pub fn get_suggestions(&self) -> Vec<String> {
        match self {
            AppError::QueueFull => vec![
                "Wait a few seconds and try again".to_string(),
                "Process fewer items at once".to_string(),
            ],
            AppError::FileTooLarge { max_size_bytes, .. } => vec![
                format!(
                    "Split the file into chunks smaller than {} MB",
                    max_size_bytes / 1024 / 1024
                ),
                "Compress the file before processing".to_string(),
            ],
            AppError::Network(_) => vec![
                "Check your internet connection".to_string(),
                "Try again in a few moments".to_string(),
                "Check if a firewall or proxy is blocking the connection".to_string(),
            ],
            AppError::PermissionDenied(_) => vec![
                "Check file permissions".to_string(),
                "Run the application with appropriate privileges".to_string(),
                "Ensure the file is not locked by another process".to_string(),
            ],
            AppError::AiModelsNotInstalled(_) => vec![
                "Download the required models from Settings → Models".to_string(),
                "Check your internet connection for model downloads".to_string(),
            ],
            AppError::RateLimitExceeded(_) => vec![
                "Wait before making more requests".to_string(),
                "Reduce the frequency of operations".to_string(),
            ],
            AppError::ServiceNotAvailable(_) => vec![
                "Wait for the service to become available".to_string(),
                "Restart the application".to_string(),
                "Check system resources (CPU, memory)".to_string(),
            ],
            _ => vec![],
        }
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
            AppError::InvalidConfig(e) => write!(
                f,
                "Configuration error: {}. Please check your settings.",
                e
            ),
            AppError::NotFound(e) => write!(f, "Not found: {}", e),
            AppError::PermissionDenied(e) => write!(
                f,
                "Permission denied: {}. Please check file permissions.",
                e
            ),
            AppError::InvalidInput(e) => write!(
                f,
                "Invalid input: {}. Please check your input and try again.",
                e
            ),
            AppError::Network(e) => write!(
                f,
                "Network error: {}. Please check your connection.",
                e
            ),
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
            AppError::QueueFull => write!(
                f,
                "Processing queue is full. Please wait and try again."
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
            AppError::Domain(e) => write!(f, "Domain error: {}", e),
            AppError::Application(e) => write!(f, "Application error: {}", e),
            AppError::Other(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for AppError {}

/// Serializable error response for Tauri IPC and API responses.
///
/// This type allows structured error information to be sent to the frontend
/// instead of plain strings. It includes error type discrimination, human-readable
/// messages, optional context, and recoverability information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Error code for programmatic handling (e.g., "NOT_FOUND", "DATABASE_ERROR")
    pub code: String,

    /// Human-readable error message
    pub message: String,

    /// Optional structured context
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub context: HashMap<String, serde_json::Value>,

    /// Recovery suggestions
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub suggestions: Vec<String>,

    /// Whether this error is recoverable
    #[serde(default)]
    pub recoverable: bool,

    /// HTTP status code (for API responses)
    pub status_code: u16,

    /// Error type discriminator (legacy field for backwards compatibility)
    #[serde(rename = "type")]
    pub error_type: String,
}

/// Convert AppError to ErrorResponse for Tauri IPC and APIs.
impl From<AppError> for ErrorResponse {
    fn from(error: AppError) -> Self {
        ErrorResponse {
            code: error.error_code().to_string(),
            message: error.to_user_friendly_message(),
            context: error.get_context(),
            suggestions: error.get_suggestions(),
            recoverable: error.is_recoverable(),
            status_code: error.http_status_code(),
            error_type: error.error_code().to_string(), // For backwards compatibility
        }
    }
}

/// Convert AppError to String for Tauri command return types.
///
/// This allows using Result<T, AppError> in Tauri commands, which
/// will automatically convert to Result<T, String> for the frontend.
impl From<AppError> for String {
    fn from(err: AppError) -> String {
        err.to_string()
    }
}

// ==================== Auto-conversions from other error types ====================

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

/// Auto-convert from sqlx::Error
impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::Database(err.to_string())
    }
}

/// Auto-convert from ndarray::ShapeError
impl From<ndarray::ShapeError> for AppError {
    fn from(err: ndarray::ShapeError) -> Self {
        AppError::Other(format!("Array shape error: {}", err))
    }
}

/// Auto-convert from keyring::Error
impl From<keyring::Error> for AppError {
    fn from(err: keyring::Error) -> Self {
        AppError::KeyringError(err.to_string())
    }
}

/// Auto-convert from ONNX Runtime errors. `ort 2.0.0-rc.11+` made
/// `Error` generic (`Error<R>`) so failed builder methods can return a
/// recovery value the caller could reuse. We don't use the recovery
/// path — and some recovery types (e.g. `SessionBuilder`) aren't Send,
/// so we leave `R` unbounded and just discard it.
impl<R> From<ort::Error<R>> for AppError {
    fn from(err: ort::Error<R>) -> Self {
        AppError::EmbeddingFailed {
            reason: format!("ONNX runtime error: {}", err),
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
impl From<crate::shared::domain_types::DomainTypeError> for AppError {
    fn from(e: crate::shared::domain_types::DomainTypeError) -> Self {
        AppError::ValidationFailed(e.to_string())
    }
}

/// Auto-convert from LLMError to AppError
impl From<crate::infrastructure::llm::types::LLMError> for AppError {
    fn from(e: crate::infrastructure::llm::types::LLMError) -> Self {
        use crate::infrastructure::llm::types::LLMError;
        match e {
            LLMError::ModelNotLoaded => AppError::ModelLoadFailed("Model not loaded".to_string()),
            LLMError::GenerationFailed(msg) => {
                AppError::Other(format!("LLM generation failed: {}", msg))
            }
            LLMError::ClientUnavailable(msg) => AppError::ServiceNotAvailable(msg),
            LLMError::Network(msg) => AppError::Network(msg),
            LLMError::InvalidConfig(msg) => AppError::InvalidConfig(msg),
            LLMError::Timeout => AppError::Other("LLM request timed out".to_string()),
            LLMError::Io(e) => AppError::Io {
                message: e.to_string(),
                kind: format!("{:?}", e.kind()),
            },
            LLMError::Reqwest(e) => AppError::Network(e.to_string()),
            LLMError::Other(msg) => AppError::Other(format!("LLM error: {}", msg)),
        }
    }
}

/// Auto-convert from DownloadError to AppError
impl From<crate::domain::download::DownloadError> for AppError {
    fn from(e: crate::domain::download::DownloadError) -> Self {
        use crate::domain::download::DownloadError;
        match e {
            DownloadError::InvalidUrl(msg) => AppError::InvalidUrl(msg),
            DownloadError::InvalidDestination(msg) => AppError::InvalidInput(msg),
            DownloadError::InvalidStateTransition { from, to } => AppError::InvalidState(format!(
                "Invalid download state transition from {:?} to {:?}",
                from, to
            )),
            DownloadError::SessionNotFound(id) => {
                AppError::NotFound(format!("Download session not found: {}", id))
            }
            DownloadError::ChecksumMismatch { expected, actual } => AppError::InvalidData(format!(
                "Checksum mismatch: expected {}, got {}",
                expected, actual
            )),
            DownloadError::NetworkError(msg) => AppError::Network(msg),
            DownloadError::IoError(msg) => AppError::FileSystem(msg),
            DownloadError::Cancelled => AppError::Other("Download cancelled".to_string()),
            DownloadError::MaxRetriesExceeded => {
                AppError::Other("Maximum retry attempts exceeded".to_string())
            }
            DownloadError::HttpError { status, message } => {
                AppError::Network(format!("HTTP error {}: {}", status, message))
            }
            DownloadError::InvalidResponse(msg) => {
                AppError::Network(format!("Invalid HTTP response: {}", msg))
            }
            DownloadError::ValidationFailed(msg) => AppError::ValidationFailed(msg),
            DownloadError::EngineInitializationError(msg) => {
                AppError::InternalError(format!("Failed to initialize download engine: {}", msg))
            }
            DownloadError::InsufficientDiskSpace {
                required,
                available,
            } => AppError::FileSystem(format!(
                "Insufficient disk space: required {} bytes, available {} bytes",
                required, available
            )),
        }
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

/// Extension trait for adding context to Result types.
///
/// This trait provides convenient methods to add contextual information
/// to errors, making debugging and error messages more helpful.
///
/// # Examples
///
/// ```
/// use vault::error::{ResultExt, AppError};
///
/// fn read_config(path: &str) -> Result<String, AppError> {
///     std::fs::read_to_string(path)
///         .context("Failed to read config file")?;
///     Ok("config".to_string())
/// }
/// ```
pub trait ResultExt<T> {
    /// Add static context to an error.
    fn context(self, msg: impl Into<String>) -> Result<T>;

    /// Add dynamic context to an error using a closure.
    /// The closure is only called if there's an error.
    fn with_context<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String;
}

impl<T, E> ResultExt<T> for std::result::Result<T, E>
where
    E: Into<AppError>,
{
    fn context(self, msg: impl Into<String>) -> Result<T> {
        self.map_err(|e| {
            let base_error = e.into();
            AppError::Other(format!("{}: {}", msg.into(), base_error))
        })
    }

    fn with_context<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String,
    {
        self.map_err(|e| {
            let base_error = e.into();
            AppError::Other(format!("{}: {}", f(), base_error))
        })
    }
}

impl<T> ResultExt<T> for Option<T> {
    fn context(self, msg: impl Into<String>) -> Result<T> {
        self.ok_or_else(|| AppError::Other(msg.into()))
    }

    fn with_context<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String,
    {
        self.ok_or_else(|| AppError::Other(f()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_methods() {
        let not_found = AppError::not_found("Document", "doc-123");
        assert!(not_found.to_string().contains("Document"));
        assert!(not_found.to_string().contains("doc-123"));
        assert_eq!(not_found.error_code(), "NOT_FOUND");

        let validation = AppError::validation_failed("email", "Invalid format");
        assert!(validation.to_string().contains("email"));
        assert!(validation.to_string().contains("Invalid format"));
        assert_eq!(validation.error_code(), "VALIDATION_FAILED");

        let rate_limit = AppError::rate_limited(Some(60));
        assert!(rate_limit.to_string().contains("60 seconds"));
        assert_eq!(rate_limit.error_code(), "RATE_LIMIT_EXCEEDED");
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(AppError::QueueFull.error_code(), "QUEUE_FULL");
        assert_eq!(
            AppError::Database("test".to_string()).error_code(),
            "DATABASE_ERROR"
        );
        assert_eq!(
            AppError::Network("test".to_string()).error_code(),
            "NETWORK_ERROR"
        );
    }

    #[test]
    fn test_http_status_codes() {
        assert_eq!(AppError::not_found("User", "123").http_status_code(), 404);
        assert_eq!(
            AppError::validation_failed("email", "Invalid").http_status_code(),
            400
        );
        assert_eq!(AppError::rate_limited(None).http_status_code(), 429);
        assert_eq!(
            AppError::ServiceNotAvailable("test".to_string()).http_status_code(),
            503
        );
        assert_eq!(
            AppError::Database("error".to_string()).http_status_code(),
            500
        );
    }

    #[test]
    fn test_context_methods() {
        let error =
            AppError::not_found("User", "123").with_context("While processing user request");
        assert!(error.to_string().contains("While processing"));

        let error = AppError::validation_failed("email", "Invalid format")
            .with_suggestion("Please use a valid email format like user@example.com");
        assert!(error.to_string().contains("Suggestion"));
    }

    #[test]
    fn test_get_suggestions() {
        let queue_full = AppError::QueueFull;
        let suggestions = queue_full.get_suggestions();
        assert!(!suggestions.is_empty());
        assert!(suggestions.iter().any(|s| s.contains("try again")));

        let network = AppError::Network("Connection failed".to_string());
        let suggestions = network.get_suggestions();
        assert!(suggestions
            .iter()
            .any(|s| s.contains("internet connection")));
    }

    #[test]
    fn test_error_response_conversion() {
        let error = AppError::not_found("Document", "doc-123");
        let response = ErrorResponse::from(error);

        assert_eq!(response.code, "NOT_FOUND");
        assert_eq!(response.status_code, 404);
        assert!(!response.recoverable);
        assert!(response.message.len() > 0);
    }

    #[test]
    fn test_structured_context() {
        let error = AppError::FileTooLarge {
            path: "/test/file.pdf".to_string(),
            size_bytes: 100_000_000,
            max_size_bytes: 50_000_000,
        };

        let context = error.get_context();
        assert!(context.contains_key("path"));
        assert!(context.contains_key("size_bytes"));
        assert!(context.contains_key("size_mb"));
    }

    #[test]
    fn test_domain_error_integration() {
        let domain_err = DomainError::EntityNotFound {
            entity_type: "Document".to_string(),
            identifier: "doc-123".to_string(),
        };

        let app_err: AppError = domain_err.into();
        assert_eq!(app_err.error_code(), "DOMAIN_ERROR");
        assert_eq!(app_err.http_status_code(), 404); // Domain not found maps to 404
    }

    #[test]
    fn test_is_recoverable() {
        assert!(AppError::QueueFull.is_recoverable());
        assert!(AppError::Network("error".to_string()).is_recoverable());
        assert!(AppError::rate_limited(None).is_recoverable());
        assert!(!AppError::validation_failed("field", "reason").is_recoverable());
    }

    #[test]
    fn test_is_user_fixable() {
        assert!(AppError::validation_failed("email", "Invalid").is_user_fixable());
        assert!(AppError::InvalidInput("test".to_string()).is_user_fixable());
        assert!(AppError::FileTooLarge {
            path: "test".to_string(),
            size_bytes: 1000,
            max_size_bytes: 500
        }
        .is_user_fixable());
        assert!(!AppError::Database("error".to_string()).is_user_fixable());
    }

    #[test]
    fn test_is_fatal() {
        assert!(AppError::Database("error".to_string()).is_fatal());
        assert!(AppError::Migration("error".to_string()).is_fatal());
        assert!(AppError::BackupCorrupted("error".to_string()).is_fatal());
        assert!(!AppError::Network("error".to_string()).is_fatal());
    }
}
