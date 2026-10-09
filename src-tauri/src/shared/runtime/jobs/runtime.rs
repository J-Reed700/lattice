//! Delivers saved jobs to their kind's handler, within the kind's limits.
use super::{
    JobDto, JobFailure, JobKindConfig, JobOutcome, JobRecord, JobStore, NewJob, RecoveryPolicy,
};
use crate::shared::{
    error::{AppError, Result},
    runtime::background::BackgroundTasks,
};
use async_trait::async_trait;
use std::{
    collections::HashMap,
    future::Future,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard, PoisonError, RwLock, Weak,
    },
    time::Duration,
};
use tokio::sync::{Notify, Semaphore};
use tokio_util::sync::CancellationToken;

const POLL: Duration = Duration::from_secs(30);
const RETRY_QUEUED: &str = "Retry queued";

type Observer = Arc<dyn Fn(&JobDto) + Send + Sync>;
type Subjects = Arc<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

async fn until_cancelled<T>(
    cancel: &CancellationToken,
    work: impl Future<Output = T>,
) -> Option<T> {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => None,
        result = work => Some(result),
    }
}

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
}

/// A handler's view of the job it is running.
pub struct JobContext {
    job: JobRecord,
    events: Arc<Events>,
    cancel: CancellationToken,
}

