use super::*;
use crate::shared::ipc::ErrorCode;

#[test]
fn test_builder_methods() {
    let not_found = AppError::not_found("Document", "doc-123");
    assert!(not_found.to_string().contains("Document"));
    assert!(not_found.to_string().contains("doc-123"));
    assert_eq!(ErrorCode::from(&not_found), ErrorCode::NotFound);

    let validation = AppError::validation_failed("email", "Invalid format");
    assert!(validation.to_string().contains("email"));
    assert!(validation.to_string().contains("Invalid format"));
    assert_eq!(ErrorCode::from(&validation), ErrorCode::ValidationError);

    let rate_limit = AppError::rate_limited(Some(60));
    assert!(rate_limit.to_string().contains("60 seconds"));
    assert_eq!(ErrorCode::from(&rate_limit), ErrorCode::RateLimitExceeded);
}

#[test]
fn a_network_error_and_an_invalid_input_say_what_happened_and_nothing_else() {
    assert_eq!(
        AppError::Network("x".to_string()).to_string(),
        "Network error: x"
    );
    assert_eq!(
        AppError::InvalidInput("x".to_string()).to_string(),
        "Invalid input: x"
    );
}

#[test]
fn a_web_status_displays_its_message_alone() {
    let error = AppError::HttpStatus {
        status: 403,
        message: "HTTP 403 Forbidden: Site blocked automated access.".to_string(),
    };
    assert_eq!(
        error.to_string(),
        "HTTP 403 Forbidden: Site blocked automated access."
    );
    assert_eq!(ErrorCode::from(&error), ErrorCode::HttpStatus);
}

#[test]
fn test_error_codes() {
    assert_eq!(
        ErrorCode::from(&AppError::Database("test".to_string())),
        ErrorCode::DatabaseError
    );
    assert_eq!(
        ErrorCode::from(&AppError::Network("test".to_string())),
        ErrorCode::NetworkError
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
fn test_domain_error_integration() {
    let domain_err = DomainError::EntityNotFound {
        entity_type: "Document".to_string(),
        identifier: "doc-123".to_string(),
    };

    let app_err: AppError = domain_err.into();
    assert_eq!(ErrorCode::from(&app_err), ErrorCode::NotFound);
}

#[test]
fn the_error_kinds_the_classes_name_say_what_the_learner_already_read() {
    let window = AppError::WindowTooSmall("window".to_string());
    assert_eq!(
        window.to_string(),
        AppError::InvalidConfig("window".to_string()).to_string()
    );
    assert_eq!(
        AppError::Cancelled("Stopped.".to_string()).to_string(),
        "Stopped."
    );
    assert_eq!(
        AppError::LessonHeld("Held.".to_string()).to_string(),
        "Held."
    );
    assert_eq!(
        AppError::DatabaseBusy("locked".to_string()).to_string(),
        "Database busy: locked"
    );
}

#[test]
fn test_is_fatal() {
    assert!(AppError::Database("error".to_string()).is_fatal());
    assert!(AppError::Migration("error".to_string()).is_fatal());
    assert!(AppError::BackupCorrupted("error".to_string()).is_fatal());
    assert!(!AppError::Network("error".to_string()).is_fatal());
}
