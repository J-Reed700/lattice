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
    let error = AppError::not_found("User", "123").with_context("While processing user request");
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
    assert!(!response.message.is_empty());
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
