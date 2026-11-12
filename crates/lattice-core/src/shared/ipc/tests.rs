//! Tests for `ApiResult` serialization and the error conversions.
use super::*;

#[test]
fn test_success_serialization() {
    let result = ApiResult::success(vec![1, 2, 3]);
    let json = serde_json::to_value(&result).unwrap();

    assert_eq!(json["ok"], true);
    assert_eq!(json["data"], serde_json::json!([1, 2, 3]));
}

#[test]
fn test_error_serialization() {
    let result = ApiResult::<()>::error(ErrorCode::NotFound, "Not found");
    let json = serde_json::to_value(&result).unwrap();

    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["code"], "NOT_FOUND");
    assert!(json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Not found"));
    assert!(json["error"]["details"].is_null());
}

#[test]
fn test_error_with_details_serialization() {
    let result = ApiResult::<()>::error_with_details(
        ErrorCode::ValidationError,
        "Invalid input",
        "Field 'email' is required",
    );
    let json = serde_json::to_value(&result).unwrap();

    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["code"], "VALIDATION_ERROR");
    assert!(json["error"]["details"].as_str().unwrap().contains("email"));
}

#[test]
fn test_from_result_ok() {
    let ok: Result<i32, AppError> = Ok(42);
    let result = ApiResult::from_result(ok);

    assert!(result.is_ok());
    if let ApiResult::Success { data, .. } = result {
        assert_eq!(data, 42);
    }
}

#[test]
fn test_from_result_err() {
    let err: Result<i32, DomainError> = Err(DomainError::EntityNotFound {
        entity_type: "Document".to_string(),
        identifier: "doc-123".to_string(),
    });
    let result = ApiResult::from_result(err);

    assert!(result.is_err());
    if let ApiResult::Error { error, .. } = result {
        assert_eq!(error.code, ErrorCode::NotFound);
    }
}

#[test]
fn test_domain_error_conversion() {
    let domain_err = DomainError::ValidationFailed {
        field: "email".to_string(),
        reason: "Invalid format".to_string(),
    };
    let api_err: ApiError = domain_err.into();

    assert_eq!(api_err.code, ErrorCode::ValidationError);
    assert!(api_err.message.contains("email"));
    assert!(api_err.details.is_some());
}

#[test]
fn test_io_error_conversion() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
    let api_err: ApiError = io_err.into();

    assert_eq!(api_err.code, ErrorCode::FileNotFound);
}

fn api_error_of(error: sqlx::Error) -> ApiError {
    crate::shared::persistence::PersistenceError::from(error).into()
}

#[test]
fn test_persistence_error_conversion() {
    let cases = [
        (
            sqlx::Error::RowNotFound,
            ErrorCode::NotFound,
            "Record not found",
        ),
        (
            sqlx::Error::PoolTimedOut,
            ErrorCode::DatabaseConnectionError,
            "Database connection error",
        ),
        (
            sqlx::Error::PoolClosed,
            ErrorCode::DatabaseConnectionError,
            "Database connection error",
        ),
        (
            sqlx::Error::Configuration("bad url".into()),
            ErrorCode::InvalidConfig,
            "Database configuration error",
        ),
        (
            sqlx::Error::WorkerCrashed,
            ErrorCode::DatabaseError,
            "Database error",
        ),
    ];
    for (error, code, message) in cases {
        let api_err = api_error_of(error);
        assert_eq!(api_err.code, code);
        assert_eq!(api_err.message, message);
        assert!(api_err.details.is_some());
    }
}

#[test]
fn test_sanitize_database_error() {
    let error = "error in SQL: SELECT * FROM users WHERE password = 'secret'";
    let sanitized = sanitize_database_error(error);

    assert!(!sanitized.contains("SELECT"));
    assert!(!sanitized.contains("secret"));
    assert!(!sanitized.contains("password"));
}

#[test]
fn test_error_code_serialization() {
    let code = ErrorCode::NotFound;
    let json = serde_json::to_string(&code).unwrap();
    assert_eq!(json, "\"NOT_FOUND\"");

    let code = ErrorCode::ValidationError;
    let json = serde_json::to_string(&code).unwrap();
    assert_eq!(json, "\"VALIDATION_ERROR\"");
}

