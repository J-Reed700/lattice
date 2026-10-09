use super::*;
use crate::shared::error::{AppError, Result};
use serde_json::json;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, Semaphore};

const KIND: &str = "test.work";
const ONCE: &str = "test.once";
const WAIT: Duration = Duration::from_secs(5);

async fn pool() -> sqlx::SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

fn new_job(kind: &str, operation: &str, subject: Option<&str>) -> NewJob {
    NewJob {
        kind: kind.into(),
        subject_id: subject.map(Into::into),
        operation_id: operation.into(),
        payload_hash: "payload".into(),
        requested: json!({"target": operation}),
        progress_total: 2,
        message: "Queued".into(),
    }
}

/// Reports each start, then holds the job until the test releases a permit or
/// cancels it. Records the saved status it observed when it was cancelled.
struct Gate {
    started: mpsc::UnboundedSender<String>,
    release: Arc<Semaphore>,
    cancelled_with: Arc<Mutex<Vec<JobStatus>>>,
}

fn gate() -> (Arc<Gate>, mpsc::UnboundedReceiver<String>) {
    let (started, receiver) = mpsc::unbounded_channel();
    (
        Arc::new(Gate {
            started,
            release: Arc::new(Semaphore::new(0)),
            cancelled_with: Arc::default(),
        }),
        receiver,
    )
}

#[async_trait::async_trait]
impl JobHandler for Gate {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        self.started.send(context.id().to_string()).unwrap();
        match context.until_cancelled(self.release.acquire()).await {
            Some(permit) => {
                permit.unwrap().forget();
                Ok(JobOutcome::Completed {
                    result_ref: format!("result-{}", context.id()),
                    message: "Done".into(),
                })
            }
            None => {
                let saved = context.store().get(context.id()).await?.status;
                self.cancelled_with.lock().unwrap().push(saved);
                Ok(JobOutcome::Stopped)
            }
        }
    }
}

struct Failing(AppError);

#[async_trait::async_trait]
impl JobHandler for Failing {
    async fn run(&self, _: &JobContext) -> Result<JobOutcome> {
        Err(self.0.clone())
    }
}

async fn next(started: &mut mpsc::UnboundedReceiver<String>) -> String {
    tokio::time::timeout(WAIT, started.recv())
        .await
        .unwrap()
        .unwrap()
}

async fn quiet(started: &mut mpsc::UnboundedReceiver<String>) {
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(started.try_recv().is_err(), "no other job may start");
}

async fn finished(runtime: &JobRuntime, id: &str) {
    tokio::time::timeout(WAIT, runtime.finished(id))
        .await
        .unwrap();
}

#[tokio::test]
async fn resubmitting_an_operation_returns_the_saved_job() -> Result<()> {
    let store = JobStore::new(pool().await);
    let first = store
        .submit(&new_job(KIND, "op-1", Some("subject")))
        .await?;
    assert!(first.created);
    let again = store
        .submit(&new_job(KIND, "op-1", Some("subject")))
        .await?;
    assert_eq!(
        again,
        Admitted {
            id: first.id.clone(),
            created: false
        }
    );
    let saved = store.get(&first.id).await?;
    assert_eq!(saved.status, JobStatus::Pending);
    assert_eq!(saved.progress_message, "Queued");
    assert_eq!(saved.requested, json!({"target": "op-1"}));
    assert_eq!(saved.subject_id.as_deref(), Some("subject"));
    assert_eq!(store.list_for_subject(KIND, "subject").await?.len(), 1);
    Ok(())
}

#[tokio::test]
async fn reusing_an_operation_id_with_a_different_payload_is_rejected() -> Result<()> {
    let store = JobStore::new(pool().await);
    store.submit(&new_job(KIND, "op-1", None)).await?;
    let mut changed = new_job(KIND, "op-1", None);
    changed.payload_hash = "different".into();
    assert!(matches!(
        store.submit(&changed).await,
        Err(AppError::InvalidInput(_))
    ));
    assert!(matches!(
        store.submit(&new_job(ONCE, "op-1", None)).await,
        Err(AppError::InvalidInput(_))
    ));
    Ok(())
}

