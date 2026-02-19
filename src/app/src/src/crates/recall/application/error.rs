//! # Application Errors
//!
//! Application-level errors for use cases, DTOs, and orchestration logic.
//!
//! ## Design Principles
//!
//! - **Layer Separation**: Application errors wrap domain errors
//! - **Use Case Focused**: Errors reflect application workflows
//! - **IPC Friendly**: All errors serialize for Tauri commands
//! - **Recoverable**: Clear indication of recoverable vs. permanent failures
//!
//! ## Usage
//!
//! ```rust,no_run
//! use vault_desktop::application::error::ApplicationError;
//! use vault_desktop::domain::error::DomainError;
//!
//! fn validate_config(data: &str) -> Result<(), ApplicationError> {
//!     if data.is_empty() {
//!         return Err(ApplicationError::InvalidConfig {
//!             details: "Configuration cannot be empty".to_string(),
//!         });
//!     }
//!     Ok(())
//! }
//!
//! fn handle_domain_error() -> Result<(), ApplicationError> {
//!     let domain_err = DomainError::EntityNotFound {
//!         entity_type: "Document".to_string(),
//!         identifier: "doc-123".to_string(),
//!     };
//!     Err(domain_err.into()) // Automatic conversion via #[from]
//! }
//! ```

use crate::domain::error::DomainError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Application errors for use cases, DTOs, and orchestration logic.
///
/// These errors represent failures in application-level operations such as
/// configuration, serialization, service availability, and use case execution.
#[derive(Debug, Clone, Error, Serialize, Deserialize, specta::Type)]
pub enum ApplicationError {
    /// Domain error (business logic violation).
    ///
    /// Wraps domain-level errors from aggregates, entities, and domain services.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use vault_desktop::application::error::ApplicationError;
    /// # use vault_desktop::domain::error::DomainError;
    /// let domain_err = DomainError::EntityNotFound {
    ///     entity_type: "Document".to_string(),
    ///     identifier: "doc-123".to_string(),
    /// };
    /// let app_err = ApplicationError::from(domain_err);
    /// assert!(app_err.is_domain_error());
    /// ```
    #[error("Domain error: {0}")]
    Domain(#[from] DomainError),

    /// Invalid configuration.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use vault_desktop::application::error::ApplicationError;
    /// let error = ApplicationError::InvalidConfig {
    ///     details: "Missing required field 'api_key'".to_string(),
    /// };
    /// assert!(error.to_string().contains("api_key"));
    /// assert!(error.is_config_error());
    /// ```
    #[error("Invalid configuration: {details}")]
    InvalidConfig { details: String },

    /// Required service is not available.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use vault_desktop::application::error::ApplicationError;
    /// let error = ApplicationError::ServiceNotAvailable {
    ///     service: "EmbeddingService".to_string(),
    ///     reason: "Model not loaded".to_string(),
    /// };
    /// assert!(error.to_string().contains("EmbeddingService"));
    /// assert!(!error.is_config_error());
    /// ```
    #[error("Service '{service}' not available: {reason}")]
    ServiceNotAvailable { service: String, reason: String },

    /// Processing queue is full (backpressure).
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use vault_desktop::application::error::ApplicationError;
    /// let error = ApplicationError::QueueFull {
    ///     queue_name: "indexing_queue".to_string(),
    /// };
    /// assert!(error.to_string().contains("indexing_queue"));
    /// assert!(error.is_recoverable());
    /// ```
    #[error("Queue '{queue_name}' is full")]
    QueueFull { queue_name: String },

    /// Serialization failed.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use vault_desktop::application::error::ApplicationError;
    /// let error = ApplicationError::SerializationFailed {
    ///     context: "DocumentDTO".to_string(),
    ///     reason: "Invalid UTF-8 in field 'content'".to_string(),
    /// };
    /// assert!(error.to_string().contains("DocumentDTO"));
    /// ```
    #[error("Serialization failed for {context}: {reason}")]
    SerializationFailed { context: String, reason: String },

    /// Deserialization failed.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use vault_desktop::application::error::ApplicationError;
    /// let error = ApplicationError::DeserializationFailed {
    ///     context: "SearchRequest".to_string(),
    ///     reason: "Missing required field 'query'".to_string(),
    /// };
    /// assert!(error.to_string().contains("SearchRequest"));
    /// ```
    #[error("Deserialization failed for {context}: {reason}")]
    DeserializationFailed { context: String, reason: String },

    /// JSON parsing error with location.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use vault_desktop::application::error::ApplicationError;
    /// let error = ApplicationError::JsonError {
    ///     message: "Expected comma".to_string(),
    ///     line: 42,
    ///     column: 15,
    /// };
    /// assert!(error.to_string().contains("line 42"));
    /// assert!(error.to_string().contains("column 15"));
    /// ```
    #[error("JSON error at line {line}, column {column}: {message}")]
    JsonError {
        message: String,
        line: usize,
        column: usize,
    },

    /// Generic parsing error.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use vault_desktop::application::error::ApplicationError;
    /// let error = ApplicationError::ParsingError {
    ///     context: "CSV file".to_string(),
    ///     reason: "Unexpected end of file".to_string(),
    /// };
    /// assert!(error.to_string().contains("CSV file"));
    /// ```
    #[error("Parsing error in {context}: {reason}")]
    ParsingError { context: String, reason: String },

    /// Use case execution failed.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use vault_desktop::application::error::ApplicationError;
    /// let error = ApplicationError::UseCaseFailed {
    ///     use_case: "IndexDocumentUseCase".to_string(),
    ///     reason: "Embedding generation timed out".to_string(),
    /// };
    /// assert!(error.to_string().contains("IndexDocumentUseCase"));
    /// ```
    #[error("Use case '{use_case}' failed: {reason}")]
    UseCaseFailed { use_case: String, reason: String },

    /// DTO mapping failed.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use vault_desktop::application::error::ApplicationError;
    /// let error = ApplicationError::MappingError {
    ///     context: "Document -> DocumentDTO".to_string(),
    ///     reason: "Missing required field".to_string(),
    /// };
    /// assert!(error.to_string().contains("Document -> DocumentDTO"));
    /// ```
    #[error("Mapping error in {context}: {reason}")]
    MappingError { context: String, reason: String },
}

impl ApplicationError {
    /// Returns true if this error wraps a domain error.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use vault_desktop::application::error::ApplicationError;
    /// # use vault_desktop::domain::error::DomainError;
    /// let domain = ApplicationError::from(DomainError::EntityNotFound {
    ///     entity_type: "Document".to_string(),
    ///     identifier: "doc-123".to_string(),
    /// });
    /// assert!(domain.is_domain_error());
    ///
    /// let config = ApplicationError::InvalidConfig {
    ///     details: "Invalid".to_string(),
    /// };
    /// assert!(!config.is_domain_error());
    /// ```
    pub fn is_domain_error(&self) -> bool {
        matches!(self, Self::Domain(_))
    }

