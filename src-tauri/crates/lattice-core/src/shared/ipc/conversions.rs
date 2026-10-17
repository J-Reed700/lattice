//! Mapping application, domain, I/O, database and serialization errors onto `ApiError`.
use super::*;

/// Convert ApplicationError (application layer) to ApiError
impl From<ApplicationError> for ApiError {
    fn from(err: ApplicationError) -> Self {
        match err {
            // Domain errors
            ApplicationError::Domain(domain_err) => domain_err.into(),

            // Configuration errors
            ApplicationError::InvalidConfig { ref details } => ApiError {
                code: ErrorCode::InvalidConfig,
                message: "Invalid configuration".to_string(),
                details: Some(details.clone()),
            },

            // Service errors
            ApplicationError::ServiceNotAvailable {
                ref service,
                ref reason,
            } => ApiError {
                code: ErrorCode::ServiceNotAvailable,
                message: format!("Service '{}' not available", service),
                details: Some(reason.clone()),
            },

            // Queue full
            ApplicationError::QueueFull { ref queue_name } => ApiError {
                code: ErrorCode::QueueFull,
                message: format!("Queue '{}' is full", queue_name),
                details: Some("Please retry later".to_string()),
            },

            // Serialization errors
            ApplicationError::SerializationFailed {
                ref context,
                ref reason,
            } => ApiError {
                code: ErrorCode::SerializationError,
                message: format!("Serialization failed for {}", context),
                details: Some(reason.clone()),
            },

            // Deserialization errors
            ApplicationError::DeserializationFailed {
                ref context,
                ref reason,
            } => ApiError {
                code: ErrorCode::DeserializationError,
                message: format!("Deserialization failed for {}", context),
                details: Some(reason.clone()),
            },

            // JSON errors
            ApplicationError::JsonError {
                ref message,
                line,
                column,
            } => ApiError {
                code: ErrorCode::ParsingError,
                message: "JSON parsing error".to_string(),
                details: Some(format!("{}:{}: {}", line, column, message)),
            },

            // Parsing errors
            ApplicationError::ParsingError {
                ref context,
                ref reason,
            } => ApiError {
                code: ErrorCode::ParsingError,
                message: format!("Parsing error in {}", context),
                details: Some(reason.clone()),
            },

            ApplicationError::UseCaseFailed {
                ref use_case,
                ref reason,
            } => ApiError {
                code: ErrorCode::ProcessingError,
                message: format!("Use case '{}' failed", use_case),
                details: Some(reason.clone()),
            },

            // Mapping errors
            ApplicationError::MappingError {
                ref context,
                ref reason,
            } => ApiError {
                code: ErrorCode::ProcessingError,
                message: format!("Mapping error in {}", context),
                details: Some(reason.clone()),
            },
        }
    }
}

