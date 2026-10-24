//! Application errors for use cases, DTOs, and orchestration logic.

use super::DomainError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Application errors for use cases, DTOs, and orchestration logic.
#[derive(Debug, Clone, Error, Serialize, Deserialize, specta::Type)]
pub enum ApplicationError {
    #[error("Domain error: {0}")]
    Domain(#[from] DomainError),

    #[error("Invalid configuration: {details}")]
    InvalidConfig { details: String },

    #[error("Service '{service}' not available: {reason}")]
    ServiceNotAvailable { service: String, reason: String },

    #[error("Queue '{queue_name}' is full")]
    QueueFull { queue_name: String },

    #[error("Serialization failed for {context}: {reason}")]
    SerializationFailed { context: String, reason: String },

    #[error("Deserialization failed for {context}: {reason}")]
    DeserializationFailed { context: String, reason: String },

    #[error("JSON error at line {line}, column {column}: {message}")]
    JsonError {
        message: String,
        line: usize,
        column: usize,
    },

    #[error("Parsing error in {context}: {reason}")]
    ParsingError { context: String, reason: String },

    #[error("Use case '{use_case}' failed: {reason}")]
    UseCaseFailed { use_case: String, reason: String },

    #[error("Mapping error in {context}: {reason}")]
    MappingError { context: String, reason: String },
}

impl ApplicationError {
    /// Returns true if this error wraps a domain error.
    pub fn is_domain_error(&self) -> bool {
        matches!(self, Self::Domain(_))
    }

    /// Extracts the wrapped domain error, if any.
    pub fn domain_error(&self) -> Option<&DomainError> {
        match self {
            Self::Domain(err) => Some(err),
            _ => None,
        }
    }

    /// Returns true if this error is potentially recoverable by retrying.
    pub fn is_recoverable(&self) -> bool {
        match self {
            Self::QueueFull { .. } | Self::ServiceNotAvailable { .. } => true,
            Self::Domain(err) => err.is_retryable(),
            _ => false,
        }
    }

    /// Returns true if this is a configuration error.
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
