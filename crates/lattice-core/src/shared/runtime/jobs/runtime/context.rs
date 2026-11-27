use super::super::{JobDto, JobFailure, JobOutcome, JobRecord, JobStore};
use super::{until_cancelled, Observer};
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use std::{
    future::Future,
    sync::{Arc, PoisonError, RwLock},
};
use tokio_util::sync::CancellationToken;

/// The work for one kind of job.
#[async_trait]
pub trait JobHandler: Send + Sync + 'static {
    /// Runs one attempt of a claimed job. Wrap work that may be abandoned in
    /// [`JobContext::until_cancelled`] and return [`JobOutcome::Stopped`] when
    /// it is; keep terminal writes outside it so they finish or roll back whole.
    async fn run(&self, context: &JobContext) -> Result<JobOutcome>;

    /// How a failed attempt is recorded. `job` is the saved state after the
    /// attempt, including its last activity.
    fn failure(&self, job: &JobRecord, error: &AppError) -> JobFailure {
        let _ = job;
        JobFailure::classify(error)
    }

    /// Called after the runtime itself failed `job` with `code` (a panic, or a
    /// job that stopped the app), so the feature can settle the state it keeps
    /// outside the jobs table.
    async fn failed_by_runtime(&self, job: &JobRecord, code: &str) {
        let _ = (job, code);
    }
}

/// A handler's view of the job it is running.
pub struct JobContext {
    job: JobRecord,
    events: Arc<Events>,
    cancel: CancellationToken,
}

impl JobContext {
    pub(super) fn from_claimed(
        job: JobRecord,
        events: Arc<Events>,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            job,
            events,
            cancel,
        }
    }

    /// A saved job's context outside any runtime, for driving a handler's
    /// steps directly: nothing observes it and nothing cancels it.
    #[cfg(any(test, feature = "test-support"))]
    pub async fn detached(store: &JobStore, id: &str) -> Result<Self> {
        Ok(Self {
            job: store.get(id).await?,
            events: Arc::new(Events::new(store.clone())),
            cancel: CancellationToken::new(),
        })
    }

    pub fn id(&self) -> &str {
        &self.job.id
    }

    /// The job as it was claimed for this attempt.
    pub fn job(&self) -> &JobRecord {
        &self.job
    }

    pub fn subject_id(&self) -> Option<&str> {
        self.job.subject_id.as_deref()
    }

    pub fn store(&self) -> &JobStore {
        self.events.store()
    }

    /// Cancelled by the user, or by shutdown.
    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancel
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// Drops `work` as soon as the job is cancelled.
    pub async fn until_cancelled<T>(&self, work: impl Future<Output = T>) -> Option<T> {
        until_cancelled(&self.cancel, work).await
    }

    /// Saves progress, and the activity snapshot when given. Returns false once
    /// the job is no longer running.
    pub async fn progress(
        &self,
        current: u32,
        message: &str,
        activity: Option<&serde_json::Value>,
    ) -> Result<bool> {
        let saved = self
            .store()
            .progress(self.id(), current, message, activity)
            .await?;
        if saved {
            self.events.publish(self.id()).await;
        }
        Ok(saved)
    }

    pub async fn heartbeat(&self) -> Result<bool> {
        self.store().heartbeat(self.id()).await
    }

    pub async fn checkpoint(&self, key: &str) -> Result<Option<serde_json::Value>> {
        self.store().checkpoint(self.id(), key).await
    }

    pub async fn put_checkpoint(&self, key: &str, data: &serde_json::Value) -> Result<bool> {
        self.store().put_checkpoint(self.id(), key, data).await
    }

    pub async fn staged(&self) -> Result<Option<serde_json::Value>> {
        self.store().staged(self.id()).await
    }

    pub async fn stage(&self, staged: serde_json::Value) -> Result<bool> {
        self.store().update_staged(self.id(), |_| Ok(staged)).await
    }
}

/// Publishes a job's saved state to the observer after each transition.
pub(super) struct Events {
    store: JobStore,
    observer: RwLock<Option<Observer>>,
}

impl Events {
    pub(super) fn new(store: JobStore) -> Self {
        Self {
            store,
            observer: RwLock::new(None),
        }
    }

    pub(super) fn store(&self) -> &JobStore {
        &self.store
    }

    pub(super) fn observe(&self, observer: Observer) {
        *self
            .observer
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Some(observer);
    }

    pub(super) async fn publish(&self, id: &str) {
        let observer = self
            .observer
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let Some(observer) = observer else { return };
        match self.store.get_summary(id).await {
            Ok(job) => observer(&JobDto::from(&job)),
            Err(error) => {
                tracing::debug!(job_id = id, %error, "Could not read a job to publish its status")
            }
        }
    }
}
