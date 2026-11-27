//! Delivers saved jobs to their kind's handler, within the kind's limits.
mod cancellation;
mod context;
mod scheduler;
#[cfg(test)]
mod tests;

pub use context::{JobContext, JobHandler};

use self::context::Events;
use self::scheduler::LastFailure;
use super::arrivals::Arrivals;
use super::repository::STOPPED_THE_APP;
use super::{JobDto, JobKindConfig, JobRecord, JobStatus, JobStore, NewJob, RecoveryPolicy};
use crate::shared::{
    error::{AppError, Result},
    persistence::Database,
    runtime::background::BackgroundTasks,
};
use futures::FutureExt;
use std::{
    collections::{HashMap, HashSet},
    future::Future,
    sync::{atomic::AtomicBool, Arc, Mutex, MutexGuard, PoisonError, RwLock, Weak},
    time::Duration,
};
use tokio::sync::{Notify, Semaphore};
use tokio_util::sync::CancellationToken;

const POLL: Duration = Duration::from_secs(30);
const RETRY_QUEUED: &str = "Retry queued";
const WAITING_TURN: &str = "Waiting for its turn";
/// The error code of a job ended because its failure repeated with nothing
/// saved in between.
pub const REPEATED_FAILURE: &str = "repeated_failure";
/// The error code of a job whose handler panicked.
pub const INTERNAL_ERROR: &str = "internal_error";
const INTERNAL_ERROR_MESSAGE: &str = "Lattice hit an internal error while working on this. Retry it; if it happens again, Settings → Logs has the details.";

type Observer = Arc<dyn Fn(&JobDto) + Send + Sync>;
type Subjects = Arc<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>;

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a non-text panic payload".to_string())
}

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

struct Kind {
    handler: Arc<dyn JobHandler>,
    config: JobKindConfig,
    slots: Arc<Semaphore>,
}

/// The application's job runtime, owned by the composition root. Workers run
/// as its own background tasks: [`JobRuntime::close`] cancels them and
/// [`JobRuntime::drain`] waits for their final writes.
///
/// A deferred job is never retried forever without anyone seeing it: each
/// deferral shows its attempt number and last error, and a failure other than
/// an outage that repeats with nothing saved in between ends the job. The
/// same work failing the same way twice is deterministic; a third attempt
/// would only spend another backoff to say so again.
pub struct JobRuntime {
    events: Arc<Events>,
    tasks: BackgroundTasks,
    kinds: RwLock<HashMap<String, Arc<Kind>>>,
    active: Arc<Mutex<HashMap<String, CancellationToken>>>,
    /// Jobs with a cancellation currently in flight (checked under `active`
    /// first so lock ordering stays canonical and dispatch cannot race).
    cancelling: Mutex<HashMap<String, usize>>,
    /// Jobs whose worker was stopped to give up its turn; each queues again
    /// once that worker has ended.
    requeued: Mutex<HashSet<String>>,
    arrivals: Arc<Arrivals>,
    subjects: Subjects,
    wake: Arc<Notify>,
    poll: Duration,
    dispatching: AtomicBool,
    /// Each deferred job's last failure. Held only while the job is deferred,
    /// and only in this process: after a restart a job gets one more attempt.
    deferred: Mutex<HashMap<String, LastFailure>>,
}

impl JobRuntime {
    pub fn new(database: Database) -> Arc<Self> {
        Self::with_poll_interval(database, POLL)
    }

    /// The saved pending rows are the outbox. Besides wakeups, the dispatcher
    /// rereads them on this interval, so a missed wakeup, a transient database
    /// error or a deferred job's retry time cannot strand work.
    pub fn with_poll_interval(database: Database, poll: Duration) -> Arc<Self> {
        Arc::new(Self {
            events: Arc::new(Events::new(JobStore::new(database))),
            tasks: BackgroundTasks::default(),
            kinds: RwLock::default(),
            active: Arc::default(),
            cancelling: Mutex::default(),
            requeued: Mutex::default(),
            arrivals: Arc::default(),
            subjects: Arc::default(),
            wake: Arc::new(Notify::new()),
            poll,
            dispatching: AtomicBool::new(false),
            deferred: Mutex::default(),
        })
    }

    pub fn store(&self) -> &JobStore {
        self.events.store()
    }