#[tokio::test]
async fn a_job_cancelled_before_it_runs_never_runs() -> Result<()> {
    let runtime = JobRuntime::new(pool().await);
    let (handler, mut started) = gate();
    // Saved before its kind is registered, then cancelled while queued.
    let saved = runtime
        .store()
        .submit(&new_job(KIND, "saved", None))
        .await?;
    runtime.cancel(&saved.id).await?;
    runtime
        .register(
            KIND,
            handler.clone(),
            JobKindConfig::new(RecoveryPolicy::Requeue).exclusive_per_subject(),
        )
        .await?;
    runtime.dispatch(&saved.id);
    finished(&runtime, &saved.id).await;

    // Waiting behind another job of its subject, then cancelled.
    let holder = runtime
        .submit(&new_job(KIND, "holder", Some("course")))
        .await?;
    assert_eq!(next(&mut started).await, holder.id);
    let waiting = runtime
        .submit(&new_job(KIND, "waiting", Some("course")))
        .await?;
    quiet(&mut started).await;
    runtime.cancel(&waiting.id).await?;
    finished(&runtime, &waiting.id).await;
    handler.release.add_permits(1);
    finished(&runtime, &holder.id).await;
    quiet(&mut started).await;

    for id in [&saved.id, &waiting.id] {
        let job = runtime.store().get(id).await?;
        assert_eq!(job.status, JobStatus::Cancelled);
        assert!(job.started_at.is_none(), "a cancelled job is never claimed");
        assert!(job.finished_at.is_some());
    }
    assert_eq!(
        runtime.store().get(&holder.id).await?.status,
        JobStatus::Completed
    );
    assert!(matches!(
        runtime.cancel(&holder.id).await,
        Err(AppError::InvalidState(_))
    ));
    Ok(())
}

#[tokio::test]
async fn cancelling_a_running_job_commits_before_signalling_and_late_writes_lose() -> Result<()> {
    let runtime = JobRuntime::new(pool().await);
    let (handler, mut started) = gate();
    runtime
        .register(
            KIND,
            handler.clone(),
            JobKindConfig::new(RecoveryPolicy::Requeue),
        )
        .await?;
    let job = runtime.submit(&new_job(KIND, "running", None)).await?;
    assert_eq!(next(&mut started).await, job.id);
    let cancelled = runtime.cancel(&job.id).await?;
    assert_eq!(cancelled.status, JobStatus::Cancelled);
    finished(&runtime, &job.id).await;
    assert_eq!(
        *handler.cancelled_with.lock().unwrap(),
        vec![JobStatus::Cancelled],
        "the worker sees the committed status when it is signalled"
    );
    let store = runtime.store();
    assert!(!store.fail(&job.id, "late", "late failure").await?);
    assert!(!store.complete(&job.id, "late", "late result").await?);
    assert!(!store.suspend(&job.id).await?);
    assert!(!store.progress(&job.id, 1, "late", None).await?);
    let saved = store.get(&job.id).await?;
    assert_eq!(saved.status, JobStatus::Cancelled);
    assert!(saved.result_ref.is_none());
    Ok(())
}

#[tokio::test]
async fn a_retry_links_a_new_attempt_that_inherits_saved_work() -> Result<()> {
    let store = JobStore::new(pool().await);
    let first = store.submit(&new_job(KIND, "first", None)).await?.id;
    assert!(matches!(
        store.retry(&first, "too-early", "Retry queued").await,
        Err(AppError::InvalidState(_))
    ));
    assert!(store.claim(&first).await?.is_some());
    assert!(
        store
            .put_checkpoint(&first, "step", &json!({"done": 1}))
            .await?
    );
    assert!(
        store
            .update_staged(&first, |_| Ok(json!({"draft": "saved"})))
            .await?
    );
    assert!(
        store
            .progress(&first, 1, "One step", Some(&json!({"phase": "a"})))
            .await?
    );
    assert!(store.fail(&first, "broken", "The step failed").await?);

    let retry = store.retry(&first, "retry-1", "Retry queued").await?;
    assert!(retry.created);
    let child = store.get(&retry.id).await?;
    assert_eq!(child.retry_of_job_id.as_deref(), Some(first.as_str()));
    assert_eq!(child.status, JobStatus::Pending);
    assert_eq!(child.progress_current, 0);
    assert_eq!(child.progress_message, "Retry queued");
    assert!(child.error_message.is_none());
    assert_eq!(child.staged_result, Some(json!({"draft": "saved"})));
    assert_eq!(child.activity, Some(json!({"phase": "a"})));
    assert_eq!(child.requested, json!({"target": "first"}));
    assert_eq!(
        store.checkpoint(&retry.id, "step").await?,
        Some(json!({"done": 1}))
    );

    // Replaying the operation, or retrying the same attempt again, returns
    // the live attempt instead of starting a second one.
    let replay = store.retry(&first, "retry-1", "Retry queued").await?;
    assert_eq!(
        (replay.id.as_str(), replay.created),
        (retry.id.as_str(), false)
    );
    let again = store.retry(&first, "retry-2", "Retry queued").await?;
    assert_eq!(
        (again.id.as_str(), again.created),
        (retry.id.as_str(), false)
    );
    assert!(matches!(
        store.retry(&retry.id, "first", "Retry queued").await,
        Err(AppError::InvalidInput(_))
    ));

    // Only the newest attempt of a failed chain can be retried.
    assert!(store.claim(&retry.id).await?.is_some());
    assert!(store.fail(&retry.id, "broken", "Failed again").await?);
    assert!(matches!(
        store.retry(&first, "retry-3", "Retry queued").await,
        Err(AppError::InvalidState(_))
    ));
    let third = store.retry(&retry.id, "retry-3", "Retry queued").await?;
    assert_eq!(
        store.get(&third.id).await?.retry_of_job_id.as_deref(),
        Some(retry.id.as_str())
    );
    Ok(())
}

