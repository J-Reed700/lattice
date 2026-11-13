//! What counts as saved work for the crash-loop guard, what an orderly exit
//! does to the count, a panic while stopping, and the feature's own word when
//! the runtime fails a job itself.
use super::*;
use crate::shared::error::Result;
use crate::shared::persistence::Database;
use serde_json::json;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

const KIND: &str = "test.work";
const WAIT: Duration = Duration::from_secs(5);

async fn pool() -> sqlx::SqlitePool {
    crate::shared::testing::sqlite::migrated_in_memory()
        .await
        .unwrap()
}

async fn database() -> Database {
    Database::new(pool().await)
}

fn new_job(operation: &str) -> NewJob {
    NewJob {
        kind: KIND.into(),
        subject_id: None,
        operation_id: operation.into(),
        payload_hash: "payload".into(),
        requested: json!({"target": operation}),
        progress_total: 3,
        message: "Queued".into(),
    }
}

async fn submitted(store: &JobStore, operation: &str) -> String {
    store.submit(&new_job(operation)).await.unwrap().id
}

async fn left_running(store: &JobStore, id: &str) {
    assert!(store.claim(id).await.unwrap().is_some());
}

async fn recovered(store: &JobStore) {
    store.recover(KIND, RecoveryPolicy::Requeue).await.unwrap();
}

async fn status(store: &JobStore, id: &str) -> JobStatus {
    store.get(id).await.unwrap().status
}

#[tokio::test]
async fn progress_messages_alone_are_not_saved_work() -> Result<()> {
    let store = JobStore::new(database().await);
    let id = submitted(&store, "reads").await;
    for launch in 0..2 {
        left_running(&store, &id).await;
        let context = JobContext::detached(&store, &id).await?;
        assert!(context.progress(0, "Reading sources", None).await?);
        recovered(&store).await;
        let expected = if launch == 0 {
            JobStatus::Pending
        } else {
            JobStatus::Failed
        };
        assert_eq!(status(&store, &id).await, expected, "launch {launch}");
    }
    assert_eq!(
        store.get(&id).await?.error_code.as_deref(),
        Some("stopped_the_app")
    );
    Ok(())
}

#[tokio::test]
async fn progress_that_rose_is_saved_work_and_a_message_after_it_is_not() -> Result<()> {
    let store = JobStore::new(database().await);
    let id = submitted(&store, "advances").await;
    left_running(&store, &id).await;
    let context = JobContext::detached(&store, &id).await?;
    assert!(context.progress(1, "Read the first source", None).await?);
    recovered(&store).await;
    assert_eq!(status(&store, &id).await, JobStatus::Pending);

    left_running(&store, &id).await;
    let context = JobContext::detached(&store, &id).await?;
    assert!(context.progress(2, "Read the second source", None).await?);
    recovered(&store).await;
    assert_eq!(
        status(&store, &id).await,
        JobStatus::Pending,
        "progress that rose since the last launch is saved work"
    );

    left_running(&store, &id).await;
    let context = JobContext::detached(&store, &id).await?;
    assert!(context.progress(2, "Writing the lesson", None).await?);
    recovered(&store).await;
    assert_eq!(status(&store, &id).await, JobStatus::Failed);
    Ok(())
}

#[tokio::test]
async fn rewriting_a_checkpoint_with_the_same_content_is_not_saved_work() -> Result<()> {
    let store = JobStore::new(database().await);
    let id = submitted(&store, "scratch").await;
    left_running(&store, &id).await;
    assert!(
        store
            .put_checkpoint(&id, "scratch", &json!({"n": 1}))
            .await?
    );
    recovered(&store).await;
    assert_eq!(status(&store, &id).await, JobStatus::Pending);

    left_running(&store, &id).await;
    assert!(
        store
            .put_checkpoint(&id, "scratch", &json!({"n": 1}))
            .await?
    );
    recovered(&store).await;
    let job = store.get(&id).await?;
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error_code.as_deref(), Some("stopped_the_app"));
    Ok(())
}

#[tokio::test]
async fn changing_a_checkpoint_is_saved_work() -> Result<()> {
    let store = JobStore::new(database().await);
    let id = submitted(&store, "scratch").await;
    left_running(&store, &id).await;
    assert!(
        store
            .put_checkpoint(&id, "scratch", &json!({"n": 1}))
            .await?
    );
    recovered(&store).await;

    left_running(&store, &id).await;
    assert!(
        store
            .put_checkpoint(&id, "scratch", &json!({"n": 2}))
            .await?
    );
    recovered(&store).await;
    assert_eq!(status(&store, &id).await, JobStatus::Pending);

    left_running(&store, &id).await;
    assert!(store.put_checkpoint(&id, "other", &json!({"n": 1})).await?);
    recovered(&store).await;
    assert_eq!(
        status(&store, &id).await,
        JobStatus::Pending,
        "a new key is new content"
    );
    Ok(())
}