#[test]
fn test_json_format_specification() {
    let success = ApiResult::success(vec![1, 2, 3]);
    let json = serde_json::to_string(&success).unwrap();
    assert!(json.contains(r#""ok":true"#));
    assert!(json.contains(r#""data""#));

    let error = ApiResult::<()>::error(ErrorCode::NotFound, "Not found");
    let json = serde_json::to_string(&error).unwrap();
    assert!(json.contains(r#""ok":false"#));
    assert!(json.contains(r#""error":"#));
    assert!(json.contains(r#""code":"NOT_FOUND""#));
    assert!(json.contains(r#""message""#));

    let error = ApiResult::<()>::error_with_details(
        ErrorCode::ValidationError,
        "Invalid input",
        "Field 'email' is required",
    );
    let json = serde_json::to_string(&error).unwrap();
    assert!(json.contains(r#""ok":false"#));
    assert!(json.contains(r#""code":"VALIDATION_ERROR""#));
    assert!(json.contains(r#""details":"Field 'email' is required""#));
}

#[test]
fn test_comprehensive_error_conversions() {
    let app_err = ApplicationError::ServiceNotAvailable {
        service: "TestService".to_string(),
        reason: "Initializing".to_string(),
    };
    let api_err: ApiError = app_err.into();
    assert_eq!(api_err.code, ErrorCode::ServiceNotAvailable);
    assert!(api_err.details.is_some());

    let domain_err = DomainError::ValidationFailed {
        field: "email".to_string(),
        reason: "Invalid format".to_string(),
    };
    let api_err: ApiError = domain_err.into();
    assert_eq!(api_err.code, ErrorCode::ValidationError);
    assert!(api_err.message.contains("email"));

    let app_error = AppError::Domain(Box::new(DomainError::EntityNotFound {
        entity_type: "Document".to_string(),
        identifier: "doc-123".to_string(),
    }));
    let api_err: ApiError = app_error.into();
    assert_eq!(api_err.code, ErrorCode::NotFound);
}

#[test]
fn a_prompt_that_fills_the_window_has_a_code_of_its_own() {
    let api_err: ApiError = AppError::PromptExceedsWindow {
        prompt_tokens: 8500,
        window: 8192,
        counted_exactly: true,
        message: "This request does not fit.".into(),
    }
    .into();
    assert_eq!(api_err.code, ErrorCode::PromptExceedsWindow);
    assert_eq!(api_err.message, "This request does not fit.");
    assert_eq!(
        serde_json::to_string(&api_err.code).unwrap(),
        "\"PROMPT_EXCEEDS_WINDOW\""
    );
}

#[test]
fn a_conflict_error_serializes_with_conflict_code() {
    let api_err: ApiError = AppError::Conflict("Revision conflict".into()).into();
    assert_eq!(api_err.code, ErrorCode::Conflict);
    assert_eq!(api_err.message, "Revision conflict");
    assert_eq!(
        serde_json::to_string(&api_err.code).unwrap(),
        "\"CONFLICT\""
    );
}

#[test]
fn a_conflict_reaches_the_frontend_as_an_error_with_code_conflict() {
    let api_err: ApiError = AppError::Conflict("Practice profile changed".into()).into();
    let sent = serde_json::to_value(&api_err).unwrap();
    assert_eq!(sent["code"], "CONFLICT");
    assert_eq!(sent["message"], "Practice profile changed");
    assert_eq!(
        ErrorCode::from(&AppError::Conflict(String::new())),
        ErrorCode::Conflict
    );
}

#[test]
fn an_unset_learning_model_has_a_code_of_its_own() {
    let sentence = "No Learning model is set. Choose one in Settings → Downloaded.";
    let api_err: ApiError = AppError::LearningModelNotSet(sentence.into()).into();
    assert_eq!(api_err.code, ErrorCode::LearningModelNotSet);
    assert_eq!(api_err.message, "No Learning model is set");
    assert_eq!(api_err.details.as_deref(), Some(sentence));
    assert_eq!(
        serde_json::to_string(&api_err.code).unwrap(),
        "\"LEARNING_MODEL_NOT_SET\""
    );
}

#[test]
fn an_invalid_input_refusal_carries_its_sentence_alone() {
    let sentence = "A question needs a prompt.";
    let api_err: ApiError = AppError::InvalidInput(sentence.into()).into();
    assert_eq!(api_err.code, ErrorCode::InvalidInput);
    assert_eq!(api_err.message, sentence);
}

#[test]
fn an_invalid_state_refusal_carries_its_sentence_alone() {
    let sentence = "This section changed since you opened it.";
    let api_err: ApiError = AppError::InvalidState(sentence.into()).into();
    assert_eq!(api_err.code, ErrorCode::InvalidState);
    assert_eq!(api_err.message, sentence);
}

#[test]
fn a_web_status_converts_to_its_own_code_with_its_message_as_the_message() {
    let error = AppError::HttpStatus {
        status: 403,
        message: "HTTP 403 Forbidden: Site blocked automated access.".to_string(),
    };

    let api = ApiError::from(error);

    assert_eq!(api.code, ErrorCode::HttpStatus);
    assert_eq!(
        api.message,
        "HTTP 403 Forbidden: Site blocked automated access."
    );
    assert_eq!(api.details, None);
    assert_eq!(
        serde_json::to_value(ErrorCode::HttpStatus).unwrap(),
        "HTTP_STATUS"
    );
}

/// A job's cause code, the renderer's errors and the outline's wait all come
/// from this one mapping.
#[test]
fn a_busy_database_converts_to_its_own_code() {
    let code = |error: AppError| serde_json::to_value(ApiError::from(error).code).unwrap();
    assert_eq!(
        code(AppError::DatabaseBusy(
            "The database is temporarily busy. Saved work is retained.".into()
        )),
        "DATABASE_BUSY"
    );
    assert_eq!(
        code(AppError::Database("no such table: x".into())),
        "DATABASE_ERROR"
    );
}

#[test]
fn each_model_and_lesson_error_has_a_code_of_its_own() {
    for (error, code) in [
        (
            AppError::WindowTooSmall("x".into()),
            ErrorCode::WindowTooSmall,
        ),
        (
            AppError::ModelRejected("x".into()),
            ErrorCode::ModelRejected,
        ),
        (
            AppError::ModelAnswerUnusable("x".into()),
            ErrorCode::ModelAnswerUnusable,
        ),
        (AppError::AnswerCutOff("x".into()), ErrorCode::AnswerCutOff),
        (AppError::Cancelled("x".into()), ErrorCode::Cancelled),
        (AppError::LessonHeld("x".into()), ErrorCode::LessonHeld),
    ] {
        assert_eq!(ErrorCode::from(&error), code, "{error}");
        assert_eq!(ApiError::from(error.clone()).code, code, "{error}");
    }
}

#[test]
fn the_code_of_an_api_error_is_the_code_of_its_source_error() {
    let errors = [
        AppError::NotFound("x".into()),
        AppError::DatabaseBusy("x".into()),
        AppError::ServiceNotAvailable("x".into()),
        AppError::ValidationFailed("x".into()),
        AppError::InternalError("x".into()),
        AppError::Domain(Box::new(DomainError::InvalidState {
            details: "x".into(),
        })),
        AppError::Domain(Box::new(DomainError::ConstraintViolation {
            constraint: "x".into(),
        })),
        AppError::Application(Box::new(ApplicationError::JsonError {
            message: "x".into(),
            line: 1,
            column: 2,
        })),
        AppError::Application(Box::new(ApplicationError::UseCaseFailed {
            use_case: "x".into(),
            reason: "y".into(),
        })),
    ];
    for error in errors {
        assert_eq!(
            ApiError::from(error.clone()).code,
            ErrorCode::from(&error),
            "{error}"
        );
    }
    assert_eq!(
        ErrorCode::from(&AppError::Domain(Box::new(DomainError::InvalidState {
            details: "x".into()
        }))),
        ErrorCode::InvalidState
    );
    assert_eq!(
        ErrorCode::from(&AppError::Application(Box::new(
            ApplicationError::UseCaseFailed {
                use_case: "x".into(),
                reason: "y".into()
            }
        ))),
        ErrorCode::ProcessingError
    );
}

#[test]
fn an_error_code_prints_as_it_serializes() {
    assert_eq!(ErrorCode::AnswerCutOff.to_string(), "ANSWER_CUT_OFF");
    for code in [
        ErrorCode::WindowTooSmall,
        ErrorCode::ModelRejected,
        ErrorCode::ModelAnswerUnusable,
        ErrorCode::AnswerCutOff,
        ErrorCode::Cancelled,
        ErrorCode::LessonHeld,
        ErrorCode::NotFound,
        ErrorCode::DatabaseBusy,
        ErrorCode::ServiceNotAvailable,
    ] {
        assert_eq!(
            serde_json::to_value(code).unwrap(),
            serde_json::Value::String(code.to_string())
        );
    }
}
