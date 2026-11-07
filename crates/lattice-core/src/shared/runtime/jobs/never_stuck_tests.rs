//! A job never hangs the learner or the app: a handler that panics fails its
//! job, and a job that keeps ending the process stops being resumed.
use super::*;
use crate::shared::error::{AppError, Result};
use serde_json::json;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

const KIND: &str = "test.work";
const WAIT: Duration = Duration::from_secs(5);
const INTERNAL_ERROR: &str = "Lattice hit an internal error while working on this. Retry it; if it happens again, Settings \u{2192} Logs has the details.";
const STOPPED_THE_APP: &str =
    "This stopped Lattice twice, so it was not resumed. Retry it to try again.";

async fn pool() -> sqlx::SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::migrate!("../lattice-desktop/migrations")
        .run(&pool)
        .await
        .unwrap();
    pool
}

fn new_job(operation: &str, subject: Option<&str>) -> NewJob {
    NewJob {
        kind: KIND.into(),
        subject_id: subject.map(Into::into),
        operation_id: operation.into(),
        payload_hash: "payload".into(),
        requested: json!({"target": operation}),
        progress_total: 2,
        message: "Queued".into(),
    }
}

struct PanicsFirst(AtomicUsize);

#[async_trait::async_trait]
impl JobHandler for PanicsFirst {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
            panic!("handler exploded");
        }
        Ok(JobOutcome::Completed {
            result_ref: format!("result-{}", context.id()),
            message: "Done".into(),
        })
    }
}

async fn finished(runtime: &JobRuntime, id: &str) {
    tokio::time::timeout(WAIT, runtime.finished(id))
        .await
        .unwrap();
}

#[tokio::test]
async fn a_handler_that_panics_fails_its_job_and_frees_the_kind() -> Result<()> {
    let runtime = JobRuntime::new(pool().await);
    runtime
        .register(
            KIND,
            Arc::new(PanicsFirst(AtomicUsize::new(0))),
            JobKindConfig::new(RecoveryPolicy::Requeue)
                .concurrency(1)
                .exclusive_per_subject(),
        )
        .await?;
    let first = runtime.submit(&new_job("panics", Some("course"))).await?;
    finished(&runtime, &first.id).await;
    let failed = runtime.store().get(&first.id).await?;
    assert_eq!(failed.status, JobStatus::Failed);
    assert_eq!(failed.error_code.as_deref(), Some("internal_error"));
    assert_eq!(failed.error_message.as_deref(), Some(INTERNAL_ERROR));
    assert!(failed.finished_at.is_some());

    let next = runtime.submit(&new_job("after", Some("course"))).await?;
    finished(&runtime, &next.id).await;
    assert_eq!(
        runtime.store().get(&next.id).await?.status,
        JobStatus::Completed,
        "the slot and the subject came back"
    );
    assert!(runtime.store().live(KIND).await?.is_empty());
    Ok(())
}

struct Failing;

#[async_trait::async_trait]
impl JobHandler for Failing {
    async fn run(&self, _: &JobContext) -> Result<JobOutcome> {
        Err(AppError::InvalidInput("bad request".into()))
    }
}

#[tokio::test]
async fn a_handler_that_returns_an_error_still_fails_with_that_error() -> Result<()> {
    let runtime = JobRuntime::new(pool().await);
    runtime
        .register(
            KIND,
            Arc::new(Failing),
            JobKindConfig::new(RecoveryPolicy::Requeue),
        )
        .await?;
    let job = runtime.submit(&new_job("fails", None)).await?;
    finished(&runtime, &job.id).await;
    let failed = runtime.store().get(&job.id).await?;
    assert_eq!(failed.status, JobStatus::Failed);
    assert_eq!(failed.error_code.as_deref(), Some("failed"));
    Ok(())
}

async fn submitted(store: &JobStore, operation: &str) -> String {
    store.submit(&new_job(operation, None)).await.unwrap().id
}

async fn left_running(store: &JobStore, id: &str) {
    assert!(store.claim(id).await.unwrap().is_some());
}

async fn recovered(store: &JobStore) {
    store.recover(KIND, RecoveryPolicy::Requeue).await.unwrap();
}

#[tokio::test]
async fn one_launch_that_finds_a_job_running_resumes_it() -> Result<()> {
    let store = JobStore::new(pool().await);
    let id = submitted(&store, "once").await;
    left_running(&store, &id).await;
    recovered(&store).await;
    let job = store.get(&id).await?;
    assert_eq!(job.status, JobStatus::Pending);
    assert!(job.error_code.is_none());
    Ok(())
}

#[tokio::test]
async fn a_second_launch_with_nothing_saved_between_fails_the_job() -> Result<()> {
    let store = JobStore::new(pool().await);
    let id = submitted(&store, "crashes").await;
    left_running(&store, &id).await;
    recovered(&store).await;
    assert_eq!(store.get(&id).await?.status, JobStatus::Pending);
    left_running(&store, &id).await;
    recovered(&store).await;
    let job = store.get(&id).await?;
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error_code.as_deref(), Some("stopped_the_app"));
    assert_eq!(job.error_message.as_deref(), Some(STOPPED_THE_APP));
    assert!(job.finished_at.is_some());
    Ok(())
}

