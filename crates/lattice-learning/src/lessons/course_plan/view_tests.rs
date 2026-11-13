//! What the learner sees of the plan: its state, and which lessons' topics
//! can still be changed.
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
use super::topics_support::*;
use super::*;
use lattice_core::shared::runtime::jobs::JobStore;
use std::sync::Arc;

#[tokio::test]
async fn the_state_goes_from_none_to_planning_to_planned() {
    let pool = crate::tests::pool().await.unwrap();
    let db = Database::new(pool.clone());
    let program = saved_program(&db).await;
    let model = Arc::new(Topics::new());

    let before = view(&db, &program.summary.id).await.unwrap();
    assert_eq!(before.state, CoursePlanStateDto::None);
    assert!(before.entries.is_empty());
    assert_eq!(before.error, None);

    let id = queue(&db, &program.summary.id).await.unwrap();
    assert!(id.is_some(), "a job is queued");
    let id = id.unwrap();
    let waiting = view(&db, &program.summary.id).await.unwrap();
    assert_eq!(waiting.state, CoursePlanStateDto::Planning);
    assert!(
        waiting.entries.is_empty(),
        "no topics are shown before they are all planned"
    );

    run_plan_job(&db, model.clone(), &id).await;

    let planned = view(&db, &program.summary.id).await.unwrap();
    assert_eq!(planned.state, CoursePlanStateDto::Planned);
    assert_eq!(planned.program_id, program.summary.id);
    assert_eq!(
        planned.outline_revision,
        outline_revision(&db, &program.summary.id).await.unwrap()
    );
    let order: Vec<_> = planned
        .entries
        .iter()
        .map(|entry| entry.topics.lesson_id.clone())
        .collect();
    assert_eq!(order, ids(&program), "entries come in course order");
    for entry in &planned.entries {
        assert_eq!(entry.entry_revision, 1);
        assert!(entry.editable);
        assert_eq!(entry.previous, None);
        assert_eq!(entry.topics.ideas, vec!["the central idea".to_owned()]);
    }
}

#[tokio::test]
async fn a_failed_plan_job_shows_failed_with_its_error() {
    let pool = crate::tests::pool().await.unwrap();
    let db = Database::new(pool.clone());
    let program = saved_program(&db).await;
    let id = queue(&db, &program.summary.id).await.unwrap();
    assert!(id.is_some(), "a job is queued");
    let id = id.unwrap();
    let store = JobStore::new(db.clone());
    assert!(store.claim(&id).await.unwrap().is_some());
    store
        .fail(&id, "model_failed", "The model gave up on the plan")
        .await
        .unwrap();

    let failed = view(&db, &program.summary.id).await.unwrap();

    assert_eq!(failed.state, CoursePlanStateDto::Failed);
    assert_eq!(
        failed.error.as_deref(),
        Some("The model gave up on the plan")
    );
    assert!(failed.entries.is_empty());
}

#[tokio::test]
async fn a_plan_for_an_earlier_outline_does_not_make_the_current_one_planned() {
    let pool = crate::tests::pool().await.unwrap();
    let db = Database::new(pool.clone());
    let (program, _) = planned_program(&db, &Topics::new()).await;
    let before = view(&db, &program.summary.id).await.unwrap();
    assert_eq!(before.state, CoursePlanStateDto::Planned);

    change_outline(&pool, &program.summary.id).await;

    let after = view(&db, &program.summary.id).await.unwrap();
    assert_eq!(after.state, CoursePlanStateDto::None);
    assert!(after.entries.is_empty());
}

#[tokio::test]
async fn a_queued_lesson_stays_editable_and_a_running_one_is_locked() {
    let pool = crate::tests::pool().await.unwrap();
    let db = Database::new(pool.clone());
    let (program, _) = planned_program(&db, &Topics::new()).await;
    let order = ids(&program);

    let job = lesson_job(&db, &program, &order[1], false).await;
    let pending = view(&db, &program.summary.id).await.unwrap();
    let queued: Vec<_> = pending.entries.iter().map(|entry| entry.editable).collect();
    assert_eq!(
        queued,
        vec![true, true, true, true],
        "a queued job leaves its lesson editable"
    );

    assert!(JobStore::new(db.clone())
        .claim(&job)
        .await
        .unwrap()
        .is_some());
    let running = view(&db, &program.summary.id).await.unwrap();
    let editable: Vec<_> = running.entries.iter().map(|entry| entry.editable).collect();
    assert_eq!(editable, vec![true, false, true, true]);
}

#[tokio::test]
async fn a_ready_lesson_and_a_completed_lesson_are_locked() {
    let pool = crate::tests::pool().await.unwrap();
    let db = Database::new(pool.clone());
    let (program, _) = planned_program(&db, &Topics::new()).await;
    let order = ids(&program);
    sqlx::query("UPDATE learning_lessons SET preparation='ready' WHERE id=?")
        .bind(&order[0])
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE learning_lessons SET completed=1 WHERE id=?")
        .bind(&order[2])
        .execute(&pool)
        .await
        .unwrap();

    let shown = view(&db, &program.summary.id).await.unwrap();

    let editable: Vec<_> = shown.entries.iter().map(|entry| entry.editable).collect();
    assert_eq!(editable, vec![false, true, false, true]);
}