/// Convert AppError (shared layer) to ApiError
impl From<AppError> for ApiError {
    fn from(err: AppError) -> Self {
        match err {
            // Domain errors
            AppError::Domain(domain_err) => (*domain_err).into(),

            // Application layer errors
            AppError::Application(app_err) => (*app_err).into(),

            // Configuration errors
            AppError::InvalidConfig(ref details) => ApiError {
                code: ErrorCode::InvalidConfig,
                message: "Invalid configuration".to_string(),
                details: Some(details.clone()),
            },

            // Resource errors
            AppError::NotFound(ref msg) => ApiError {
                code: ErrorCode::NotFound,
                message: format!("Not found: {}", msg),
                details: None,
            },

            // Permission errors
            AppError::PermissionDenied(ref msg) => ApiError {
                code: ErrorCode::PermissionDenied,
                message: format!("Permission denied: {}", msg),
                details: None,
            },

            AppError::InvalidInput(ref msg) => ApiError {
                code: ErrorCode::InvalidInput,
                message: format!("Invalid input: {}", msg),
                details: None,
            },
            AppError::ValidationFailed(ref msg) => ApiError {
                code: ErrorCode::ValidationError,
                message: format!("Validation failed: {}", msg),
                details: None,
            },
            AppError::InvalidState(ref msg) => ApiError {
                code: ErrorCode::InvalidState,
                message: format!("Invalid state: {}", msg),
                details: None,
            },

            // File errors
            AppError::FileNotFound { ref path } => ApiError {
                code: ErrorCode::FileNotFound,
                message: format!("File not found: {}", path),
                details: None,
            },
            AppError::FileTooLarge {
                ref path,
                size_bytes,
                max_size_bytes,
            } => ApiError {
                code: ErrorCode::FileTooLarge,
                message: format!("File too large: {}", path),
                details: Some(format!(
                    "Size: {} bytes, Max: {} bytes",
                    size_bytes, max_size_bytes
                )),
            },
            AppError::UnsupportedFileType {
                ref path,
                ref detected_type,
            } => ApiError {
                code: ErrorCode::UnsupportedFileType,
                message: format!("Unsupported file type: {}", path),
                details: Some(format!("Detected type: {}", detected_type)),
            },
            AppError::FileRead {
                ref path,
                ref reason,
            } => ApiError {
                code: ErrorCode::FileReadError,
                message: format!("Failed to read file: {}", path),
                details: Some(reason.clone()),
            },
            AppError::FileStorage(ref msg) | AppError::FileSystem(ref msg) => ApiError {
                code: ErrorCode::FileSystemError,
                message: "File system error".to_string(),
                details: Some(msg.clone()),
            },

            // Database errors
            AppError::Database(ref msg) => ApiError {
                code: ErrorCode::DatabaseError,
                message: "Database error".to_string(),
                details: Some(sanitize_database_error(msg)),
            },
            AppError::Migration(ref msg) => ApiError {
                code: ErrorCode::MigrationError,
                message: "Database migration failed".to_string(),
                details: Some(msg.clone()),
            },

            // Network errors
            AppError::Network(ref msg) => ApiError {
                code: ErrorCode::NetworkError,
                message: "Network error".to_string(),
                details: Some(msg.clone()),
            },
            AppError::InvalidUrl(ref msg) => ApiError {
                code: ErrorCode::InvalidInput,
                message: "Invalid URL".to_string(),
                details: Some(msg.clone()),
            },
            AppError::RateLimitExceeded(ref msg) => ApiError {
                code: ErrorCode::RateLimitExceeded,
                message: "Rate limit exceeded".to_string(),
                details: Some(msg.clone()),
            },

            // Service errors
            AppError::ServiceNotAvailable(ref msg) => ApiError {
                code: ErrorCode::ServiceNotAvailable,
                message: "Service not available".to_string(),
                details: Some(msg.clone()),
            },
            AppError::ModelRejected(ref msg) => ApiError {
                code: ErrorCode::ProcessingError,
                message: "The model rejected the request".to_string(),
                details: Some(msg.clone()),
            },
            AppError::AiModelsNotInstalled(ref msg) | AppError::ModelLoadFailed(ref msg) => {
                ApiError {
                    code: ErrorCode::ModelNotLoaded,
                    message: "AI model not available".to_string(),
                    details: Some(msg.clone()),
                }
            }

            // Processing errors
            AppError::EmbeddingFailed { ref reason } => ApiError {
                code: ErrorCode::EmbeddingError,
                message: "Embedding generation failed".to_string(),
                details: Some(reason.clone()),
            },
            AppError::ContentExtraction {
                ref path,
                ref reason,
            } => ApiError {
                code: ErrorCode::ExtractionError,
                message: format!("Content extraction failed: {} ({})", path, reason),
                details: Some(reason.clone()),
            },
            AppError::TokenizationError { ref reason } => ApiError {
                code: ErrorCode::TokenizationError,
                message: "Tokenization error".to_string(),
                details: Some(reason.clone()),
            },
            AppError::Serialization(ref msg) => ApiError {
                code: ErrorCode::SerializationError,
                message: "Serialization failed".to_string(),
                details: Some(msg.clone()),
            },
            AppError::Deserialization(ref msg) | AppError::Parsing(ref msg) => ApiError {
                code: ErrorCode::DeserializationError,
                message: "Deserialization failed".to_string(),
                details: Some(msg.clone()),
            },

            // Concurrency errors
            AppError::ConcurrentModification {
                ref resource,
                ref details,
            } => ApiError {
                code: ErrorCode::ConcurrentModification,
                message: format!("Concurrent modification: {}", resource),
                details: Some(details.clone()),
            },
            AppError::QueueFull => ApiError {
                code: ErrorCode::QueueFull,
                message: "Processing queue is full".to_string(),
                details: Some("Please retry later".to_string()),
            },

            // Backup errors
            AppError::BackupCreationFailed(ref msg) => ApiError {
                code: ErrorCode::BackupCreationFailed,
                message: "Backup creation failed".to_string(),
                details: Some(msg.clone()),
            },
            AppError::BackupRestoreFailed(ref msg) => ApiError {
                code: ErrorCode::BackupRestoreFailed,
                message: "Backup restore failed".to_string(),
                details: Some(msg.clone()),
            },
            AppError::BackupCorrupted(ref msg) => ApiError {
                code: ErrorCode::BackupCorrupted,
                message: "Backup file is corrupted".to_string(),
                details: Some(msg.clone()),
            },

            // I/O errors
            AppError::Io { ref message, .. } => ApiError {
                code: ErrorCode::FileSystemError,
                message: "I/O error".to_string(),
                details: Some(message.clone()),
            },

            // JSON errors
            AppError::Json {
                ref message,
                line,
                column,
            } => ApiError {
                code: ErrorCode::ParsingError,
                message: "JSON parsing error".to_string(),
                details: Some(format!("{}:{}: {}", line, column, message)),
            },

            // Security errors
            AppError::Security(ref msg) | AppError::KeyringError(ref msg) => ApiError {
                code: ErrorCode::SecurityViolation,
                message: "Security error".to_string(),
                details: Some(msg.clone()),
            },

            // Generic errors
            AppError::InvalidData(ref msg) => ApiError {
                code: ErrorCode::InvalidInput,
                message: "Invalid data".to_string(),
                details: Some(msg.clone()),
            },
            AppError::Storage(ref msg) => ApiError {
                code: ErrorCode::InternalError,
                message: "Storage error".to_string(),
                details: Some(msg.clone()),
            },
            AppError::InternalError(ref msg) | AppError::Other(ref msg) => ApiError {
                code: ErrorCode::InternalError,
                message: "Internal error".to_string(),
                details: Some(msg.clone()),
            },
        }
    }
}

