//! How lesson jobs run under the job runtime: one preparation per course,
//! resumption after a restart, and durable redelivery after a temporary
//! service or connection outage.
use super::*;
use crate::shared::runtime::jobs::{
    Backoff, Deferral, JobFailure, JobKindConfig, JobRecord, RecoveryPolicy,
};

pub(in crate::features::learning) const LESSON_PREPARATION: &str = "learning.lesson_preparation";

/// Jobs for a course share its source snapshots and accepted outline: each one
/// refreshes and re-embeds the same sources and seeds the same snapshot, and
/// two at once would do that work twice over the same rows. Different courses
/// are independent, and how their model calls share the backend is the
/// inference scheduler's decision. A running preparation resumes from its
/// checkpoints after a restart.
pub(in crate::features::learning) fn lesson_job_config() -> JobKindConfig {
    JobKindConfig::new(RecoveryPolicy::Requeue)
        .exclusive_per_subject()
        .backoff(Backoff::default())
}

/// A disconnected model request is deferred work, not a failed lesson review.
/// Repeated outages never exhaust a budget; the delay between attempts grows
/// to five minutes.
pub(in crate::features::learning) fn lesson_failure(
    job: &JobRecord,
    error: &AppError,
) -> JobFailure {
    if !JobFailure::is_transient(error) {
        return JobFailure::Fail {
            code: "generation_failed".into(),
            message: error.to_string(),
        };
    }
    let activity = job
        .activity
        .clone()
        .and_then(|value| {
            serde_json::from_value::<curriculum::LearningGenerationActivity>(value).ok()
        })
        .map(|mut activity| {
            activity.model_running = false;
            activity.response_characters = 0;
            activity.model_attempt = 0;
            activity.last_activity_at = now();
            activity
        })
        .and_then(|activity| serde_json::to_value(activity).ok());
    let message = match error {
        AppError::Network(_) => "Connection interrupted; saved work will resume automatically",
        AppError::RateLimitExceeded(_) => {
            "The model service is rate limiting requests; preparation will retry automatically"
        }
        _ => "The model service could not finish the request; preparation will retry automatically",
    };
    let recovery = "Your draft and completed checks are saved. Preparation retries automatically while Lattice is open, with increasing delays of up to five minutes. Repeated service failures can prevent further progress until the service recovers. You can leave this screen or cancel.";
    JobFailure::Defer(Deferral {
        code: "temporarily_unavailable".into(),
        message: format!("{error} {recovery}"),
        progress_message: message.into(),
        activity,
    })
}

#[cfg(test)]
#[test]
fn only_database_pool_admission_timeout_is_a_temporary_service_failure() {
    assert!(matches!(
        db(sqlx::Error::PoolTimedOut),
        AppError::ServiceNotAvailable(_)
    ));
    assert!(matches!(
        db(sqlx::Error::RowNotFound),
        AppError::Database(_)
    ));
    assert!(matches!(
        db(sqlx::Error::Protocol("invalid state".into())),
        AppError::Database(_)
    ));
}
