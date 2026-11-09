//! A course's setup is the one part of lesson preparation that touches state
//! every lesson of the course shares, so two lessons never set it up at once.
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
use super::*;
use crate::lessons::course_setup::CourseSetups;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[derive(Default)]
struct Overlap {
    active: AtomicUsize,
    most: AtomicUsize,
    runs: AtomicUsize,
}

fn worker(pool: &SqlitePool, overlap: &Arc<Overlap>) -> LessonGenerationWorker {
    let overlap = overlap.clone();
    LessonGenerationWorker {
        course_setups: Default::default(),
        runtimes: crate::embedded_runtime::BuiltinRuntimes::for_tests(),
        pool: pool.clone(),
        load_llm: Arc::new(|| Box::pin(async { Err(AppError::InvalidState("unused".into())) })),
        courses: None,
        refresh_sources: Arc::new(move |_| {
            let overlap = overlap.clone();
            Box::pin(async move {
                overlap.runs.fetch_add(1, Ordering::SeqCst);
                let now = overlap.active.fetch_add(1, Ordering::SeqCst) + 1;
                overlap.most.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(50)).await;
                overlap.active.fetch_sub(1, Ordering::SeqCst);
                Ok(())
            })
        }),
        research_web: None,
        verifier: None,
    }
}

#[tokio::test]
async fn two_setups_of_one_course_run_one_after_the_other() {
    let pool = crate::tests::pool().await.unwrap();
    let (repo, job) = repository_tests::queued_job(&pool).await;
    let overlap = Arc::new(Overlap::default());
    let worker = worker(&pool, &overlap);

    let (first, second) = tokio::join!(
        worker.set_up_course(&repo, &job.program_id),
        worker.set_up_course(&repo, &job.program_id),
    );

    first.unwrap();
    second.unwrap();
    assert_eq!(overlap.runs.load(Ordering::SeqCst), 2);
    assert_eq!(
        overlap.most.load(Ordering::SeqCst),
        1,
        "two setups of one course refreshed its sources at once"
    );
    let accepted: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM learning_curriculum_revisions WHERE program_id=? AND status='accepted'",
    )
    .bind(&job.program_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(accepted, 1);
}

#[tokio::test]
async fn setups_of_different_courses_do_not_wait_for_each_other() {
    let setups = CourseSetups::default();
    let held = setups.enter("course-a").await;
    tokio::time::timeout(Duration::from_secs(5), setups.enter("course-b"))
        .await
        .expect("another course's setup waited");
    drop(held);
}

#[tokio::test]
async fn a_course_is_forgotten_once_no_setup_holds_or_awaits_it() {
    let setups = CourseSetups::default();
    let held = setups.enter("course").await;
    let waiting = tokio::spawn({
        let setups = setups.clone();
        async move { setups.enter("course").await }
    });
    tokio::task::yield_now().await;
    drop(held);
    drop(waiting.await.unwrap());
    assert!(!setups.tracks("course"));

    let cancelled = tokio::time::timeout(Duration::from_millis(20), async {
        let _held = setups.enter("course").await;
        setups.enter("course").await
    })
    .await;
    assert!(cancelled.is_err());
    assert!(!setups.tracks("course"));
}