#[tokio::test]
async fn work_saved_between_two_launches_keeps_the_job_resumable() -> Result<()> {
    let store = JobStore::new(pool().await);
    let checkpointed = submitted(&store, "checkpointed").await;
    let progressed = submitted(&store, "progressed").await;
    for id in [&checkpointed, &progressed] {
        left_running(&store, id).await;
    }
    recovered(&store).await;
    for id in [&checkpointed, &progressed] {
        left_running(&store, id).await;
    }
    assert!(
        store
            .put_checkpoint(&checkpointed, "step", &json!({"done": 1}))
            .await?
    );
    assert!(store.progress(&progressed, 1, "One step", None).await?);
    recovered(&store).await;
    for id in [&checkpointed, &progressed] {
        assert_eq!(store.get(id).await?.status, JobStatus::Pending, "{id}");
    }

    for id in [&checkpointed, &progressed] {
        left_running(&store, id).await;
    }
    recovered(&store).await;
    for id in [&checkpointed, &progressed] {
        let job = store.get(id).await?;
        assert_eq!(job.status, JobStatus::Failed, "{id}");
        assert_eq!(job.error_code.as_deref(), Some("stopped_the_app"));
    }
    Ok(())
}

#[tokio::test]
async fn a_retried_job_starts_counting_afresh() -> Result<()> {
    let store = JobStore::new(pool().await);
    let first = submitted(&store, "crashes").await;
    left_running(&store, &first).await;
    recovered(&store).await;
    left_running(&store, &first).await;
    recovered(&store).await;
    assert_eq!(store.get(&first).await?.status, JobStatus::Failed);

    let retry = store.retry(&first, "retry", "Retry queued").await?.id;
    left_running(&store, &retry).await;
    recovered(&store).await;
    assert_eq!(
        store.get(&retry).await?.status,
        JobStatus::Pending,
        "the first launch after a retry resumes it"
    );
    left_running(&store, &retry).await;
    recovered(&store).await;
    let job = store.get(&retry).await?;
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error_code.as_deref(), Some("stopped_the_app"));
    Ok(())
}

struct Completes;

#[async_trait::async_trait]
impl JobHandler for Completes {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        Ok(JobOutcome::Completed {
            result_ref: format!("result-{}", context.id()),
            message: "Done".into(),
        })
    }
}

fn registration(kind: &str) -> JobKindRegistration {
    JobKindRegistration {
        kind: kind.into(),
        handler: Arc::new(Completes),
        config: JobKindConfig::new(RecoveryPolicy::Requeue),
    }
}

#[tokio::test]
async fn a_kind_that_cannot_be_recovered_does_not_stop_the_others() -> Result<()> {
    const BAD: &str = "test.bad";
    const AFTER: &str = "test.after";
    let pool = pool().await;
    let runtime = JobRuntime::new(pool.clone());
    let store = runtime.store();
    let mut stuck = new_job("stuck", None);
    stuck.kind = BAD.into();
    let stuck = store.submit(&stuck).await?.id;
    let mut waiting = new_job("waiting", None);
    waiting.kind = AFTER.into();
    let waiting = store.submit(&waiting).await?.id;
    for id in [&stuck, &waiting] {
        store.claim(id).await?;
    }
    sqlx::query("CREATE TRIGGER refuse BEFORE UPDATE ON jobs WHEN OLD.kind='test.bad' BEGIN SELECT RAISE(ABORT,'jobs table unreadable'); END")
        .execute(&pool)
        .await
        .unwrap();

    let failures = runtime
        .register_all(vec![registration(BAD), registration(AFTER)])
        .await;
    assert_eq!(
        failures.iter().map(|f| f.kind.as_str()).collect::<Vec<_>>(),
        vec![BAD]
    );
    tokio::time::timeout(WAIT, async {
        while store.get(&waiting).await.unwrap().status != JobStatus::Completed {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        store.get(&waiting).await?.status,
        JobStatus::Completed,
        "the kinds after the failed one are registered and recovered"
    );
    assert_eq!(
        store.get(&stuck).await?.status,
        JobStatus::Running,
        "the failed kind's jobs stay as they were, for the next launch"
    );
    Ok(())
}

#[tokio::test]
async fn registering_every_kind_without_a_failure_reports_none() -> Result<()> {
    let runtime = JobRuntime::new(pool().await);
    let failures = runtime
        .register_all(vec![registration(KIND), registration("test.other")])
        .await;
    assert!(failures.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_kind_that_failed_to_register_is_retried_until_it_does_and_its_jobs_run() -> Result<()> {
    const LATE: &str = "test.late";
    let pool = pool().await;
    let runtime = JobRuntime::new(pool.clone());
    let store = runtime.store();
    let mut saved = new_job("saved", None);
    saved.kind = LATE.into();
    let saved = store.submit(&saved).await?.id;
    store.claim(&saved).await?;
    sqlx::query("CREATE TRIGGER refuse BEFORE UPDATE ON jobs WHEN OLD.kind='test.late' BEGIN SELECT RAISE(ABORT,'jobs table unreadable'); END")
        .execute(&pool)
        .await
        .unwrap();

    let mut late = registration(LATE);
    late.config = late.config.backoff(Backoff {
        initial: Duration::from_millis(10),
        max: Duration::from_millis(20),
    });
    let failures = tokio::time::timeout(WAIT, runtime.register_all(vec![late]))
        .await
        .unwrap();
    assert_eq!(
        failures.iter().map(|f| f.kind.as_str()).collect::<Vec<_>>(),
        vec![LATE]
    );
    assert_eq!(store.get(&saved).await?.status, JobStatus::Running);

    sqlx::query("DROP TRIGGER refuse")
        .execute(&pool)
        .await
        .unwrap();
    tokio::time::timeout(WAIT, async {
        while store.get(&saved).await.unwrap().status != JobStatus::Completed {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the kind registers on its own and its saved job runs");
    Ok(())
}