#[tokio::test]
async fn deferred_jobs_wait_for_their_retry_time_and_back_off() -> Result<()> {
    let backoff = Backoff::default();
    let delays: Vec<u64> = (0..6).map(|n| backoff.delay(n).as_secs()).collect();
    assert_eq!(delays, vec![30, 60, 120, 240, 300, 300]);

    let store = JobStore::new(pool().await);
    let outage = Deferral {
        code: "temporarily_unavailable".into(),
        message: "The service is down".into(),
        progress_message: "Waiting to retry".into(),
        activity: Some(json!({"model": "idle"})),
    };
    let later = store.submit(&new_job(KIND, "later", None)).await?.id;
    store.claim(&later).await?;
    let hour = Duration::from_secs(3600);
    assert!(
        store
            .defer(
                &later,
                &outage,
                Backoff {
                    initial: hour,
                    max: hour
                }
            )
            .await?
    );
    let deferred = store.get(&later).await?;
    assert_eq!(deferred.status, JobStatus::Pending);
    assert_eq!(deferred.retry_count, 1);
    assert!(deferred.finished_at.is_none());
    assert_eq!(
        deferred.error_message.as_deref(),
        Some("The service is down")
    );
    assert_eq!(deferred.activity, Some(json!({"model": "idle"})));
    assert!(deferred.retry_not_before.unwrap() > chrono::Utc::now().timestamp_millis());
    assert!(store.due(KIND).await?.is_empty());
    assert!(
        store.claim(&later).await?.is_none(),
        "no hot loop during backoff"
    );

    let now = store.submit(&new_job(KIND, "now", None)).await?.id;
    store.claim(&now).await?;
    let zero = Duration::ZERO;
    for attempt in 1..=3 {
        assert!(
            store
                .defer(
                    &now,
                    &outage,
                    Backoff {
                        initial: zero,
                        max: zero
                    }
                )
                .await?
        );
        assert_eq!(
            store
                .due(KIND)
                .await?
                .iter()
                .map(|job| &job.id)
                .collect::<Vec<_>>(),
            vec![&now]
        );
        let resumed = store.claim(&now).await?.unwrap();
        assert!(resumed.error_message.is_none());
        assert!(resumed.retry_not_before.is_none());
        assert_eq!(
            resumed.retry_count, attempt,
            "outages never exhaust a budget"
        );
    }
    Ok(())
}

#[tokio::test]
async fn transient_errors_defer_and_other_errors_fail() -> Result<()> {
    for (error, expected) in [
        (AppError::Network("lost".into()), JobStatus::Pending),
        (
            AppError::ServiceNotAvailable("down".into()),
            JobStatus::Pending,
        ),
        (
            AppError::InvalidInput("bad request".into()),
            JobStatus::Failed,
        ),
    ] {
        let runtime = JobRuntime::new(pool().await);
        runtime
            .register(
                KIND,
                Arc::new(Failing(error)),
                JobKindConfig::new(RecoveryPolicy::Requeue),
            )
            .await?;
        let job = runtime.submit(&new_job(KIND, "fails", None)).await?;
        finished(&runtime, &job.id).await;
        let saved = runtime.store().get(&job.id).await?;
        assert_eq!(saved.status, expected);
        assert!(saved.error_message.is_some());
        assert_eq!(saved.finished_at.is_some(), expected == JobStatus::Failed);
    }
    Ok(())
}