impl JobContext {
    /// A saved job's context outside any runtime, for driving a handler's
    /// steps directly: nothing observes it and nothing cancels it.
    #[cfg(test)]
    pub(crate) async fn detached(store: &JobStore, id: &str) -> Result<Self> {
        Ok(Self {
            job: store.get(id).await?,
            events: Arc::new(Events {
                store: store.clone(),
                observer: RwLock::new(None),
            }),
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
        &self.events.store
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
struct Events {
    store: JobStore,
    observer: RwLock<Option<Observer>>,
}

impl Events {
    async fn publish(&self, id: &str) {
        let observer = self
            .observer
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let Some(observer) = observer else { return };
        match self.store.get(id).await {
            Ok(job) => observer(&JobDto::from(&job)),
            Err(error) => {
                tracing::debug!(job_id = id, %error, "Could not read a job to publish its status")
            }
        }
    }
}

struct Kind {
    handler: Arc<dyn JobHandler>,
    config: JobKindConfig,
    slots: Arc<Semaphore>,
}

/// Removes a job from the in-flight set when its worker ends.
struct Registration {
    active: Arc<Mutex<HashMap<String, CancellationToken>>>,
    id: String,
}

impl Drop for Registration {
    fn drop(&mut self) {
        lock(&self.active).remove(&self.id);
    }
}

/// A claim on a subject's lock. The map forgets a subject once nothing holds
/// or waits for it.
struct SubjectLease {
    subjects: Subjects,
    key: String,
    lock: Arc<tokio::sync::Mutex<()>>,
}

impl Drop for SubjectLease {
    fn drop(&mut self) {
        let mut subjects = lock(&self.subjects);
        // A count of two is the map and this lease.
        if subjects
            .get(&self.key)
            .is_some_and(|existing| Arc::strong_count(existing) == 2)
        {
            subjects.remove(&self.key);
        }
    }
}

/// The application's job runtime, owned by the composition root. Workers run
/// as its own background tasks: [`JobRuntime::close`] cancels them and
/// [`JobRuntime::drain`] waits for their final writes.
pub struct JobRuntime {
    events: Arc<Events>,
    tasks: BackgroundTasks,
    kinds: RwLock<HashMap<String, Arc<Kind>>>,
    active: Arc<Mutex<HashMap<String, CancellationToken>>>,
    subjects: Subjects,
    wake: Arc<Notify>,
    poll: Duration,
    dispatching: AtomicBool,
}

impl JobRuntime {
    pub fn new(pool: sqlx::SqlitePool) -> Arc<Self> {
        Self::with_poll_interval(pool, POLL)
    }

    /// The saved pending rows are the outbox. Besides wakeups, the dispatcher
    /// rereads them on this interval, so a missed wakeup, a transient database
    /// error or a deferred job's retry time cannot strand work.
    pub fn with_poll_interval(pool: sqlx::SqlitePool, poll: Duration) -> Arc<Self> {
        Arc::new(Self {
            events: Arc::new(Events {
                store: JobStore::new(pool),
                observer: RwLock::new(None),
            }),
            tasks: BackgroundTasks::default(),
            kinds: RwLock::default(),
            active: Arc::default(),
            subjects: Arc::default(),
            wake: Arc::new(Notify::new()),
            poll,
            dispatching: AtomicBool::new(false),
        })
    }

    pub fn store(&self) -> &JobStore {
        &self.events.store
    }

    /// Receives every job's state after each transition and progress write.
    pub fn observe(&self, observer: impl Fn(&JobDto) + Send + Sync + 'static) {
        *self
            .events
            .observer
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(observer));
    }

    fn kind(&self, kind: &str) -> Option<Arc<Kind>> {
        self.kinds
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(kind)
            .cloned()
    }

    /// Registers the handler for `kind`, first settling the jobs of that kind
    /// the last process left running according to `config.recovery`, then
    /// starts delivering its saved work.
    pub async fn register(
        self: &Arc<Self>,
        kind: &str,
        handler: Arc<dyn JobHandler>,
        config: JobKindConfig,
    ) -> Result<()> {
        let duplicate =
            || AppError::InvalidState(format!("Job kind {kind} is already registered."));
        if self.kind(kind).is_some() {
            return Err(duplicate());
        }
        let recovered = self.store().recover(kind, config.recovery).await?;
        {
            let mut kinds = self.kinds.write().unwrap_or_else(PoisonError::into_inner);
            if kinds.contains_key(kind) {
                return Err(duplicate());
            }
            kinds.insert(
                kind.to_string(),
                Arc::new(Kind {
                    handler,
                    config,
                    slots: Arc::new(Semaphore::new(config.concurrency)),
                }),
            );
        }
        for id in recovered {
            self.events.publish(&id).await;
        }
        self.start_dispatcher();
        self.wake.notify_one();
        Ok(())
    }

    fn start_dispatcher(self: &Arc<Self>) {
        if self.dispatching.swap(true, Ordering::SeqCst) {
            return;
        }
        let runtime = Arc::downgrade(self);
        let cancel = self.tasks.token();
        let wake = self.wake.clone();
        let poll = self.poll;
        let _ = self.tasks.spawn(async move {
            let mut interval = tokio::time::interval(poll);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => break,
                    _ = interval.tick() => {}
                    _ = wake.notified() => {}
                }
                let Some(runtime) = Weak::upgrade(&runtime) else {
                    break;
                };
                runtime.dispatch_due().await;
            }
        });
    }

    async fn dispatch_due(self: &Arc<Self>) {
        let kinds: Vec<String> = self
            .kinds
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .keys()
            .cloned()
            .collect();
        for kind in kinds {
            match self.store().due(&kind).await {
                Ok(jobs) => {
                    for job in jobs {
                        self.dispatch(&job.id);
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, kind, "Could not dispatch saved jobs; will retry")
                }
            }
        }
    }

    /// Starts a worker for a saved job. A job already in flight is left alone,
    /// and a worker whose job is not pending and due exits without running.
    pub fn dispatch(self: &Arc<Self>, id: &str) {
        let cancel = self.tasks.token().child_token();
        {
            let mut active = lock(&self.active);
            if active.contains_key(id) {
                return;
            }
            // Registered before spawning, so a queued job can be cancelled.
            active.insert(id.to_string(), cancel.clone());
        }
        let registration = Registration {
            active: self.active.clone(),
            id: id.to_string(),
        };
        let runtime = Arc::clone(self);
        let id = id.to_string();
        // Rejected admission at shutdown drops the registration; the job is
        // still pending and resumes at the next start.
        let _ = self.tasks.spawn(async move {
            let _registration = registration;
            runtime.work(&id, cancel).await;
        });
    }

    fn subject_lease(&self, kind: &str, subject: &str) -> SubjectLease {
        let key = format!("{kind}\u{1f}{subject}");
        let lock = lock(&self.subjects).entry(key.clone()).or_default().clone();
        SubjectLease {
            subjects: self.subjects.clone(),
            key,
            lock,
        }
    }

    async fn work(&self, id: &str, cancel: CancellationToken) {
        let job = match self.store().get(id).await {
            Ok(job) => job,
            Err(error) => {
                tracing::warn!(job_id = id, %error, "Could not read a saved job before running it");
                return;
            }
        };
        let Some(kind) = self.kind(&job.kind) else {
            tracing::debug!(job_id = id, kind = %job.kind, "No handler is registered for this job kind yet");
            return;
        };
        // Take the subject before a slot: a job waiting for its subject must not
        // hold a slot that another subject's job could use.
        let lease = match (&job.subject_id, kind.config.exclusive_per_subject) {
            (Some(subject), true) => Some(self.subject_lease(&job.kind, subject)),
            _ => None,
        };
        let _subject = match &lease {
            Some(lease) => match until_cancelled(&cancel, lease.lock.clone().lock_owned()).await {
                Some(guard) => Some(guard),
                None => return,
            },
            None => None,
        };
        let Some(Ok(_slot)) = until_cancelled(&cancel, kind.slots.clone().acquire_owned()).await
        else {
            return;
        };
        let job = match self.store().claim(id).await {
            Ok(Some(job)) => job,
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(job_id = id, %error, "Could not start saved job; will retry");
                return;
            }
        };
        self.events.publish(id).await;
        let context = JobContext {
            job,
            events: self.events.clone(),
            cancel: cancel.clone(),
        };
        let finalized = match kind.handler.run(&context).await {
            Ok(JobOutcome::Completed {
                result_ref,
                message,
            }) => self
                .store()
                .complete(id, &result_ref, &message)
                .await
                .map(drop),
            Ok(JobOutcome::Committed) => Ok(()),
            Ok(JobOutcome::Stopped) => self.stop(id, kind.config.recovery).await,
            // An error raised while stopping is the stop, not a failure of the work.
            Err(_) if cancel.is_cancelled() => self.stop(id, kind.config.recovery).await,
            Err(error) => self.record_failure(id, &kind, &error).await,
        };
        if let Err(error) = finalized {
            tracing::warn!(job_id = id, %error, "Could not finalize job");
        }
        self.events.publish(id).await;
    }

