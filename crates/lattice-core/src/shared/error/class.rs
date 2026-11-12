//! The one classification of an error, for every caller.
use super::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    /// Waiting can change the outcome: the service or database was not reached.
    Outage,
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
            AppError::Network(_)
            | AppError::RateLimitExceeded(_)
            | AppError::ServiceNotAvailable(_)
            | AppError::DatabaseBusy(_) => Self::Outage,
            AppError::HttpStatus { status, .. } if is_retry_later_status(*status) => Self::Outage,
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
    fn waiting_changes_the_outcome_only_for_an_unreached_service() {
        for outage in [
            AppError::Network("reset".into()),
            AppError::RateLimitExceeded("slow down".into()),
            AppError::ServiceNotAvailable("down".into()),
            AppError::DatabaseBusy("locked".into()),
            answered(429),
            answered(502),
            answered(503),
            answered(504),
        ] {
            assert_eq!(ErrorClass::of(&outage), ErrorClass::Outage, "{outage}");
        }
        for settled in [answered(400), answered(403), answered(404), answered(500)] {
            assert_eq!(ErrorClass::of(&settled), ErrorClass::Fault, "{settled}");
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