#[tokio::test]
async fn checkpoints_round_trip_and_are_written_only_while_running() -> Result<()> {
    let store = JobStore::new(pool().await);
    let id = store.submit(&new_job(KIND, "checkpoints", None)).await?.id;
    assert!(!store.put_checkpoint(&id, "lesson/a", &json!(1)).await?);
    store.claim(&id).await?;
    for (key, value) in [
        ("lesson/a", json!(1)),
        ("lesson/b", json!(2)),
        ("other/a", json!(3)),
    ] {
        assert!(store.put_checkpoint(&id, key, &value).await?);
    }
    assert!(
        store
            .put_checkpoint(&id, "lesson/a", &json!({"v": 2}))
            .await?
    );
    assert_eq!(
        store.checkpoint(&id, "lesson/a").await?,
        Some(json!({"v": 2}))
    );
    assert_eq!(store.checkpoint(&id, "missing").await?, None);
    let mut prefixed = store.checkpoints_with_prefix(&id, "lesson/").await?;
    prefixed.sort_by_key(|value| value.to_string());
    assert_eq!(prefixed, vec![json!(2), json!({"v": 2})]);
    assert!(store.complete(&id, "result", "Done").await?);
    assert!(!store.put_checkpoint(&id, "lesson/c", &json!(4)).await?);
    assert!(!store.update_staged(&id, |_| panic!("not running")).await?);
    assert_eq!(store.checkpoint(&id, "other/a").await?, Some(json!(3)));
    Ok(())
}

#[tokio::test]
async fn progress_only_moves_forward_and_drops_stale_activity() -> Result<()> {
    let store = JobStore::new(pool().await);
    let id = store.submit(&new_job(KIND, "progress", None)).await?.id;
    assert!(!store.progress(&id, 1, "not running", None).await?);
    store.claim(&id).await?;
    assert!(
        store
            .progress(&id, 1, "One", Some(&json!({"step": 1})))
            .await?
    );
    assert!(
        store
            .progress(&id, 0, "Older heartbeat", Some(&json!({"step": 0})))
            .await?
    );
    let saved = store.get(&id).await?;
    assert_eq!(
        (saved.progress_current, saved.progress_message.as_str()),
        (1, "One")
    );
    assert_eq!(saved.activity, Some(json!({"step": 1})));
    assert!(store.progress(&id, 0, "Backwards", None).await.is_err());
    assert!(store.progress(&id, 3, "Past the end", None).await.is_err());
    assert!(store.heartbeat(&id).await?);
    assert!(store.complete(&id, "result", "Done").await?);
    let done = store.get(&id).await?;
    assert_eq!(done.progress_current, done.progress_total);
    assert!(!store.heartbeat(&id).await?);
    Ok(())
}