#[tokio::test]
async fn restaging_the_same_result_is_not_saved_work_and_a_new_one_is() -> Result<()> {
    let store = JobStore::new(database().await);
    let id = submitted(&store, "staged").await;
    left_running(&store, &id).await;
    assert!(
        store
            .update_staged(&id, |_| Ok(json!({"draft": "a"})))
            .await?
    );
    recovered(&store).await;

    left_running(&store, &id).await;
    assert!(
        store
            .update_staged(&id, |_| Ok(json!({"draft": "b"})))
            .await?
    );
    recovered(&store).await;
    assert_eq!(status(&store, &id).await, JobStatus::Pending);

    left_running(&store, &id).await;
    assert!(
        store
            .update_staged(&id, |_| Ok(json!({"draft": "b"})))
            .await?
    );
    recovered(&store).await;
    assert_eq!(status(&store, &id).await, JobStatus::Failed);
    Ok(())
}

/// Ignores its cancellation: it is still working when the app has to leave.
struct Deaf {
    started: mpsc::UnboundedSender<String>,
}

#[async_trait::async_trait]
impl JobHandler for Deaf {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        self.started.send(context.id().to_string()).unwrap();
        std::future::pending().await
    }
}

#[tokio::test]
async fn an_orderly_shutdown_suspends_a_job_that_was_slow_to_stop_without_counting_it() -> Result<()>
{
    let runtime = JobRuntime::new(database().await);
    let store = runtime.store();
    let id = submitted(store, "long").await;
    // One earlier launch already found it running.
    left_running(store, &id).await;
    recovered(store).await;

    let (started, mut running) = mpsc::unbounded_channel();
    runtime
        .register(
            KIND,
            Arc::new(Deaf { started }),
            JobKindConfig::new(RecoveryPolicy::Requeue),
        )
        .await?;
    assert_eq!(
        tokio::time::timeout(WAIT, running.recv())
            .await
            .unwrap()
            .unwrap(),
        id
    );
    assert_eq!(status(store, &id).await, JobStatus::Running);

    runtime.close();
    assert!(
        tokio::time::timeout(Duration::from_millis(100), runtime.drain())
            .await
            .is_err(),
        "the worker does not stop"
    );
    runtime.suspend_running().await?;
    let job = store.get(&id).await?;
    assert_eq!(job.status, JobStatus::Pending);
    assert!(job.error_code.is_none());

    // The next launch finds it, resumes it, and has nothing to count.
    left_running(store, &id).await;
    recovered(store).await;
    assert_eq!(status(store, &id).await, JobStatus::Pending);
    left_running(store, &id).await;
    recovered(store).await;
    assert_eq!(
        status(store, &id).await,
        JobStatus::Failed,
        "only a real second crash fails it"
    );
    Ok(())
}

/// Panics on its way out of a cancellation.
struct PanicsWhenCancelled {
    started: mpsc::UnboundedSender<String>,
}

#[async_trait::async_trait]
impl JobHandler for PanicsWhenCancelled {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        self.started.send(context.id().to_string()).unwrap();
        context.cancellation().cancelled().await;
        panic!("cancel path");
    }
}

#[tokio::test]
async fn a_panic_while_the_job_is_being_cancelled_is_a_stop_not_a_failure() -> Result<()> {
    let runtime = JobRuntime::new(database().await);
    let (started, mut running) = mpsc::unbounded_channel();
    runtime
        .register(
            KIND,
            Arc::new(PanicsWhenCancelled { started }),
            JobKindConfig::new(RecoveryPolicy::Requeue),
        )
        .await?;
    let job = runtime.submit(&new_job("stops")).await?;
    tokio::time::timeout(WAIT, running.recv())
        .await
        .unwrap()
        .unwrap();

    runtime.close();
    tokio::time::timeout(WAIT, runtime.drain()).await.unwrap();
    let saved = runtime.store().get(&job.id).await?;
    assert_eq!(saved.status, JobStatus::Pending, "suspended, so it resumes");
    assert!(saved.error_code.is_none());
    Ok(())
}

/// Reports every job the runtime fails on its behalf, with the status the
/// store held when it was told.
struct Reports {
    store: JobStore,
    panics: bool,
    told: Mutex<Vec<(String, String, JobStatus)>>,
}

#[async_trait::async_trait]
impl JobHandler for Reports {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        if self.panics {
            panic!("handler exploded");
        }
        Ok(JobOutcome::Completed {
            result_ref: format!("result-{}", context.id()),
            message: "Done".into(),
        })
    }

    async fn failed_by_runtime(&self, job: &JobRecord, code: &str) {
        let stored = self.store.get(&job.id).await.unwrap().status;
        self.told
            .lock()
            .unwrap()
            .push((job.id.clone(), code.to_string(), stored));
    }
}