    /// Extracts the wrapped domain error, if any.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use vault_desktop::application::error::ApplicationError;
    /// # use vault_desktop::domain::error::DomainError;
    /// let domain_err = DomainError::EntityNotFound {
    ///     entity_type: "Document".to_string(),
    ///     identifier: "doc-123".to_string(),
    /// };
    /// let app_err = ApplicationError::from(domain_err.clone());
    ///
    /// assert_eq!(app_err.domain_error(), Some(&domain_err));
    ///
    /// let config_err = ApplicationError::InvalidConfig {
    ///     details: "Invalid".to_string(),
    /// };
    /// assert_eq!(config_err.domain_error(), None);
    /// ```
    pub fn domain_error(&self) -> Option<&DomainError> {
        match self {
            Self::Domain(err) => Some(err),
            _ => None,
        }
    }

    /// Returns true if this error is potentially recoverable by retrying.
    ///
    /// Recoverable errors include:
    /// - Queue full (backpressure)
    /// - Service not available (transient)
    /// - Domain errors marked as retryable (e.g., concurrent modification)
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use vault_desktop::application::error::ApplicationError;
    /// # use vault_desktop::domain::error::DomainError;
    /// let queue_full = ApplicationError::QueueFull {
    ///     queue_name: "indexing".to_string(),
    /// };
    /// assert!(queue_full.is_recoverable());
    ///
    /// let service_down = ApplicationError::ServiceNotAvailable {
    ///     service: "Embedding".to_string(),
    ///     reason: "Loading".to_string(),
    /// };
    /// assert!(service_down.is_recoverable());
    ///
    /// let concurrent = ApplicationError::from(DomainError::ConcurrentModification {
    ///     resource: "Document".to_string(),
    ///     details: "Version conflict".to_string(),
    /// });
    /// assert!(concurrent.is_recoverable());
    ///
    /// let invalid_config = ApplicationError::InvalidConfig {
    ///     details: "Missing API key".to_string(),
    /// };
    /// assert!(!invalid_config.is_recoverable());
    /// ```
    pub fn is_recoverable(&self) -> bool {
        match self {
            Self::QueueFull { .. } | Self::ServiceNotAvailable { .. } => true,
            Self::Domain(err) => err.is_retryable(),
            _ => false,
        }
    }

