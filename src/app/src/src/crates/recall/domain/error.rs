//! # Domain Errors
//!
//! Domain-level errors representing business rule violations and entity state problems.
//!
//! ## Design Principles
//!
//! - **Business-Focused**: Errors reflect domain concepts, not technical details
//! - **User-Friendly**: Error messages are actionable and understandable
//! - **Serializable**: All errors can cross IPC boundaries (Tauri commands)
//! - **Typed**: Structured error variants for precise error handling
//!
//! ## Usage
//!
//! ```rust,no_run
//! use vault_desktop::domain::error::DomainError;
//!
//! fn validate_document(id: &str) -> Result<(), DomainError> {
//!     if id.is_empty() {
//!         return Err(DomainError::ValidationFailed {
//!             field: "id".to_string(),
//!             reason: "ID cannot be empty".to_string(),
//!         });
//!     }
//!     Ok(())
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

/// Domain errors representing business rule violations and entity state problems.
///
/// These errors are thrown by domain entities, aggregates, and services when
/// business invariants are violated or invalid operations are attempted.
#[derive(Debug, Clone, Error, Serialize, Deserialize, PartialEq, specta::Type)]
pub enum DomainError {
    /// Entity not found in the system.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use vault_desktop::domain::error::DomainError;
    /// let error = DomainError::EntityNotFound {
    ///     entity_type: "Document".to_string(),
    ///     identifier: "doc-123".to_string(),
    /// };
    /// assert!(error.to_string().contains("Document"));
    /// assert!(error.to_string().contains("doc-123"));
    /// ```
    #[error("Entity {entity_type} with identifier '{identifier}' not found")]
    EntityNotFound {
        entity_type: String,
        identifier: String,
    },

    /// Entity or aggregate is in an invalid state for the requested operation.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use vault_desktop::domain::error::DomainError;
    /// let error = DomainError::InvalidState {
    ///     details: "Document is archived and cannot be modified".to_string(),
    /// };
    /// assert!(error.to_string().contains("invalid state"));
    /// ```
    #[error("Invalid state: {details}")]
    InvalidState { details: String },

    /// Field validation failed.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use vault_desktop::domain::error::DomainError;
    /// let error = DomainError::ValidationFailed {
    ///     field: "email".to_string(),
    ///     reason: "Invalid email format".to_string(),
    /// };
    /// assert!(error.to_string().contains("email"));
    /// assert!(error.to_string().contains("Invalid email format"));
    /// ```
    #[error("Validation failed for field '{field}': {reason}")]
    ValidationFailed { field: String, reason: String },

    /// File type is not supported for indexing or processing.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use vault_desktop::domain::error::DomainError;
    /// # use std::path::PathBuf;
    /// let error = DomainError::UnsupportedFileType {
    ///     path: PathBuf::from("/path/to/file.xyz"),
    ///     detected_type: Some("application/octet-stream".to_string()),
    /// };
    /// assert!(error.to_string().contains("file.xyz"));
    /// ```
    #[error("Unsupported file type at {path:?} (detected: {detected_type:?})")]
    UnsupportedFileType {
        path: PathBuf,
        detected_type: Option<String>,
    },

    /// File size exceeds maximum allowed size.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use vault_desktop::domain::error::DomainError;
    /// # use std::path::PathBuf;
    /// let error = DomainError::FileTooLarge {
    ///     path: PathBuf::from("/path/to/huge.pdf"),
    ///     size_bytes: 100_000_000,
    ///     max_size_bytes: 50_000_000,
    /// };
    /// assert!(error.to_string().contains("100000000 bytes"));
    /// assert!(error.to_string().contains("50000000 bytes"));
    /// ```
    #[error("File at {path:?} is too large ({size_bytes} bytes, max: {max_size_bytes} bytes)")]
    FileTooLarge {
        path: PathBuf,
        size_bytes: u64,
        max_size_bytes: u64,
    },

    /// Concurrent modification detected (optimistic locking failure).
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use vault_desktop::domain::error::DomainError;
    /// let error = DomainError::ConcurrentModification {
    ///     resource: "Document doc-123".to_string(),
    ///     details: "Version mismatch: expected v2, found v3".to_string(),
    /// };
    /// assert!(error.to_string().contains("doc-123"));
    /// assert!(error.to_string().contains("Version mismatch"));
    /// ```
    #[error("Concurrent modification of {resource}: {details}")]
    ConcurrentModification { resource: String, details: String },