    /// Suspends or interrupts a stopped job. Both are conditional on it still
    /// running, so a committed user cancellation stays.
    async fn stop(&self, id: &str, policy: RecoveryPolicy) -> Result<()> {
        match policy {
            RecoveryPolicy::Requeue => self.store().suspend(id).await,
            RecoveryPolicy::Interrupt => self.store().interrupt(id).await,
        }
        .map(drop)
    }

    async fn record_failure(&self, id: &str, kind: &Kind, error: &AppError) -> Result<()> {
        let job = self.store().get(id).await?;
        match kind.handler.failure(&job, error) {
            JobFailure::Fail { code, message } => {
                tracing::warn!(job_id = id, %error, "Job failed; saved work requires attention");
                self.store().fail(id, &code, &message).await
            }
            JobFailure::Defer(deferral) => {
                tracing::warn!(job_id = id, %error, "Job attempt failed; scheduling automatic retry");
                self.store().defer(id, &deferral, kind.config.backoff).await
            }
        }
        .map(drop)
    }

    /// Saves a job and starts its worker.
    pub async fn submit(self: &Arc<Self>, job: &NewJob) -> Result<JobRecord> {
        let admitted = self.store().submit(job).await?;
        self.submitted(&admitted.id).await;
        self.store().get(&admitted.id).await
    }

    /// For a job a feature saved in its own transaction: publish it and start
    /// its worker.
    pub async fn submitted(self: &Arc<Self>, id: &str) {
        self.events.publish(id).await;
        self.dispatch(id);
    }

    /// Commits the cancellation, then signals the worker.
    pub async fn cancel(&self, id: &str) -> Result<JobRecord> {
        self.store().cancel(id).await?;
        self.cancelled(id).await;
        self.store().get(id).await
    }

    /// For a cancellation a feature committed in its own transaction: signal
    /// the worker, whose later writes can no longer change the saved status.
    pub async fn cancelled(&self, id: &str) {
        if let Some(token) = lock(&self.active).get(id) {
            token.cancel();
        }
        self.events.publish(id).await;
    }

    /// Queues a new attempt of a finished job and starts its worker.
    pub async fn retry(self: &Arc<Self>, id: &str, operation_id: &str) -> Result<JobRecord> {
        let admitted = self.store().retry(id, operation_id, RETRY_QUEUED).await?;
        self.submitted(&admitted.id).await;
        self.store().get(&admitted.id).await
    }

    /// Stops admitting work and cancels every worker. Each reaches its
    /// cancellation point and is suspended or interrupted per its kind.
    pub fn close(&self) {
        self.tasks.close();
    }

    /// Waits for every worker's final write. Call after [`Self::close`] and
    /// before the database closes.
    pub async fn drain(&self) {
        self.tasks.wait().await;
    }

    /// Waits until no worker is running or waiting for `id`.
    #[cfg(test)]
    pub(crate) async fn finished(&self, id: &str) {
        while lock(&self.active).contains_key(id) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
}
