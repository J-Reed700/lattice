//! The one classification of an error, for every caller.
use super::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    /// The service or database was not reached.
    Outage,
    /// The service was reached and said it cannot answer now.
    Unavailable,
    /// A model answered, but not usably, or refused the request.
    Unusable,
    /// The request does not fit the configured context window.
    DoesNotFit,
    /// Anything else; a retry would meet the same error.
    Fault,
}

/// Whether the same work may succeed when sent in smaller parts.
pub fn smaller_parts_may_succeed(error: &AppError) -> bool {
    ErrorClass::of(error) == ErrorClass::DoesNotFit || matches!(error, AppError::AnswerCutOff(_))
}

impl ErrorClass {
    /// The one classification of an error, for every caller.
    pub fn of(error: &AppError) -> Self {
        match error {
            AppError::Network(_) | AppError::RateLimitExceeded(_) | AppError::DatabaseBusy(_) => {
                Self::Outage
            }
            AppError::ServiceNotAvailable(_) => Self::Unavailable,
            AppError::HttpStatus { status, .. } if is_retry_later_status(*status) => {
                Self::Unavailable
            }
            AppError::ModelAnswerUnusable(_)
            | AppError::AnswerCutOff(_)
            | AppError::ModelRejected(_) => Self::Unusable,
            AppError::PromptExceedsWindow { .. } | AppError::WindowTooSmall(_) => Self::DoesNotFit,
            AppError::HttpStatus { .. }
            | AppError::Io { .. }
            | AppError::Json { .. }
            | AppError::Database(_)
            | AppError::InvalidConfig(_)
            | AppError::LearningModelNotSet(_)
            | AppError::ModelServerCannotStart(_)
            | AppError::NotFound(_)
            | AppError::PermissionDenied(_)
            | AppError::InvalidInput(_)
            | AppError::FileTooLarge { .. }
            | AppError::FileStorage(_)
            | AppError::FileNotFound { .. }
            | AppError::FileRead { .. }
            | AppError::EmbeddingFailed { .. }
            | AppError::UnsupportedFileType { .. }
            | AppError::ContentExtraction { .. }
            | AppError::TokenizationError { .. }
            | AppError::InvalidData(_)
            | AppError::FileSystem(_)
            | AppError::InvalidUrl(_)
            | AppError::InvalidState(_)
            | AppError::Storage(_)
            | AppError::Serialization(_)
            | AppError::Security(_)
            | AppError::KeyringError(_)
            | AppError::ValidationFailed(_)
            | AppError::InternalError(_)
            | AppError::Parsing(_)
            | AppError::Deserialization(_)
            | AppError::BackupCreationFailed(_)
            | AppError::BackupRestoreFailed(_)
            | AppError::BackupCorrupted(_)
            | AppError::Migration(_)
            | AppError::AiModelsNotInstalled(_)
            | AppError::ModelLoadFailed(_)
            | AppError::ConcurrentModification { .. }
            | AppError::Conflict(_)
            | AppError::Cancelled(_)
            | AppError::LessonHeld(_)
            | AppError::Domain(_)
            | AppError::Application(_)
            | AppError::Other(_) => Self::Fault,
        }
    }

    /// Whether waiting may change the outcome.
    pub fn is_temporary(self) -> bool {
        matches!(self, Self::Outage | Self::Unavailable)
    }
}

/// Whether a web server's status says to try again later: too many requests,
/// or a gateway or service that is down or busy. Any other status is the
/// server's settled answer.
fn is_retry_later_status(status: u16) -> bool {
    matches!(status, 429 | 502 | 503 | 504)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answered(status: u16) -> AppError {
        AppError::HttpStatus {
            status,
            message: format!("HTTP {status}"),
        }
    }

    #[test]
    fn an_unreached_service_is_an_outage() {
        for outage in [
            AppError::Network("reset".into()),
            AppError::RateLimitExceeded("slow down".into()),
            AppError::DatabaseBusy("locked".into()),
        ] {
            assert_eq!(ErrorClass::of(&outage), ErrorClass::Outage, "{outage}");
            assert!(ErrorClass::of(&outage).is_temporary(), "{outage}");
        }
    }

    #[test]
    fn a_service_that_says_it_cannot_answer_now_is_unavailable() {
        for unavailable in [
            AppError::ServiceNotAvailable("down".into()),
            answered(429),
            answered(502),
            answered(503),
            answered(504),
        ] {
            assert_eq!(
                ErrorClass::of(&unavailable),
                ErrorClass::Unavailable,
                "{unavailable}"
            );
            assert!(ErrorClass::of(&unavailable).is_temporary(), "{unavailable}");
        }
    }

    #[test]
    fn a_settled_answer_is_a_fault_that_waiting_does_not_change() {
        for settled in [answered(400), answered(403), answered(404), answered(500)] {
            assert_eq!(ErrorClass::of(&settled), ErrorClass::Fault, "{settled}");
            assert!(!ErrorClass::of(&settled).is_temporary(), "{settled}");
        }
    }

    #[test]
    fn a_model_that_answered_badly_or_refused_is_unusable() {
        for unusable in [
            AppError::ModelAnswerUnusable("no".into()),
            AppError::AnswerCutOff("no".into()),
            AppError::ModelRejected("no".into()),
        ] {
            assert_eq!(ErrorClass::of(&unusable), ErrorClass::Unusable);
        }
    }

    #[test]
    fn a_window_that_cannot_hold_the_work_is_apart_from_a_missing_model() {
        let too_big = AppError::PromptExceedsWindow {
            prompt_tokens: 10,
            window: 5,
            counted_exactly: true,
            message: "too big".into(),
        };
        assert_eq!(ErrorClass::of(&too_big), ErrorClass::DoesNotFit);
        assert_eq!(
            ErrorClass::of(&AppError::WindowTooSmall("small".into())),
            ErrorClass::DoesNotFit
        );
        assert_eq!(
            ErrorClass::of(&AppError::InvalidConfig("no model".into())),
            ErrorClass::Fault
        );
    }

    #[test]
    fn a_stop_on_request_is_a_fault_never_an_outage() {
        for stopped in [
            AppError::Cancelled("stopped".into()),
            AppError::LessonHeld("held".into()),
            AppError::Database("corrupt".into()),
        ] {
            assert_eq!(ErrorClass::of(&stopped), ErrorClass::Fault, "{stopped}");
        }
    }
}
