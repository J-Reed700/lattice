//! A build that stopped the app stays failed, visible, until the learner
//! retries it: no start-up pass or reader queues the same course again.

use super::tests::{builds, course, indexes, pages};
use super::*;
use crate::infrastructure::passage_index::testing::{FakeEmbedder, FixedEmbedders};
use lattice_core::shared::runtime::jobs::{JobStatus, RecoveryPolicy};
use lattice_core::shared::Result;
use std::time::Duration;

#[tokio::test]
async fn a_build_that_stopped_the_app_stays_failed_until_the_learner_retries_it() -> Result<()> {
    // A build running as a job saves progress beside its own reads, which the
    // one-connection test database cannot do.
    let temp = tempfile::tempdir().unwrap();
    let pool = first_tests::pool(temp.path()).await?;
    let courses = indexes(
        &pool,
        &temp.path().join("course-index"),
        FixedEmbedders::new(Some(FakeEmbedder::new("model-a"))),
    );
    let (program, _) = course(&pool, &pages(2)).await?;
    // No build handler is registered yet, so the queued build stays queued.
    courses.refresh(&program).await;
    let queued = builds(&pool, &program).await;
    assert_eq!(queued.len(), 1);
    let id = queued[0].id.clone();
    let store = courses.jobs.store();
    for _ in 0..2 {
        store.claim(&id).await?.unwrap();
        store.recover(COURSE_INDEX, RecoveryPolicy::Requeue).await?;
    }
    let stopped = store.get(&id).await?;
    assert_eq!(stopped.status, JobStatus::Failed);
    assert_eq!(stopped.error_code.as_deref(), Some("stopped_the_app"));

    // The start-up pass, a reader's refresh and a status read.
    courses.backfill().await?;
    courses.refresh(&program).await;
    courses.status(&program).await;

    let after = builds(&pool, &program).await;
    assert_eq!(
        after.iter().map(|job| job.id.as_str()).collect::<Vec<_>>(),
        [id.as_str()],
        "the failure is kept and nothing else is queued"
    );
    assert_eq!(after[0].status, JobStatus::Failed);
    assert_eq!(after[0].error_code.as_deref(), Some("stopped_the_app"));

    // The learner's Retry is the one way it starts again.
    let failures = courses.jobs.register_all(vec![courses.job_kind()]).await;
    assert!(failures.is_empty(), "{failures:?}");
    let retried = courses.jobs.retry(&id, "learner-retry").await?;
    let finished = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let job = courses.jobs.store().get(&retried.id).await.unwrap();
            if job.status.is_finished() {
                return job;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the retried build never finished");
    assert_eq!(finished.status, JobStatus::Completed);
    Ok(())
}
