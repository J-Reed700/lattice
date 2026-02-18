//! Gateway Helper Utilities
//!
//! Provides conversion utilities for transforming domain/application layer
//! results into gateway-friendly ApiResult types.

use crate::shared::api_result::{ApiError, ApiResult};

/// Extension trait for converting Results into ApiResult
///
/// Provides ergonomic `.into_api_result()` method for any Result
/// where the error type can be converted to ApiError.
///
/// # Examples
///
/// ```rust
/// use crate::shared::gateway_helpers::IntoApiResult;
/// use crate::domain::error::DomainError;
///
/// fn some_use_case() -> Result<Vec<String>, DomainError> {
///     // domain logic
/// }
///
/// // In gateway command:
/// let result = some_use_case().await.into_api_result();
/// // Returns ApiResult<Vec<String>>
/// ```
pub trait IntoApiResult<T> {
    fn into_api_result(self) -> ApiResult<T>;
}

/// Blanket implementation for all Results where E converts to ApiError
impl<T, E> IntoApiResult<T> for Result<T, E>
where
    E: Into<ApiError>,
{
    fn into_api_result(self) -> ApiResult<T> {
        ApiResult::from_result(self)
    }
}

/// Free function variant for explicit conversion
///
/// Use when method syntax is not ergonomic (e.g., in match arms).
///
/// # Examples
///
/// ```rust
/// use crate::shared::gateway_helpers::into_api_result;
///
/// match some_operation().await {
///     Ok(data) => into_api_result(Ok(data)),
///     Err(e) => into_api_result(Err(e)),
/// }
/// ```
pub fn into_api_result<T, E>(result: Result<T, E>) -> ApiResult<T>
where
    E: Into<ApiError>,
{
    ApiResult::from_result(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::error::ApplicationError;
    use crate::domain::error::DomainError;
    use crate::shared::api_result::ErrorCode;

    #[test]
    fn test_domain_error_conversion() {
        let result: Result<String, DomainError> = Err(DomainError::EntityNotFound {
            entity_type: "Model".to_string(),
            identifier: "test-model".to_string(),
        });

        let api_result = result.into_api_result();

        assert!(api_result.is_err());
        match api_result {
            ApiResult::Error { error, .. } => {
                assert_eq!(error.code, ErrorCode::NotFound);
                assert!(error.message.contains("Model"));
            }
            _ => panic!("Expected error"),
        }
    }

    #[test]
    fn test_application_error_conversion() {
        let result: Result<i32, ApplicationError> = Err(ApplicationError::ServiceNotAvailable {
            service: "DownloadService".to_string(),
            reason: "Initializing".to_string(),
        });

        let api_result = result.into_api_result();

        assert!(api_result.is_err());
        match api_result {
            ApiResult::Error { error, .. } => {
                assert_eq!(error.code, ErrorCode::ServiceNotAvailable);
            }
            _ => panic!("Expected error"),
        }
    }

    #[test]
    fn test_success_conversion() {
        let result: Result<Vec<String>, DomainError> = Ok(vec!["test".to_string()]);

        let api_result = result.into_api_result();

        assert!(api_result.is_ok());
        match api_result {
            ApiResult::Success { data, .. } => {
                assert_eq!(data, vec!["test"]);
            }
            _ => panic!("Expected success"),
        }
    }

    #[test]
    fn test_anyhow_error_conversion() {
        let result: Result<String, anyhow::Error> = Err(anyhow::anyhow!("Unexpected error"));

        let api_result = result.into_api_result();

        assert!(api_result.is_err());
        match api_result {
            ApiResult::Error { error, .. } => {
                assert_eq!(error.code, ErrorCode::InternalError);
                assert!(error.details.is_some());
            }
            _ => panic!("Expected error"),
        }
    }
}
