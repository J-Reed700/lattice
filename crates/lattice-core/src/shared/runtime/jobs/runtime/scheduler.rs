use super::super::arrivals::{poll_once, Arrival};
use super::super::rank::{within, RankScope};
use super::super::{
    Backoff, Deferral, JobFailure, JobKindConfig, JobOutcome, JobRecord, RecoveryPolicy,
};
use super::{
    lock, panic_text, until_cancelled, JobContext, JobRuntime, Kind, Subjects, INTERNAL_ERROR,
    INTERNAL_ERROR_MESSAGE, REPEATED_FAILURE, WAITING_TURN,
};
use crate::shared::{
    error::{AppError, ErrorClass, Result},
    ipc::ErrorCode,
};
use futures::FutureExt;
use std::{
    collections::HashMap,
    future::Future,
    sync::{atomic::Ordering, Arc, Mutex, Weak},
    task::Poll,
};
use tokio_util::sync::CancellationToken;

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

/// What a deferred job's last attempt failed with, and a digest of the work it
/// had saved by then.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct LastFailure {
    /// The error's code: its kind, never its text, which may carry a request
    /// id or a slot number that changes on every attempt.
    failure: ErrorCode,
    saved_work: String,
}

impl JobRuntime {
    pub(super) fn start_dispatcher(self: &Arc<Self>) {
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
            .unwrap_or_else(std::sync::PoisonError::into_inner)
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
            if active.contains_key(id) || lock(&self.cancelling).contains_key(id) {
                return;
            }
            // Registered before spawning, so a queued job can be cancelled.
            active.insert(id.to_string(), cancel.clone());
        }
        let registration = Registration {
            active: self.active.clone(),
            id: id.to_string(),
        };
        let arrival = self.arrivals.arrive();
        let runtime = Arc::clone(self);
        let id = id.to_string();
        // Rejected admission at shutdown drops the registration; the job is
        // still pending and resumes at the next start.
        let _ = self.tasks.spawn(async move {
            runtime.work(&id, cancel, arrival).await;
            drop(registration);
            if lock(&runtime.requeued).remove(&id) {
                runtime.dispatch(&id);
            }
            // Wake after dropping `registration` so `dispatch` sees no stale active entry.
            runtime.wake.notify_one();
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

    async fn work(&self, id: &str, cancel: CancellationToken, arrival: Arrival) {
        let job = match self.store().get_summary(id).await {
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
        let mut arrival = Some(arrival);
        let _subject = match &lease {
            Some(lease) => {
                let mut locked =
                    std::pin::pin!(until_cancelled(&cancel, lease.lock.clone().lock_owned()));
                let guard = match poll_once(locked.as_mut()).await {
                    Poll::Ready(guard) => guard,
                    Poll::Pending => {
                        // Waiting for its subject, it lets later workers queue.
                        arrival = None;
                        locked.await
                    }
                };
                match guard {
                    Some(guard) => Some(guard),
                    None => return,
                }
            }
            None => None,
        };
        if let Some(arrival) = &arrival {
            if until_cancelled(&cancel, arrival.turn()).await.is_none() {
                return;
            }
        }
        let mut slot = std::pin::pin!(until_cancelled(&cancel, kind.slots.clone().acquire_owned()));
        let first = poll_once(slot.as_mut()).await;
        // Holding the slot or queued for it: later workers queue behind it.
        drop(arrival);
        let slot = match first {
            Poll::Ready(slot) => slot,
            Poll::Pending => slot.await,
        };
        let Some(Ok(_slot)) = slot else {
            return;
        };
        if cancel.is_cancelled() || lock(&self.cancelling).contains_key(id) {
            return;
        }
        let job = match self.store().claim(id).await {
            Ok(Some(job)) => job,
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(job_id = id, %error, "Could not start saved job; will retry");
                return;
            }
        };
        self.events.publish(id).await;
        let context = JobContext::from_claimed(job, self.events.clone(), cancel.clone());
        let mut panicked = false;
        let rank = context
            .job()
            .rank
            .filter(|_| kind.config.ranked)
            .map(RankScope::new);
        let outcome = match std::panic::AssertUnwindSafe(within(rank, kind.handler.run(&context)))
            .catch_unwind()
            .await
        {
            Ok(outcome) => outcome,
            Err(payload) => {
                tracing::error!(job_id = id, kind = %context.job().kind, panic = %panic_text(payload.as_ref()), "A job handler panicked");
                // A panic on the way out of a cancellation is the stop, as an error is.
                if cancel.is_cancelled() {
                    Ok(JobOutcome::Stopped)
                } else {
                    panicked = true;
                    Ok(JobOutcome::Failed {
                        code: INTERNAL_ERROR.into(),
                        message: INTERNAL_ERROR_MESSAGE.into(),
                    })
                }
            }
        };
        if outcome.is_ok() || cancel.is_cancelled() {
            lock(&self.deferred).remove(id);
        }
        let backoff = kind.config.backoff;
        let finalized = match outcome {
            Ok(JobOutcome::Completed {
                result_ref,
                message,
            }) => self
                .settle(id, backoff, || {
                    self.store().complete(id, &result_ref, &message)
                })
                .await
                .map(drop),
            Ok(JobOutcome::Committed) => Ok(()),
            Ok(JobOutcome::Failed { code, message }) => self
                .settle(id, backoff, || self.store().fail(id, &code, &message))
                .await
                .map(drop),
            Ok(JobOutcome::Stopped) => self.stop(id, kind.config).await,
            // An error raised while stopping is the stop, not a failure of the work.
            Err(_) if cancel.is_cancelled() => self.stop(id, kind.config).await,
            Err(error) => self.record_failure(id, &kind, &error).await,
        };
        match finalized {
            Ok(()) if panicked => self.tell_failed(&kind.handler, id, INTERNAL_ERROR).await,
            Ok(()) => {}
            Err(error) => tracing::warn!(job_id = id, %error, "Could not finalize job"),
        }
        self.events.publish(id).await;
    }

    /// Retries a write that settles a job whose worker has finished, for as
    /// long as the database is only busy. The worker is gone and the
    /// dispatcher never claims a running row, so a dropped write left the job
    /// running, with nothing behind it, until the app restarted. Shutdown
    /// stops the retries; startup recovery then settles the row.
    async fn settle<T, F, Fut>(&self, id: &str, backoff: Backoff, write: F) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        let shutdown = self.tasks.token();
        let mut retries = 0;
        loop {
            match write().await {
                Err(error) if ErrorClass::of(&error).is_temporary() => {
                    tracing::warn!(job_id = id, retries, %error, "Could not record the end of a job; retrying");
                    let wait = tokio::time::sleep(backoff.delay(retries));
                    if until_cancelled(&shutdown, wait).await.is_none() {
                        return Err(error);
                    }
                    retries = retries.saturating_add(1);
                }
                settled => return settled,
            }
        }
    }

    /// Suspends or interrupts a stopped job. Both are conditional on it still
    /// running, so a committed user cancellation stays.
    async fn stop(&self, id: &str, config: JobKindConfig) -> Result<()> {
        let requeued = lock(&self.requeued).contains(id);
        self.settle(id, config.backoff, || async move {
            match config.recovery {
                RecoveryPolicy::Requeue if requeued => self.store().requeue(id, WAITING_TURN).await,
                RecoveryPolicy::Requeue => self.store().suspend(id).await,
                RecoveryPolicy::Interrupt => self.store().interrupt(id).await,
            }
        })
        .await
        .map(drop)
    }

    async fn record_failure(&self, id: &str, kind: &Kind, error: &AppError) -> Result<()> {
        let backoff = kind.config.backoff;
        let job = self.settle(id, backoff, || self.store().get(id)).await?;
        let failure = kind.handler.failure(&job, error);
        let cause = ErrorCode::from(error).to_string();
        let repeated = matches!(failure, JobFailure::Defer(_))
            && self.repeats_without_progress(&job, error, backoff).await?;
        match failure {
            JobFailure::Fail { code, message } => {
                lock(&self.deferred).remove(id);
                tracing::warn!(job_id = id, %error, "Job failed; saved work requires attention");
                self.settle(id, backoff, || {
                    self.store()
                        .fail_because(id, &code, &message, Some(cause.as_str()))
                })
                .await
            }
            JobFailure::Defer(_) if repeated => {
                lock(&self.deferred).remove(id);
                tracing::warn!(
                    job_id = id,
                    code = REPEATED_FAILURE,
                    %error,
                    "job.stopped: the same failure repeated with nothing saved in between"
                );
                let message = format!(
                    "{error} The previous attempt failed the same way and nothing was saved \
                     in between, so Lattice stopped retrying it on its own."
                );
                self.settle(id, backoff, || {
                    self.store()
                        .fail_because(id, REPEATED_FAILURE, &message, Some(cause.as_str()))
                })
                .await
            }
            JobFailure::Defer(deferral) => {
                let attempt = job.retry_count.saturating_add(1);
                tracing::warn!(job_id = id, attempt, %error, "Job attempt failed; scheduling automatic retry");
                let deferral = Deferral {
                    progress_message: format!(
                        "{} · attempt {attempt} · last error: {error}",
                        deferral.progress_message
                    ),
                    ..deferral
                };
                self.settle(id, backoff, || {
                    self.store()
                        .defer_because(id, &deferral, backoff, Some(cause.as_str()))
                })
                .await
            }
        }
        .map(drop)
    }

    /// Whether `error` is what the job's previous attempt was deferred for,
    /// with nothing saved since. An outage never is: it says nothing about the
    /// work. Records this failure as the job's latest either way.
    async fn repeats_without_progress(
        &self,
        job: &JobRecord,
        error: &AppError,
        backoff: Backoff,
    ) -> Result<bool> {
        if ErrorClass::of(error) == ErrorClass::Outage {
            return Ok(false);
        }
        let latest = LastFailure {
            failure: ErrorCode::from(error),
            saved_work: self
                .settle(&job.id, backoff, || self.saved_work(job))
                .await?,
        };
        let mut deferred = lock(&self.deferred);
        let repeated = deferred.get(&job.id) == Some(&latest);
        deferred.insert(job.id.clone(), latest);
        Ok(repeated)
    }

    /// A digest of everything `job` has saved: its checkpoints, in a stable
    /// order, and its staged result. Content, not save times, so a checkpoint
    /// written again unchanged is not progress.
    async fn saved_work(&self, job: &JobRecord) -> Result<String> {
        use sha2::{Digest, Sha256};
        let mut saved: Vec<String> = self
            .store()
            .checkpoints_with_prefix(&job.id, "")
            .await?
            .iter()
            .map(serde_json::Value::to_string)
            .collect();
        saved.sort_unstable();
        saved.extend(job.staged_result.as_ref().map(serde_json::Value::to_string));
        let mut digest = Sha256::new();
        for item in &saved {
            digest.update(item.as_bytes());
            digest.update([0]);
        }
        Ok(hex::encode(digest.finalize()))
    }
}