async fn told(handler: &Reports) -> Vec<(String, String, JobStatus)> {
    tokio::time::timeout(WAIT, async {
        loop {
            let told = handler.told.lock().unwrap().clone();
            if !told.is_empty() {
                return told;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the handler was never told")
}

#[tokio::test]
async fn a_handler_is_told_when_the_runtime_fails_its_job_for_a_panic() -> Result<()> {
    let runtime = JobRuntime::new(database().await);
    let handler = Arc::new(Reports {
        store: runtime.store().clone(),
        panics: true,
        told: Mutex::default(),
    });
    runtime
        .register(
            KIND,
            handler.clone(),
            JobKindConfig::new(RecoveryPolicy::Requeue),
        )
        .await?;
    let job = runtime.submit(&new_job("panics")).await?;
    assert_eq!(
        told(&handler).await,
        vec![(job.id, "internal_error".into(), JobStatus::Failed)]
    );
    Ok(())
}

#[tokio::test]
async fn a_handler_is_told_when_recovery_fails_its_job_for_stopping_the_app() -> Result<()> {
    let runtime = JobRuntime::new(database().await);
    let store = runtime.store();
    let id = submitted(store, "crashes").await;
    left_running(store, &id).await;
    recovered(store).await;
    left_running(store, &id).await;

    let handler = Arc::new(Reports {
        store: store.clone(),
        panics: false,
        told: Mutex::default(),
    });
    runtime
        .register(
            KIND,
            handler.clone(),
            JobKindConfig::new(RecoveryPolicy::Requeue),
        )
        .await?;
    assert_eq!(
        told(&handler).await,
        vec![(id, "stopped_the_app".into(), JobStatus::Failed)]
    );
    Ok(())
}

#[tokio::test]
async fn progress_climbing_back_to_a_height_already_reached_is_not_saved_work() -> Result<()> {
    let store = JobStore::new(database().await);
    let id = submitted(&store, "regrades").await;
    for attempt in 0..2 {
        left_running(&store, &id).await;
        let context = JobContext::detached(&store, &id).await?;
        // Each attempt starts its progress over, then climbs back to the same point.
        let _ = context.progress(0, "Fetching the page", None).await;
        let _ = context.progress(1, "Reading the page", None).await;
        recovered(&store).await;
        let expected = if attempt == 0 {
            JobStatus::Pending
        } else {
            JobStatus::Failed
        };
        assert_eq!(status(&store, &id).await, expected, "attempt {attempt}");
    }
    assert_eq!(
        store.get(&id).await?.error_code.as_deref(),
        Some("stopped_the_app")
    );
    Ok(())
}

#[tokio::test]
async fn progress_past_the_highest_value_ever_reached_is_saved_work() -> Result<()> {
    let store = JobStore::new(database().await);
    let id = submitted(&store, "advances").await;
    for (reached, expected) in [(1, JobStatus::Pending), (1, JobStatus::Failed)] {
        left_running(&store, &id).await;
        let context = JobContext::detached(&store, &id).await?;
        let _ = context.progress(0, "Fetching the page", None).await;
        let _ = context.progress(reached, "Reading the page", None).await;
        recovered(&store).await;
        assert_eq!(status(&store, &id).await, expected);
    }

    let other = submitted(&store, "climbs").await;
    for (reached, expected) in [(1, JobStatus::Pending), (2, JobStatus::Pending)] {
        left_running(&store, &other).await;
        let context = JobContext::detached(&store, &other).await?;
        let _ = context.progress(0, "Fetching the page", None).await;
        let _ = context.progress(reached, "Reading the page", None).await;
        recovered(&store).await;
        assert_eq!(
            status(&store, &other).await,
            expected,
            "a rise past the old high is saved work"
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_orderly_shutdown_leaves_a_job_of_an_interrupt_kind_to_be_interrupted() -> Result<()> {
    let runtime = JobRuntime::new(database().await);
    let store = runtime.store();
    let (started, mut running) = mpsc::unbounded_channel();
    runtime
        .register(
            KIND,
            Arc::new(Deaf { started }),
            JobKindConfig::new(RecoveryPolicy::Interrupt),
        )
        .await?;
    let job = runtime.submit(&new_job("once")).await?;
    tokio::time::timeout(WAIT, running.recv())
        .await
        .unwrap()
        .unwrap();
    runtime.close();
    assert!(
        tokio::time::timeout(Duration::from_millis(100), runtime.drain())
            .await
            .is_err()
    );
    runtime.suspend_running().await?;

    store.recover(KIND, RecoveryPolicy::Interrupt).await?;
    let saved = store.get(&job.id).await?;
    assert_eq!(
        saved.status,
        JobStatus::Interrupted,
        "work that cannot resume is interrupted, not run again"
    );
    assert_eq!(saved.error_code.as_deref(), Some("interrupted"));
    Ok(())
}
