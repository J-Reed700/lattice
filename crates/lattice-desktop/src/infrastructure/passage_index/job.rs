//! A passage index build as a job on the app's job runtime: how its kind is
//! configured, and how its progress is saved without writing the job row on
//! every batch.

use lattice_core::shared::runtime::jobs::{JobContext, JobKindConfig, RecoveryPolicy};
use lattice_core::shared::Result;
use std::time::{Duration, Instant};

/// The saved progress moves at most this often while counts move; a change
/// of state is saved at once.
const PROGRESS_INTERVAL: Duration = Duration::from_secs(1);

/// Builds take turns, one at a time per kind, and two builds of one index
/// (one subject) never overlap. A build resumes after a restart: the index on
/// disk is its checkpoint.
pub fn build_job_config() -> JobKindConfig {
    JobKindConfig::new(RecoveryPolicy::Requeue)
        .concurrency(1)
        .exclusive_per_subject()
}

/// Saves a build's progress and activity to its job, throttled. `S` is the
/// owner's state: a change of it is always saved.
pub struct ProgressSaver<'a, S> {
    context: &'a JobContext,
    /// Saved progress only moves forward; a scan that finds more passages
    /// lowers the share until embedding catches up.
    high: u32,
    last: Option<(Instant, S)>,
}

impl<'a, S: Copy + PartialEq> ProgressSaver<'a, S> {
    pub fn new(context: &'a JobContext) -> Self {
        Self {
            context,
            high: context.job().progress_current,
            last: None,
        }
    }

    /// Saves `percent`, `message` and `activity` when `force`d, when `state`
    /// moved, or when the last save is old enough.
    pub async fn save(
        &mut self,
        state: S,
        percent: u32,
        message: &str,
        activity: &serde_json::Value,
        force: bool,
    ) -> Result<()> {
        let due = force
            || self
                .last
                .is_none_or(|(at, last)| last != state || at.elapsed() >= PROGRESS_INTERVAL);
        if !due {
            return Ok(());
        }
        self.last = Some((Instant::now(), state));
        self.high = self.high.max(percent).min(100);
        self.context
            .progress(self.high, message, Some(activity))
            .await?;
        Ok(())
    }
}