    /// Receives every job's state after each transition and progress write.
    pub fn observe(&self, observer: impl Fn(&JobDto) + Send + Sync + 'static) {
        self.events.observe(Arc::new(observer));
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
        let recovered = self.store().recover_settling(kind, config.recovery).await?;
        let told = handler.clone();
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
        for id in &recovered.stopped {
            self.tell_failed(&told, id, STOPPED_THE_APP).await;
        }
        for id in recovered.settled {
            self.events.publish(&id).await;
        }
        self.start_dispatcher();
        self.wake.notify_one();
        Ok(())
    }

    /// Tells `handler` that the runtime failed job `id` with `code`, after the
    /// failure was stored. A hook that fails or panics is logged, never fatal.
    async fn tell_failed(&self, handler: &Arc<dyn JobHandler>, id: &str, code: &str) {
        let job = match self.store().get(id).await {
            Ok(job) if job.status == JobStatus::Failed => job,
            Ok(_) => return,
            Err(error) => {
                tracing::warn!(job_id = id, %error, "Could not read a failed job to tell its handler");
                return;
            }
        };
        if let Err(payload) = std::panic::AssertUnwindSafe(handler.failed_by_runtime(&job, code))
            .catch_unwind()
            .await
        {
            tracing::error!(job_id = id, panic = %panic_text(payload.as_ref()), "A job handler panicked while told its job failed");
        }
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

    /// For an orderly exit that did not wait for every worker: suspends, in
    /// one statement, the running jobs of the kinds registered to resume, and
    /// clears the unsaved-start count of the rest, which the next recovery
    /// settles by their kind's policy. Returns the jobs touched.
    pub async fn suspend_running(&self) -> Result<usize> {
        let resumable: Vec<String> = self
            .kinds
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|(_, kind)| kind.config.recovery == RecoveryPolicy::Requeue)
            .map(|(name, _)| name.clone())
            .collect();
        let resumable: Vec<&str> = resumable.iter().map(String::as_str).collect();
        self.store().suspend_all_running(&resumable).await
    }

    /// Waits until no worker is running or waiting for `id`.
    pub async fn finished(&self, id: &str) {
        while lock(&self.active).contains_key(id) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    /// Registers each kind, returning at once with the kinds that could not
    /// be. Each of those keeps being tried in the background, on its own
    /// backoff, until it registers, so its saved jobs run once the cause has
    /// passed. Nothing here waits on a failed kind.
    pub async fn register_all(
        self: &Arc<Self>,
        kinds: Vec<JobKindRegistration>,
    ) -> Vec<RegistrationFailure> {
        let mut failures = Vec::new();
        for registration in kinds {
            let Err(error) = self
                .register(
                    &registration.kind,
                    registration.handler.clone(),
                    registration.config,
                )
                .await
            else {
                continue;
            };
            tracing::warn!(kind = %registration.kind, %error, "Could not register a job kind; retrying in the background");
            self.retry_registration(&registration);
            failures.push(RegistrationFailure {
                kind: registration.kind,
                error,
            });
        }
        failures
    }

    fn retry_registration(self: &Arc<Self>, registration: &JobKindRegistration) {
        let runtime = Arc::downgrade(self);
        let cancel = self.tasks.token();
        let kind = registration.kind.clone();
        let handler = registration.handler.clone();
        let config = registration.config;
        let _ = self.tasks.spawn(async move {
            let mut retries = 0u32;
            loop {
                let wait = tokio::time::sleep(config.backoff.delay(retries));
                if until_cancelled(&cancel, wait).await.is_none() {
                    return;
                }
                let Some(runtime) = Weak::upgrade(&runtime) else {
                    return;
                };
                match runtime.register(&kind, handler.clone(), config).await {
                    Ok(()) => {
                        tracing::info!(%kind, retries, "Registered a job kind that had failed to");
                        return;
                    }
                    Err(AppError::InvalidState(_)) if runtime.kind(&kind).is_some() => return,
                    Err(error) => {
                        tracing::warn!(%kind, %error, retries, "Could not register a job kind; retrying in the background");
                        retries = retries.saturating_add(1);
                    }
                }
            }
        });
    }
}

/// One kind to register at startup.
pub struct JobKindRegistration {
    pub kind: String,
    pub handler: Arc<dyn JobHandler>,
    pub config: JobKindConfig,
}

/// A kind that could not be registered, and why.
#[derive(Debug)]
pub struct RegistrationFailure {
    pub kind: String,
    pub error: AppError,
}