    /// Domain constraint or business rule violated.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use vault_desktop::domain::error::DomainError;
    /// let error = DomainError::ConstraintViolation {
    ///     constraint: "A document must have at least one chunk".to_string(),
    /// };
    /// assert!(error.to_string().contains("at least one chunk"));
    /// ```
    #[error("Constraint violation: {constraint}")]
    ConstraintViolation { constraint: String },
}

impl DomainError {
    /// Returns true if this error represents a "not found" condition.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use vault_desktop::domain::error::DomainError;
    /// let not_found = DomainError::EntityNotFound {
    ///     entity_type: "Document".to_string(),
    ///     identifier: "doc-123".to_string(),
    /// };
    /// assert!(not_found.is_not_found());
    ///
    /// let validation = DomainError::ValidationFailed {
    ///     field: "name".to_string(),
    ///     reason: "Required".to_string(),
    /// };
    /// assert!(!validation.is_not_found());
    /// ```
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::EntityNotFound { .. })
    }

    /// Returns true if this error represents a business rule violation.
    ///
    /// Business rule violations include validation failures, constraint violations,
    /// and invalid state transitions.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use vault_desktop::domain::error::DomainError;
    /// let validation = DomainError::ValidationFailed {
    ///     field: "email".to_string(),
    ///     reason: "Invalid format".to_string(),
    /// };
    /// assert!(validation.is_business_rule_violation());
    ///
    /// let constraint = DomainError::ConstraintViolation {
    ///     constraint: "Unique email required".to_string(),
    /// };
    /// assert!(constraint.is_business_rule_violation());
    ///
    /// let not_found = DomainError::EntityNotFound {
    ///     entity_type: "User".to_string(),
    ///     identifier: "user-123".to_string(),
    /// };
    /// assert!(!not_found.is_business_rule_violation());
    /// ```
    pub fn is_business_rule_violation(&self) -> bool {
        matches!(
            self,
            Self::ValidationFailed { .. }
                | Self::ConstraintViolation { .. }
                | Self::InvalidState { .. }
        )
    }

    /// Returns true if this error condition might be resolved by retrying.
    ///
    /// Currently, only concurrent modification errors are considered retryable.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use vault_desktop::domain::error::DomainError;
    /// let concurrent = DomainError::ConcurrentModification {
    ///     resource: "Document".to_string(),
    ///     details: "Version conflict".to_string(),
    /// };
    /// assert!(concurrent.is_retryable());
    ///
    /// let validation = DomainError::ValidationFailed {
    ///     field: "email".to_string(),
    ///     reason: "Invalid".to_string(),
    /// };
    /// assert!(!validation.is_retryable());
    /// ```
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::ConcurrentModification { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_not_found() {
        let error = DomainError::EntityNotFound {
            entity_type: "Document".to_string(),
            identifier: "doc-123".to_string(),
        };

        assert!(error.is_not_found());
        assert!(!error.is_business_rule_violation());
        assert!(!error.is_retryable());

        let msg = error.to_string();
        assert!(msg.contains("Document"));
        assert!(msg.contains("doc-123"));
    }

    #[test]
    fn test_invalid_state() {
        let error = DomainError::InvalidState {
            details: "Document is archived".to_string(),
        };

        assert!(!error.is_not_found());
        assert!(error.is_business_rule_violation());
        assert!(!error.is_retryable());

        let msg = error.to_string();
        assert!(msg.contains("Invalid state"));
        assert!(msg.contains("archived"));
    }

    #[test]
    fn test_validation_failed() {
        let error = DomainError::ValidationFailed {
            field: "email".to_string(),
            reason: "Invalid format".to_string(),
        };

        assert!(!error.is_not_found());
        assert!(error.is_business_rule_violation());
        assert!(!error.is_retryable());

        let msg = error.to_string();
        assert!(msg.contains("email"));
        assert!(msg.contains("Invalid format"));
    }

    #[test]
    fn test_unsupported_file_type() {
        let error = DomainError::UnsupportedFileType {
            path: PathBuf::from("/test/file.xyz"),
            detected_type: Some("application/octet-stream".to_string()),
        };

        assert!(!error.is_not_found());
        assert!(!error.is_business_rule_violation());
        assert!(!error.is_retryable());

        let msg = error.to_string();
        assert!(msg.contains("file.xyz"));
    }

    #[test]
    fn test_file_too_large() {
        let error = DomainError::FileTooLarge {
            path: PathBuf::from("/test/huge.pdf"),
            size_bytes: 100_000_000,
            max_size_bytes: 50_000_000,
        };

        assert!(!error.is_not_found());
        assert!(!error.is_business_rule_violation());
        assert!(!error.is_retryable());

        let msg = error.to_string();
        assert!(msg.contains("100000000"));
        assert!(msg.contains("50000000"));
    }

    #[test]
    fn test_concurrent_modification() {
        let error = DomainError::ConcurrentModification {
            resource: "Document doc-123".to_string(),
            details: "Version mismatch".to_string(),
        };

        assert!(!error.is_not_found());
        assert!(!error.is_business_rule_violation());
        assert!(error.is_retryable());

        let msg = error.to_string();
        assert!(msg.contains("doc-123"));
        assert!(msg.contains("Version mismatch"));
    }

    #[test]
    fn test_constraint_violation() {
        let error = DomainError::ConstraintViolation {
            constraint: "Document must have at least one chunk".to_string(),
        };

        assert!(!error.is_not_found());
        assert!(error.is_business_rule_violation());
        assert!(!error.is_retryable());

        let msg = error.to_string();
        assert!(msg.contains("at least one chunk"));
    }

    #[test]
    fn test_serialization_roundtrip() {
        let errors = vec![
            DomainError::EntityNotFound {
                entity_type: "Document".to_string(),
                identifier: "doc-123".to_string(),
            },
            DomainError::ValidationFailed {
                field: "email".to_string(),
                reason: "Invalid".to_string(),
            },
            DomainError::ConcurrentModification {
                resource: "Document".to_string(),
                details: "Version conflict".to_string(),
            },
        ];

        for error in errors {
            let json = serde_json::to_string(&error).expect("serialization failed");
            let deserialized: DomainError =
                serde_json::from_str(&json).expect("deserialization failed");
            assert_eq!(error, deserialized);
        }
    }

    #[test]
    fn test_clone() {
        let error = DomainError::ValidationFailed {
            field: "name".to_string(),
            reason: "Required".to_string(),
        };

        let cloned = error.clone();
        assert_eq!(error, cloned);
    }

    #[test]
    fn test_debug_format() {
        let error = DomainError::EntityNotFound {
            entity_type: "Document".to_string(),
            identifier: "doc-123".to_string(),
        };

        let debug = format!("{:?}", error);
        assert!(debug.contains("EntityNotFound"));
        assert!(debug.contains("Document"));
        assert!(debug.contains("doc-123"));
    }

    #[test]
    fn test_helper_methods_comprehensive() {
        // Test all error types with all helper methods
        let test_cases = vec![
            (
                DomainError::EntityNotFound {
                    entity_type: "Document".to_string(),
                    identifier: "doc-123".to_string(),
                },
                (true, false, false), // (is_not_found, is_business_rule, is_retryable)
            ),
            (
                DomainError::InvalidState {
                    details: "Archived".to_string(),
                },
                (false, true, false),
            ),
            (
                DomainError::ValidationFailed {
                    field: "email".to_string(),
                    reason: "Invalid".to_string(),
                },
                (false, true, false),
            ),
            (
                DomainError::UnsupportedFileType {
                    path: PathBuf::from("/test.xyz"),
                    detected_type: None,
                },
                (false, false, false),
            ),
            (
                DomainError::FileTooLarge {
                    path: PathBuf::from("/test.pdf"),
                    size_bytes: 1000,
                    max_size_bytes: 500,
                },
                (false, false, false),
            ),
            (
                DomainError::ConcurrentModification {
                    resource: "Doc".to_string(),
                    details: "Conflict".to_string(),
                },
                (false, false, true),
            ),
            (
                DomainError::ConstraintViolation {
                    constraint: "Unique".to_string(),
                },
                (false, true, false),
            ),
        ];

        for (error, (expected_not_found, expected_business, expected_retryable)) in test_cases {
            assert_eq!(error.is_not_found(), expected_not_found);
            assert_eq!(error.is_business_rule_violation(), expected_business);
            assert_eq!(error.is_retryable(), expected_retryable);
        }
    }
}
