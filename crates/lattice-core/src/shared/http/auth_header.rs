//! The authentication header of a remote model server, as request headers.

use crate::shared::error::AppError;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

/// Request headers carrying one authentication header. The value is marked
/// sensitive and no error echoes it.
pub fn auth_header_map(name: &str, value: &str) -> Result<HeaderMap, AppError> {
    let name = HeaderName::from_bytes(name.trim().as_bytes())
        .map_err(|_| AppError::InvalidConfig("Invalid authentication header name".into()))?;
    let mut value = HeaderValue::from_str(value.trim())
        .map_err(|_| AppError::InvalidConfig("Invalid authentication header value".into()))?;
    value.set_sensitive(true);
    let mut headers = HeaderMap::new();
    headers.insert(name, value);
    Ok(headers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_valid_header_is_trimmed_and_sensitive() {
        let headers = auth_header_map("  X-Api-Key ", "  secret ").unwrap();
        let value = headers.get("x-api-key").unwrap();
        assert_eq!(value.to_str().unwrap(), "secret");
        assert!(value.is_sensitive());
        assert_eq!(headers.len(), 1);
    }

    #[test]
    fn an_invalid_name_is_refused_without_the_value() {
        let error = auth_header_map("bad header", "hunter2").unwrap_err();
        assert!(
            matches!(&error, AppError::InvalidConfig(m) if m == "Invalid authentication header name")
        );
        assert!(!format!("{error:?} {error}").contains("hunter2"));
    }

    #[test]
    fn an_invalid_value_is_refused_without_echoing_it() {
        let error = auth_header_map("X-Api-Key", "hunter2\nbreak").unwrap_err();
        assert!(
            matches!(&error, AppError::InvalidConfig(m) if m == "Invalid authentication header value")
        );
        assert!(!format!("{error:?} {error}").contains("hunter2"));
    }
}
