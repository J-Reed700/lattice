//! Durable background jobs: one table, one lifecycle, one dispatcher.
//!
//! A feature registers a [`JobHandler`] per kind with the application's
//! [`JobRuntime`]. Submission is idempotent on the caller's operation ID, and
//! every state transition is a conditional write in [`JobStore`], so a user's
//! cancellation always wins over a late worker result. Handlers own their
//! cancellation points: terminal writes run to completion rather than being
//! dropped mid-transaction.
mod repository;
mod runtime;

pub use repository::JobStore;
pub use runtime::{JobContext, JobHandler, JobRuntime};

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// The renderer event that carries a [`JobDto`] after every transition.
pub const STATUS_EVENT: &str = "jobs://status";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

impl JobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "pending" => Self::Pending,
            "running" => Self::Running,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            "interrupted" => Self::Interrupted,
            _ => return None,
        })
    }

    pub fn is_finished(self) -> bool {
        !matches!(self, Self::Pending | Self::Running)
    }
}

/// A saved job as the store reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct JobRecord {
    pub id: String,
    pub kind: String,
    pub subject_id: Option<String>,
    pub operation_id: String,
    pub payload_hash: String,
    pub status: JobStatus,
    pub requested: serde_json::Value,
    pub progress_current: u32,
    pub progress_total: u32,
    pub progress_message: String,
    pub activity: Option<serde_json::Value>,
    pub staged_result: Option<serde_json::Value>,
    pub result_ref: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub retry_of_job_id: Option<String>,
    pub retry_count: u32,
    pub retry_not_before: Option<i64>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub heartbeat_at: Option<i64>,
}

/// A request to run work. `operation_id` is the caller's idempotency key;
/// resubmitting it returns the saved job, unless `payload_hash` differs.
#[derive(Debug, Clone)]
pub struct NewJob {
    pub kind: String,
    pub subject_id: Option<String>,
    pub operation_id: String,
    pub payload_hash: String,
    pub requested: serde_json::Value,
    pub progress_total: u32,
    pub message: String,
}

/// The job a submission or retry resolved to, and whether this call made it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Admitted {
    pub id: String,
    pub created: bool,
}

/// What a job's handler concluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobOutcome {
    /// The runtime records completion with this reference.
    Completed { result_ref: String, message: String },
    /// The handler committed completion itself, atomically with its own
    /// writes, through [`JobStore::complete_in`].
    Committed,
    /// The handler stopped at a cancellation point. The runtime suspends or
    /// interrupts the job according to the kind's [`RecoveryPolicy`]; a user's
    /// cancellation has already been committed and stays.
    Stopped,
    /// The work ran and did not succeed, without an error to classify: the job
    /// ends as failed with this code, and the user may retry it.
    Failed { code: String, message: String },
}

/// How a failed attempt is recorded.
#[derive(Debug, Clone, PartialEq)]
pub enum JobFailure {
    /// A terminal failure; the user may retry it as a new attempt.
    Fail { code: String, message: String },
    /// A temporary outage. The job returns to pending and is redelivered after
    /// the kind's backoff, without a limit on attempts.
    Defer(Deferral),
}

/// What a deferred job shows while it waits to be redelivered.
#[derive(Debug, Clone, PartialEq)]
pub struct Deferral {
    pub code: String,
    pub message: String,
    pub progress_message: String,
    /// Replaces the saved activity when present.
    pub activity: Option<serde_json::Value>,
}

impl JobFailure {
    /// Whether an error is a temporary outage of the network or a service,
    /// rather than a failure of the work itself.
    pub fn is_transient(error: &crate::shared::error::AppError) -> bool {
        use crate::shared::error::AppError;
        matches!(
            error,
            AppError::Network(_)
                | AppError::ServiceNotAvailable(_)
                | AppError::RateLimitExceeded(_)
        )
    }

    /// Outages are deferred; anything else fails the attempt.
    pub fn classify(error: &crate::shared::error::AppError) -> Self {
        if Self::is_transient(error) {
            Self::Defer(Deferral {
                code: "temporarily_unavailable".into(),
                message: error.to_string(),
                progress_message: "Waiting to retry after a service outage".into(),
                activity: None,
            })
        } else {
            Self::Fail {
                code: "failed".into(),
                message: error.to_string(),
            }
        }
    }
}

/// What happens to a running job when its process stops: at shutdown and,
/// for a crash, at the next startup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryPolicy {
    /// The handler resumes from its checkpoints.
    Requeue,
    /// The work cannot resume; the job ends as interrupted and can be retried.
    Interrupt,
}

/// Exponential delay between redeliveries of a deferred job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    pub initial: Duration,
    pub max: Duration,
}

impl Backoff {
    /// The delay before attempt `retries + 1`. This caps the wait between
    /// attempts, never the number of attempts or the job's running time.
    pub fn delay(self, retries: u32) -> Duration {
        self.initial
            .saturating_mul(1u32 << retries.min(16))
            .min(self.max)
    }
}

impl Default for Backoff {
    fn default() -> Self {
        Self {
            initial: Duration::from_secs(30),
            max: Duration::from_secs(300),
        }
    }
}

/// How the runtime schedules one kind of job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobKindConfig {
    pub concurrency: usize,
    pub exclusive_per_subject: bool,
    pub recovery: RecoveryPolicy,
    pub backoff: Backoff,
}

impl JobKindConfig {
    /// Unbounded concurrency: contention for a shared backend such as the
    /// model is that backend's scheduler's decision, not the job runtime's.
    pub fn new(recovery: RecoveryPolicy) -> Self {
        Self {
            concurrency: tokio::sync::Semaphore::MAX_PERMITS,
            exclusive_per_subject: false,
            recovery,
            backoff: Backoff::default(),
        }
    }

    pub fn concurrency(mut self, limit: usize) -> Self {
        self.concurrency = limit.clamp(1, tokio::sync::Semaphore::MAX_PERMITS);
        self
    }

    /// At most one job of this kind runs per subject; later ones wait, still
    /// pending, without holding a concurrency slot.
    pub fn exclusive_per_subject(mut self) -> Self {
        self.exclusive_per_subject = true;
        self
    }

    pub fn backoff(mut self, backoff: Backoff) -> Self {
        self.backoff = backoff;
        self
    }
}

/// A job as the renderer sees it on [`STATUS_EVENT`].
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct JobDto {
    pub id: String,
    pub kind: String,
    pub subject_id: Option<String>,
    pub status: JobStatus,
    pub progress_current: u32,
    pub progress_total: u32,
    pub progress_message: String,
    pub activity: Option<serde_json::Value>,
    pub result_ref: Option<String>,
    pub error_code: Option<String>,
    pub error: Option<String>,
    pub retry_of_job_id: Option<String>,
    pub retry_count: u32,
    pub retry_not_before: Option<i64>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
}

impl From<&JobRecord> for JobDto {
    fn from(job: &JobRecord) -> Self {
        Self {
            id: job.id.clone(),
            kind: job.kind.clone(),
            subject_id: job.subject_id.clone(),
            status: job.status,
            progress_current: job.progress_current,
            progress_total: job.progress_total,
            progress_message: job.progress_message.clone(),
            activity: job.activity.clone(),
            result_ref: job.result_ref.clone(),
            error_code: job.error_code.clone(),
            error: job.error_message.clone(),
            retry_of_job_id: job.retry_of_job_id.clone(),
            retry_count: job.retry_count,
            retry_not_before: job.retry_not_before,
            created_at: job.created_at,
            started_at: job.started_at,
            finished_at: job.finished_at,
        }
    }
}

#[cfg(test)]
mod tests;