    /// Returns true if this is a configuration error.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use vault_desktop::application::error::ApplicationError;
    /// let config = ApplicationError::InvalidConfig {
    ///     details: "Missing field".to_string(),
    /// };
    /// assert!(config.is_config_error());
    ///
    /// let queue = ApplicationError::QueueFull {
    ///     queue_name: "indexing".to_string(),
    /// };
    /// assert!(!queue.is_config_error());
    /// ```
    pub fn is_config_error(&self) -> bool {
        matches!(self, Self::InvalidConfig { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_error_wrapping() {
        let domain_err = DomainError::EntityNotFound {
            entity_type: "Document".to_string(),
            identifier: "doc-123".to_string(),
        };

        let app_err = ApplicationError::from(domain_err.clone());

        assert!(app_err.is_domain_error());
        assert_eq!(app_err.domain_error(), Some(&domain_err));

        let msg = app_err.to_string();
        assert!(msg.contains("Domain error"));
        assert!(msg.contains("Document"));
    }

    #[test]
    fn test_invalid_config() {
        let error = ApplicationError::InvalidConfig {
            details: "Missing API key".to_string(),
        };

        assert!(!error.is_domain_error());
        assert!(error.is_config_error());
        assert!(!error.is_recoverable());

        let msg = error.to_string();
        assert!(msg.contains("Invalid configuration"));
        assert!(msg.contains("API key"));
    }

    #[test]
    fn test_service_not_available() {
        let error = ApplicationError::ServiceNotAvailable {
            service: "EmbeddingService".to_string(),
            reason: "Model not loaded".to_string(),
        };

        assert!(!error.is_domain_error());
        assert!(!error.is_config_error());
        assert!(error.is_recoverable());

        let msg = error.to_string();
        assert!(msg.contains("EmbeddingService"));
        assert!(msg.contains("not loaded"));
    }

    #[test]
    fn test_queue_full() {
        let error = ApplicationError::QueueFull {
            queue_name: "indexing_queue".to_string(),
        };

        assert!(!error.is_domain_error());
        assert!(error.is_recoverable());

        let msg = error.to_string();
        assert!(msg.contains("indexing_queue"));
        assert!(msg.contains("full"));
    }

    #[test]
    fn test_serialization_failed() {
        let error = ApplicationError::SerializationFailed {
            context: "DocumentDTO".to_string(),
            reason: "Invalid UTF-8".to_string(),
        };

        assert!(!error.is_domain_error());
        assert!(!error.is_recoverable());

        let msg = error.to_string();
        assert!(msg.contains("DocumentDTO"));
        assert!(msg.contains("Invalid UTF-8"));
    }

    #[test]
    fn test_deserialization_failed() {
        let error = ApplicationError::DeserializationFailed {
            context: "SearchRequest".to_string(),
            reason: "Missing field 'query'".to_string(),
        };

        assert!(!error.is_domain_error());
        assert!(!error.is_recoverable());

        let msg = error.to_string();
        assert!(msg.contains("SearchRequest"));
        assert!(msg.contains("Missing field"));
    }

    #[test]
    fn test_json_error() {
        let error = ApplicationError::JsonError {
            message: "Expected comma".to_string(),
            line: 42,
            column: 15,
        };

        assert!(!error.is_domain_error());
        assert!(!error.is_recoverable());

        let msg = error.to_string();
        assert!(msg.contains("line 42"));
        assert!(msg.contains("column 15"));
        assert!(msg.contains("Expected comma"));
    }

    #[test]
    fn test_parsing_error() {
        let error = ApplicationError::ParsingError {
            context: "CSV file".to_string(),
            reason: "Unexpected end".to_string(),
        };

        assert!(!error.is_domain_error());
        assert!(!error.is_recoverable());

        let msg = error.to_string();
        assert!(msg.contains("CSV file"));
        assert!(msg.contains("Unexpected end"));
    }

    #[test]
    fn test_use_case_failed() {
        let error = ApplicationError::UseCaseFailed {
            use_case: "IndexDocumentUseCase".to_string(),
            reason: "Timeout".to_string(),
        };

        assert!(!error.is_domain_error());
        assert!(!error.is_recoverable());

        let msg = error.to_string();
        assert!(msg.contains("IndexDocumentUseCase"));
        assert!(msg.contains("Timeout"));
    }

    #[test]
    fn test_mapping_error() {
        let error = ApplicationError::MappingError {
            context: "Document -> DTO".to_string(),
            reason: "Missing field".to_string(),
        };

        assert!(!error.is_domain_error());
        assert!(!error.is_recoverable());

        let msg = error.to_string();
        assert!(msg.contains("Document -> DTO"));
        assert!(msg.contains("Missing field"));
    }

    #[test]
    fn test_recoverable_domain_error() {
        let concurrent = DomainError::ConcurrentModification {
            resource: "Document".to_string(),
            details: "Version conflict".to_string(),
        };

        let app_err = ApplicationError::from(concurrent);
        assert!(app_err.is_recoverable());
    }

    #[test]
    fn test_non_recoverable_domain_error() {
        let validation = DomainError::ValidationFailed {
            field: "email".to_string(),
            reason: "Invalid format".to_string(),
        };

        let app_err = ApplicationError::from(validation);
        assert!(!app_err.is_recoverable());
    }

    #[test]
    fn test_serialization_roundtrip() {
        let errors = vec![
            ApplicationError::InvalidConfig {
                details: "Test".to_string(),
            },
            ApplicationError::QueueFull {
                queue_name: "test_queue".to_string(),
            },
            ApplicationError::from(DomainError::EntityNotFound {
                entity_type: "Test".to_string(),
                identifier: "test-123".to_string(),
            }),
        ];

        for error in errors {
            let json = serde_json::to_string(&error).expect("serialization failed");
            let deserialized: ApplicationError =
                serde_json::from_str(&json).expect("deserialization failed");

            assert_eq!(error.to_string(), deserialized.to_string());
            assert_eq!(error.is_domain_error(), deserialized.is_domain_error());
            assert_eq!(error.is_recoverable(), deserialized.is_recoverable());
        }
    }

    #[test]
    fn test_clone() {
        let error = ApplicationError::InvalidConfig {
            details: "Test".to_string(),
        };

        let cloned = error.clone();
        assert_eq!(error.to_string(), cloned.to_string());
    }

    #[test]
    fn test_debug_format() {
        let error = ApplicationError::ServiceNotAvailable {
            service: "Test".to_string(),
            reason: "Down".to_string(),
        };

        let debug = format!("{:?}", error);
        assert!(debug.contains("ServiceNotAvailable"));
        assert!(debug.contains("Test"));
    }

    #[test]
    fn test_helper_methods_comprehensive() {
        let test_cases = vec![
            (
                ApplicationError::InvalidConfig {
                    details: "Test".to_string(),
                },
                (false, true, false), // (is_domain, is_config, is_recoverable)
            ),
            (
                ApplicationError::ServiceNotAvailable {
                    service: "Test".to_string(),
                    reason: "Down".to_string(),
                },
                (false, false, true),
            ),
            (
                ApplicationError::QueueFull {
                    queue_name: "test".to_string(),
                },
                (false, false, true),
            ),
            (
                ApplicationError::from(DomainError::EntityNotFound {
                    entity_type: "Test".to_string(),
                    identifier: "123".to_string(),
                }),
                (true, false, false),
            ),
        ];

        for (error, (expected_domain, expected_config, expected_recoverable)) in test_cases {
            assert_eq!(error.is_domain_error(), expected_domain);
            assert_eq!(error.is_config_error(), expected_config);
            assert_eq!(error.is_recoverable(), expected_recoverable);
        }
    }
}
