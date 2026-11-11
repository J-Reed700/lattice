//! The job that plans a course's topics as soon as its outline exists.
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
use super::topics_support::*;
use super::*;
use crate::repository::LearningRepository;
use lattice_core::shared::runtime::jobs::{JobStatus, JobStore};
use sqlx::SqlitePool;
use std::sync::Arc;

async fn plan_jobs(pool: &SqlitePool, program_id: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind=? AND subject_id=?")
        .bind(COURSE_PLAN)
        .bind(program_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn planning_twice_admits_one_job() {
    let pool = crate::tests::pool().await.unwrap();
    let program = saved_program(&pool).await;

    let first = queue(&Database::from(pool.clone()), &program.summary.id)
        .await
        .unwrap();
    let second = queue(&Database::from(pool.clone()), &program.summary.id)
        .await
        .unwrap();

    assert!(first.is_some(), "a course with no plan is queued");
    assert_eq!(first, second);
    assert_eq!(plan_jobs(&pool, &program.summary.id).await, 1);
}

#[tokio::test]
async fn a_job_carries_the_course_and_the_outline_revision_it_plans() {
    let pool = crate::tests::pool().await.unwrap();
    let program = saved_program(&pool).await;
    let revision = outline_revision(&Database::from(pool.clone()), &program.summary.id)
        .await
        .unwrap();

    let id = queue(&Database::from(pool.clone()), &program.summary.id)
        .await
        .unwrap();

    assert!(id.is_some(), "a job is queued");

    let id = id.unwrap();
    let job = JobStore::new(pool.clone()).get(&id).await.unwrap();
    assert_eq!(job.kind, COURSE_PLAN);
    assert_eq!(job.requested["programId"], program.summary.id.as_str());
    assert_eq!(job.requested["outlineRevision"], revision);
    assert_eq!(job.status, JobStatus::Pending);
}

#[tokio::test]
async fn a_completed_plan_is_a_no_op() {
    let pool = crate::tests::pool().await.unwrap();
    let (program, _) = planned_program(&pool, &Topics::new()).await;

    let queued = queue(&Database::from(pool.clone()), &program.summary.id)
        .await
        .unwrap();

    assert_eq!(queued, None);
    assert_eq!(plan_jobs(&pool, &program.summary.id).await, 0);
    let shown = view(&Database::from(pool.clone()), &program.summary.id)
        .await
        .unwrap();
    assert_eq!(shown.state, CoursePlanStateDto::Planned);
}

#[tokio::test]
async fn a_draft_course_can_be_planned() {
    let pool = crate::tests::pool().await.unwrap();
    let program = saved_program(&pool).await;
    let model = Arc::new(Topics::new());
    let id = queue(&Database::from(pool.clone()), &program.summary.id)
        .await
        .unwrap();
    assert!(id.is_some(), "a draft course is queued");
    let id = id.unwrap();

    run_plan_job(&pool, model.clone(), &id).await;

    assert_eq!(model.calls(), 1);
    let job = JobStore::new(pool.clone()).get(&id).await.unwrap();
    assert_eq!(job.status, JobStatus::Completed);
    let planned = view(&Database::from(pool.clone()), &program.summary.id)
        .await
        .unwrap();
    assert_eq!(planned.state, CoursePlanStateDto::Planned);
    assert_eq!(planned.entries.len(), 4);
}

#[tokio::test]
async fn an_active_course_can_be_planned() {
    let pool = crate::tests::pool().await.unwrap();
    let program = saved_program(&pool).await;
    LearningRepository::new(pool.clone())
        .accept(&crate::dto::AcceptLearningProgramRequestDto {
            program_id: program.summary.id.clone(),
            expected_revision: 0,
            title: program.summary.title.clone(),
        })
        .await
        .unwrap();

    let id = queue(&Database::from(pool.clone()), &program.summary.id)
        .await
        .unwrap();

    assert!(id.is_some());
}

#[tokio::test]
async fn an_unknown_course_is_refused() {
    let pool = crate::tests::pool().await.unwrap();

    let refused = queue(&Database::from(pool.clone()), "no-such-program").await;

    assert!(matches!(refused, Err(AppError::NotFound(_))), "{refused:?}");
}

#[tokio::test]
async fn a_stale_revision_completes_without_a_call() {
    let pool = crate::tests::pool().await.unwrap();
    let program = saved_program(&pool).await;
    let model = Arc::new(Topics::new());
    let id = queue(&Database::from(pool.clone()), &program.summary.id)
        .await
        .unwrap();
    assert!(id.is_some(), "a job is queued");
    let id = id.unwrap();
    change_outline(&pool, &program.summary.id).await;

    run_plan_job(&pool, model.clone(), &id).await;

    assert_eq!(
        model.calls(),
        0,
        "a plan for an outline that is gone is not made"
    );
    let job = JobStore::new(pool.clone()).get(&id).await.unwrap();
    assert_eq!(job.status, JobStatus::Completed);
    let saved: i64 =
        sqlx::query_scalar("SELECT count(*) FROM learning_course_plan_lessons WHERE program_id=?")
            .bind(&program.summary.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(saved, 0);
}

#[tokio::test]
async fn the_job_plans_the_revision_it_was_queued_for_and_a_new_one_after_an_outline_change() {
    let pool = crate::tests::pool().await.unwrap();
    let program = saved_program(&pool).await;
    let first = queue(&Database::from(pool.clone()), &program.summary.id)
        .await
        .unwrap();
    change_outline(&pool, &program.summary.id).await;

    let second = queue(&Database::from(pool.clone()), &program.summary.id)
        .await
        .unwrap();

    assert!(first.is_some());
    assert!(second.is_some());
    assert_ne!(
        first, second,
        "the new outline is planned by a job of its own"
    );
}

#[tokio::test]
async fn a_failed_job_is_tried_again_when_the_course_is_planned_again() {
    let pool = crate::tests::pool().await.unwrap();
    let program = saved_program(&pool).await;
    let failed = queue(&Database::from(pool.clone()), &program.summary.id)
        .await
        .unwrap();
    assert!(failed.is_some(), "a job is queued");
    let failed = failed.unwrap();
    let store = JobStore::new(pool.clone());
    assert!(store.claim(&failed).await.unwrap().is_some());
    store
        .fail(&failed, "model_failed", "The model gave up")
        .await
        .unwrap();

    let again = queue(&Database::from(pool.clone()), &program.summary.id)
        .await
        .unwrap();

    assert!(again.is_some(), "a new attempt is queued");

    let again = again.unwrap();
    assert_ne!(again, failed);
    assert_eq!(store.get(&again).await.unwrap().status, JobStatus::Pending);
    let twice = queue(&Database::from(pool.clone()), &program.summary.id)
        .await
        .unwrap();
    assert_eq!(
        twice,
        Some(again),
        "asking again meanwhile returns the live attempt"
    );
}
