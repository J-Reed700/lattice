//! What a new attempt of a job may continue from.
use super::JobStatus;
use crate::shared::error::{AppError, Result};

/// What a new attempt of a job continues from (`retry_in`, `continue_in`).
pub(super) enum Attempt<'a> {
    /// A failed, interrupted or cancelled job, rerun as it was requested.
    Retry,
    /// A completed job, for the work it left: `requested`, of `total` steps.
    Continue {
        requested: &'a serde_json::Value,
        total: u32,
    },
}

impl Attempt<'_> {
    pub(super) fn admits(&self, status: JobStatus) -> Result<()> {
        let (admitted, refusal) = match self {
            Self::Retry => (
                matches!(
                    status,
                    JobStatus::Failed | JobStatus::Interrupted | JobStatus::Cancelled
                ),
                "Only failed, interrupted, or cancelled jobs can be retried.",
            ),
            Self::Continue { .. } => (
                status == JobStatus::Completed,
                "Only a completed job can continue; retry a failed or stopped one.",
            ),
        };
        if admitted {
            Ok(())
        } else {
            Err(AppError::InvalidState(refusal.into()))
        }
    }
}