/// Convert DomainError to ApiError
impl From<DomainError> for ApiError {
    fn from(err: DomainError) -> Self {
        match err {
            DomainError::EntityNotFound {
                ref entity_type,
                ref identifier,
            } => ApiError {
                code: ErrorCode::NotFound,
                message: format!("{} not found", entity_type),
                details: Some(format!("Identifier: {}", identifier)),
            },
            DomainError::InvalidState { ref details } => ApiError {
                code: ErrorCode::InvalidState,
                message: "Invalid state".to_string(),
                details: Some(details.clone()),
            },
            DomainError::ValidationFailed {
                ref field,
                ref reason,
            } => ApiError {
                code: ErrorCode::ValidationError,
                message: format!("Validation failed for field '{}'", field),
                details: Some(reason.clone()),
            },
            DomainError::UnsupportedFileType {
                ref path,
                ref detected_type,
            } => ApiError {
                code: ErrorCode::UnsupportedFileType,
                message: format!("Unsupported file type: {}", path.display()),
                details: detected_type.clone(),
            },
            DomainError::FileTooLarge {
                ref path,
                size_bytes,
                max_size_bytes,
            } => ApiError {
                code: ErrorCode::FileTooLarge,
                message: format!("File too large: {}", path.display()),
                details: Some(format!(
                    "Size: {} bytes, Max: {} bytes",
                    size_bytes, max_size_bytes
                )),
            },
            DomainError::ConcurrentModification {
                ref resource,
                ref details,
            } => ApiError {
                code: ErrorCode::ConcurrentModification,
                message: format!("Concurrent modification: {}", resource),
                details: Some(details.clone()),
            },
            DomainError::ConstraintViolation { ref constraint } => ApiError {
                code: ErrorCode::ConstraintViolation,
                message: "Constraint violation".to_string(),
                details: Some(constraint.clone()),
            },
        }
    }
}

/// Convert anyhow::Error to ApiError
impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        ApiError {
            code: ErrorCode::InternalError,
            message: "Internal error".to_string(),
            details: Some(format!("{:#}", err)),
        }
    }
}

/// Convert std::io::Error to ApiError
impl From<std::io::Error> for ApiError {
    fn from(err: std::io::Error) -> Self {
        use std::io::ErrorKind;

        let code = match err.kind() {
            ErrorKind::NotFound => ErrorCode::FileNotFound,
            ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
            ErrorKind::TimedOut => ErrorCode::Timeout,
            ErrorKind::InvalidInput | ErrorKind::InvalidData => ErrorCode::InvalidInput,
            _ => ErrorCode::FileSystemError,
        };

        ApiError {
            code,
            message: "I/O error".to_string(),
            details: Some(err.to_string()),
        }
    }
}

/// Convert sqlx::Error to ApiError
impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        use sqlx::Error;

        let (code, message) = match &err {
            Error::RowNotFound => (ErrorCode::NotFound, "Record not found"),
            Error::PoolTimedOut | Error::PoolClosed => (
                ErrorCode::DatabaseConnectionError,
                "Database connection error",
            ),
            Error::Configuration(_) => (ErrorCode::InvalidConfig, "Database configuration error"),
            _ => (ErrorCode::DatabaseError, "Database error"),
        };

        ApiError {
            code,
            message: message.to_string(),
            details: Some(sanitize_database_error(&err.to_string())),
        }
    }
}

/// Convert serde_json::Error to ApiError
impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        ApiError {
            code: ErrorCode::SerializationError,
            message: "JSON serialization error".to_string(),
            details: Some(err.to_string()),
        }
    }
}