#[tokio::test]
async fn one_subject_runs_one_job_at_a_time_while_other_subjects_proceed() -> Result<()> {
    let runtime = JobRuntime::new(pool().await);
    let (handler, mut started) = gate();
    runtime
        .register(
            KIND,
            handler.clone(),
            JobKindConfig::new(RecoveryPolicy::Requeue).exclusive_per_subject(),
        )
        .await?;
    let first = runtime.submit(&new_job(KIND, "a-1", Some("a"))).await?;
    assert_eq!(next(&mut started).await, first.id);
    let second = runtime.submit(&new_job(KIND, "a-2", Some("a"))).await?;
    let other = runtime.submit(&new_job(KIND, "b-1", Some("b"))).await?;
    assert_eq!(
        next(&mut started).await,
        other.id,
        "another subject does not wait"
    );
    quiet(&mut started).await;
    assert_eq!(
        runtime.store().get(&second.id).await?.status,
        JobStatus::Pending
    );

    handler.release.add_permits(2);
    assert_eq!(next(&mut started).await, second.id);
    handler.release.add_permits(1);
    for job in [&first, &second, &other] {
        finished(&runtime, &job.id).await;
        assert_eq!(
            runtime.store().get(&job.id).await?.status,
            JobStatus::Completed
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_kind_runs_no_more_jobs_than_its_concurrency() -> Result<()> {
    let runtime = JobRuntime::new(pool().await);
    let (handler, mut started) = gate();
    runtime
        .register(
            KIND,
            handler.clone(),
            JobKindConfig::new(RecoveryPolicy::Requeue).concurrency(1),
        )
        .await?;
    let first = runtime.submit(&new_job(KIND, "one", None)).await?;
    assert_eq!(next(&mut started).await, first.id);
    let second = runtime.submit(&new_job(KIND, "two", None)).await?;
    let cancelled = runtime.submit(&new_job(KIND, "three", None)).await?;
    quiet(&mut started).await;
    // A cancelled waiter gives up its place without ever taking a slot.
    runtime.cancel(&cancelled.id).await?;
    finished(&runtime, &cancelled.id).await;
    handler.release.add_permits(1);
    assert_eq!(next(&mut started).await, second.id);
    handler.release.add_permits(1);
    finished(&runtime, &second.id).await;
    quiet(&mut started).await;
    let never_ran = runtime.store().get(&cancelled.id).await?;
    assert_eq!(never_ran.status, JobStatus::Cancelled);
    assert!(never_ran.started_at.is_none());
    Ok(())
}

/// Holds work until it is dropped, and says when.
struct DropSignal(Option<tokio::sync::oneshot::Sender<()>>);

impl Drop for DropSignal {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}

struct Abandons {
    started: mpsc::UnboundedSender<String>,
    dropped: Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
    signal: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    dropped_before_return: Arc<Mutex<Option<bool>>>,
}

#[async_trait::async_trait]
impl JobHandler for Abandons {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        let guard = DropSignal(self.signal.lock().unwrap().take());
        let started = self.started.clone();
        let id = context.id().to_string();
        let outcome = context
            .until_cancelled(async move {
                let _guard = guard;
                started.send(id).unwrap();
                std::future::pending::<()>().await;
            })
            .await;
        assert!(outcome.is_none());
        let mut dropped = self.dropped.lock().unwrap().take().unwrap();
        *self.dropped_before_return.lock().unwrap() = Some(dropped.try_recv().is_ok());
        Ok(JobOutcome::Stopped)
    }
}

#[tokio::test]
async fn cancellation_drops_in_flight_work_before_the_handler_continues() -> Result<()> {
    let runtime = JobRuntime::new(pool().await);
    let (started, mut receiver) = mpsc::unbounded_channel();
    let (signal, dropped) = tokio::sync::oneshot::channel();
    let observed: Arc<Mutex<Option<bool>>> = Arc::default();
    runtime
        .register(
            KIND,
            Arc::new(Abandons {
                started,
                dropped: Mutex::new(Some(dropped)),
                signal: Mutex::new(Some(signal)),
                dropped_before_return: observed.clone(),
            }),
            JobKindConfig::new(RecoveryPolicy::Requeue),
        )
        .await?;
    let job = runtime.submit(&new_job(KIND, "abandoned", None)).await?;
    next(&mut receiver).await;
    runtime.close();
    tokio::time::timeout(WAIT, runtime.drain()).await.unwrap();
    assert_eq!(*observed.lock().unwrap(), Some(true));
    assert_eq!(
        runtime.store().get(&job.id).await?.status,
        JobStatus::Pending
    );
    Ok(())
}

#[tokio::test]
async fn startup_recovery_requeues_or_interrupts_by_kind() -> Result<()> {
    let runtime = JobRuntime::new(pool().await);
    let store = runtime.store();
    let resumable = store.submit(&new_job(KIND, "resume", None)).await?.id;
    let single = store.submit(&new_job(ONCE, "once", None)).await?.id;
    let untouched = store
        .submit(&new_job("test.unregistered", "other", None))
        .await?
        .id;
    for id in [&resumable, &single, &untouched] {
        store.claim(id).await?;
    }
    store.progress(&resumable, 1, "Halfway", None).await?;

    let (resume_handler, mut resumed) = gate();
    let (once_handler, mut once_started) = gate();
    runtime
        .register(
            ONCE,
            once_handler,
            JobKindConfig::new(RecoveryPolicy::Interrupt),
        )
        .await?;
    let interrupted = store.get(&single).await?;
    assert_eq!(interrupted.status, JobStatus::Interrupted);
    assert!(interrupted.finished_at.is_some());
    assert_eq!(interrupted.error_code.as_deref(), Some("interrupted"));
    assert!(interrupted.result_ref.is_none());
    quiet(&mut once_started).await;

    runtime
        .register(
            KIND,
            resume_handler.clone(),
            JobKindConfig::new(RecoveryPolicy::Requeue),
        )
        .await?;
    assert_eq!(
        next(&mut resumed).await,
        resumable,
        "requeued work runs again"
    );
    let running = store.get(&resumable).await?;
    assert_eq!(running.progress_current, 1, "saved progress is kept");
    assert!(running.error_message.is_none());
    assert_eq!(store.get(&untouched).await?.status, JobStatus::Running);
    assert!(matches!(
        runtime
            .register(
                KIND,
                resume_handler,
                JobKindConfig::new(RecoveryPolicy::Requeue)
            )
            .await,
        Err(AppError::InvalidState(_))
    ));
    Ok(())
}

#[tokio::test]
async fn shutdown_drains_workers_and_keeps_resumable_work_pending() -> Result<()> {
    let runtime = JobRuntime::new(pool().await);
    let (resume_handler, mut resumed) = gate();
    let (once_handler, mut once_started) = gate();
    runtime
        .register(
            KIND,
            resume_handler,
            JobKindConfig::new(RecoveryPolicy::Requeue),
        )
        .await?;
    runtime
        .register(
            ONCE,
            once_handler,
            JobKindConfig::new(RecoveryPolicy::Interrupt),
        )
        .await?;
    let resumable = runtime.submit(&new_job(KIND, "resume", None)).await?;
    let single = runtime.submit(&new_job(ONCE, "once", None)).await?;
    next(&mut resumed).await;
    next(&mut once_started).await;

    runtime.close();
    tokio::time::timeout(WAIT, runtime.drain()).await.unwrap();
    let suspended = runtime.store().get(&resumable.id).await?;
    assert_eq!(suspended.status, JobStatus::Pending);
    assert!(suspended.finished_at.is_none());
    assert_eq!(
        runtime.store().get(&single.id).await?.status,
        JobStatus::Interrupted
    );

    // After close, work is saved for the next start but not started.
    let queued = runtime.submit(&new_job(KIND, "after-close", None)).await?;
    finished(&runtime, &queued.id).await;
    quiet(&mut resumed).await;
    assert_eq!(
        runtime.store().get(&queued.id).await?.status,
        JobStatus::Pending
    );
    Ok(())
}

#[tokio::test]
async fn observers_see_each_transition_of_a_job() -> Result<()> {
    let runtime = JobRuntime::new(pool().await);
    let seen: Arc<Mutex<Vec<(String, JobStatus)>>> = Arc::default();
    let sink = seen.clone();
    runtime.observe(move |job| {
        sink.lock().unwrap().push((job.id.clone(), job.status));
    });
    let (handler, mut started) = gate();
    runtime
        .register(
            KIND,
            handler.clone(),
            JobKindConfig::new(RecoveryPolicy::Requeue),
        )
        .await?;
    // Let the dispatcher's start-up pass finish, so only this submission
    // starts the worker and the pending state is published first.
    tokio::time::sleep(Duration::from_millis(20)).await;
    let job = runtime.submit(&new_job(KIND, "observed", None)).await?;
    next(&mut started).await;
    handler.release.add_permits(1);
    finished(&runtime, &job.id).await;
    let statuses: Vec<JobStatus> = seen
        .lock()
        .unwrap()
        .iter()
        .filter(|(id, _)| *id == job.id)
        .map(|(_, status)| *status)
        .collect();
    assert_eq!(
        statuses,
        vec![JobStatus::Pending, JobStatus::Running, JobStatus::Completed]
    );
    let dto = JobDto::from(&runtime.store().get(&job.id).await?);
    let wire = serde_json::to_value(&dto).unwrap();
    assert_eq!(wire["status"], "completed");
    assert_eq!(wire["resultRef"], format!("result-{}", job.id));
    Ok(())
}

#[tokio::test]
async fn startup_recovery_waits_for_a_concurrent_writer() -> Result<()> {
    let directory = tempfile::tempdir().unwrap();
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(directory.path().join("startup-recovery.sqlite"))
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5));
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let store = JobStore::new(pool.clone());
    let id = store.submit(&new_job(KIND, "running", None)).await?.id;
    store.claim(&id).await?;
    let mut writer = pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    sqlx::query("UPDATE jobs SET progress_message='Concurrent startup write' WHERE id=?")
        .bind(&id)
        .execute(&mut *writer)
        .await
        .unwrap();
    let recovery = store.recover(KIND, RecoveryPolicy::Requeue);
    tokio::pin!(recovery);
    tokio::select! {
        result = &mut recovery => panic!("Recovery must wait for the writer, not abort app startup: {result:?}"),
        _ = tokio::time::sleep(Duration::from_millis(100)) => {},
    }
    writer.commit().await.unwrap();
    assert_eq!(recovery.await?, vec![id.clone()]);
    let recovered = store.get(&id).await?;
    assert_eq!(recovered.status, JobStatus::Pending);
    assert!(recovered.result_ref.is_none());
    Ok(())
}
