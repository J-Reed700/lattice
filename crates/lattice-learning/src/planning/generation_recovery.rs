//! How lesson jobs run under the job runtime: one preparation per course,
//! resumption after a restart, and durable redelivery after a temporary
//! service or connection outage.
use super::*;
use lattice_core::shared::error::ErrorClass;
use lattice_core::shared::runtime::jobs::{
    Backoff, Deferral, JobFailure, JobKindConfig, JobRecord, RecoveryPolicy,
};

pub const LESSON_PREPARATION: &str = "learning.lesson_preparation";

/// The outcome code of a lesson preparation attempt an update ended: it has no saved work to resume.
pub(crate) const ENDED_BY_UPDATE: &str = "ended_by_update";

/// Lessons of one course prepare side by side: the state they share is set up
/// under the course's setup lock, and how their model calls share the backend
/// is the inference scheduler's decision. A running preparation resumes from
/// its checkpoints after a restart.
pub fn lesson_job_config() -> JobKindConfig {
    JobKindConfig::new(RecoveryPolicy::Requeue)
        .ranked()
        .backoff(Backoff::default())
}

/// A disconnected model request is deferred work, not a failed lesson review.
/// Outages never exhaust a budget; the delay between attempts grows to five
/// minutes. A model's rejection ([`AppError::ModelRejected`]) fails at once,
/// and the job runtime ends a job whose other failure comes back with nothing
/// saved in between, so a lesson never retries forever unseen.
pub(crate) fn lesson_failure(job: &JobRecord, error: &AppError) -> JobFailure {
    if !ErrorClass::of(error).is_temporary() {
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
            activity.waiting_for_model = false;
            activity.response_characters = 0;
            activity.model_attempt = 0;
            activity.last_activity_at = now();
            activity
        })
        .and_then(|activity| serde_json::to_value(activity).ok());
    let message = match error {
        AppError::Network(_) => "Connection interrupted; saved work will resume automatically",
        AppError::HttpStatus { .. } => "A website is busy; saved work will resume automatically",
        AppError::RateLimitExceeded(_) => {
            "The model service is rate limiting requests; preparation will retry automatically"
        }
        AppError::DatabaseBusy(_) => {
            "Another task is using the database; preparation will retry automatically"
        }
        _ => "The model service could not finish the request; preparation will retry automatically",
    };
    let recovery = "Your draft and completed checks are saved. Preparation retries automatically while Lattice is open, with increasing delays of up to five minutes. If the same failure comes back with nothing saved in between, preparation stops and shows it instead of retrying again. You can leave this screen or cancel.";
    JobFailure::Defer(Deferral {
        code: "temporarily_unavailable".into(),
        message: format!("{error} {recovery}"),
        progress_message: message.into(),
        activity,
    })
}
