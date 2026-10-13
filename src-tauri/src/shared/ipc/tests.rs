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

#[test]
fn test_sqlx_error_conversion() {
    let sqlx_err = sqlx::Error::RowNotFound;
    let api_err: ApiError = sqlx_err.into();

    assert_eq!(api_err.code, ErrorCode::NotFound);
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
