//! The error for a web server that answered a page request with an error
//! status.

use lattice_core::shared::error::AppError;
use reqwest::StatusCode;

/// A status as a line: its code and standard reason, or the code alone when it
/// has none.
pub(crate) fn status_line(status: StatusCode) -> String {
    match status.canonical_reason() {
        Some(reason) => format!("HTTP {} {reason}", status.as_u16()),
        None => format!("HTTP {}", status.as_u16()),
    }
}

/// What a page request's error status is: a block for 403, 429 and 503, which
/// sites answer to automated readers, and the status itself for any other.
pub(crate) fn status_error(status: StatusCode) -> AppError {
    match status {
        StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS | StatusCode::SERVICE_UNAVAILABLE => {
            AppError::HttpStatus {
                status: status.as_u16(),
                message: format!(
                    "{}: Site blocked automated access. Try importing the PDF or local file version.",
                    status_line(status),
                ),
            }
        }
        _ => AppError::HttpStatus {
            status: status.as_u16(),
            message: status_line(status),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_status_sites_give_automated_readers_says_it_blocked_the_read() {
        for status in [
            StatusCode::FORBIDDEN,
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::SERVICE_UNAVAILABLE,
        ] {
            let error = status_error(status);
            let AppError::HttpStatus {
                status: got,
                message,
            } = &error
            else {
                panic!("{status} is an answer, not a lost connection: {error:?}");
            };
            assert_eq!(*got, status.as_u16());
            assert!(
                message.starts_with(&format!("HTTP {} ", status.as_u16())),
                "{message}"
            );
            assert!(
                message.ends_with(
                    "Site blocked automated access. Try importing the PDF or local file version."
                ),
                "{message}"
            );
        }
    }

    #[test]
    fn any_other_status_is_named_and_not_blamed_on_the_connection() {
        let error = status_error(StatusCode::NOT_FOUND);
        let AppError::HttpStatus { status, message } = &error else {
            panic!("404 is an answer, not a lost connection: {error:?}");
        };
        assert_eq!(*status, 404);
        assert_eq!(message, "HTTP 404 Not Found");
        assert_eq!(error.to_string(), "HTTP 404 Not Found");
    }

    #[test]
    fn a_status_with_no_standard_reason_is_named_by_its_code_alone() {
        let error = status_error(StatusCode::from_u16(599).unwrap());
        let AppError::HttpStatus { status, message } = &error else {
            panic!("599 is an answer, not a lost connection: {error:?}");
        };
        assert_eq!(*status, 599);
        assert_eq!(message, "HTTP 599");
    }
}
