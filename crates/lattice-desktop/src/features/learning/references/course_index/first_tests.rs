#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
//! The course that goes first: a course opened in its workspace takes the
//! build slot from another course's build, which gives up its turn at its
//! next batch and finishes after it, whether the course was opened before
//! or after its build was queued.

use super::tests::{builds, course, indexes};
use super::*;
use crate::features::learning::tests;
use crate::infrastructure::passage_index::build::BATCH_SIZE;
use crate::infrastructure::passage_index::testing::{BatchGate, FakeEmbedder, FixedEmbedders};
use lattice_core::shared::{AppError, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// A database on disk with room for a build's progress writes beside its
/// reads, as the app's has: the one-connection test database would make a
/// running build's progress write wait on the build's own read.
async fn pool(dir: &Path) -> Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(dir.join("lattice.db"))
        .create_if_missing(true)
        .busy_timeout(Duration::from_secs(5))
        .pragma("journal_mode", "WAL");
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await
        .map_err(|error| AppError::Database(error.to_string()))?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|error| AppError::Database(error.to_string()))?;
    Ok(pool)
}

fn texts(word: &str, count: usize) -> Vec<String> {
    (0..count)
        .map(|i| format!("{word} topic {i} covers {word}{i} in depth."))
        .collect()
}

/// Waits until every build of `program` has finished.
async fn finished(pool: &SqlitePool, program: &str) {
    for _ in 0..2_000 {
        let jobs = builds(pool, program).await;
        if !jobs.is_empty() && jobs.iter().all(|job| job.status.is_finished()) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("the builds of {program} did not finish");
}

/// Course A builds in the background and is held inside its second batch;
/// course B is opened and queued (in `order`), then the model is let go.
async fn opened_course_goes_first(open_before_queueing: bool) -> Result<()> {
    let temp = tempfile::tempdir().unwrap();
    let pool = pool(temp.path()).await?;
    let model = FakeEmbedder::new("model-a");
    let gate = BatchGate::new(1);
    *model.batch_gate.lock() = Some(Arc::clone(&gate));
    let courses = indexes(
        &pool,
        &temp.path().join("course-index"),
        FixedEmbedders::new(Some(Arc::clone(&model))),
    );
    courses.register().await?;
    let (a, _) = course(&pool, &texts("alpha", BATCH_SIZE * 3)).await?;
    let (b, _) = course(&pool, &texts("beta", BATCH_SIZE)).await?;

    courses.refresh(&a).await;
    tokio::select! {
        () = gate.blocked.notified() => {}
        () = finished(&pool, &a) => panic!("{:?}", builds(&pool, &a).await),
    }
    if open_before_queueing {
        courses.open(&b).await?;
        courses.refresh(&b).await;
    } else {
        courses.refresh(&b).await;
        courses.open(&b).await?;
    }
    gate.permits.add_permits(BATCH_SIZE * 10);
    finished(&pool, &b).await;
    finished(&pool, &a).await;

    // A's two batches, all of B, then the rest of A.
    let order: Vec<char> = model
        .embedded()
        .iter()
        .map(|text| if text.contains("beta") { 'b' } else { 'a' })
        .collect();
    let mut expected = vec!['a'; BATCH_SIZE * 2];
    expected.extend(vec!['b'; BATCH_SIZE]);
    expected.extend(vec!['a'; BATCH_SIZE]);
    assert_eq!(order, expected);
    assert_eq!(courses.ensure_current_quietly(&a).await?, IndexMode::Hybrid);
    Ok(())
}

#[tokio::test]
async fn a_course_opened_with_its_build_queued_goes_first() -> Result<()> {
    opened_course_goes_first(false).await
}

#[tokio::test]
async fn a_build_queued_for_an_open_course_goes_first() -> Result<()> {
    opened_course_goes_first(true).await
}

#[tokio::test]
async fn closing_the_workspace_gives_the_turn_back_and_a_later_course_keeps_it() -> Result<()> {
    let pool = tests::pool().await?;
    let temp = tempfile::tempdir().unwrap();
    let courses = indexes(&pool, temp.path(), FixedEmbedders::new(None));
    let (a, _) = course(&pool, &texts("alpha", 1)).await?;
    let (b, _) = course(&pool, &texts("beta", 1)).await?;

    courses.open(&a).await?;
    assert!(courses.is_awaited(&a));
    // Opening another course replaces the first; closing the first then
    // leaves the second where it is.
    courses.open(&b).await?;
    assert!(!courses.is_awaited(&a));
    courses.close(&a);
    assert!(courses.is_awaited(&b));
    courses.close(&b);
    assert!(!courses.is_awaited(&b));
    assert!(courses.open("not-a-program").await.is_err());
    Ok(())
}
