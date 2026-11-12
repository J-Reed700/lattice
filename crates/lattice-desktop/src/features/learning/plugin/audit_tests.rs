//! Holding a lesson for review after an audit sends the review back to Needs
//! review and nothing else: a draft written while the hold waits for the
//! write lock is kept.

use std::time::Duration;

use lattice_core::domain::learning::claim_review::{LessonReview, LessonReviewState};
use lattice_core::shared::persistence::Database;
use lattice_learning::repository::LearningRepository;
use sqlx::SqlitePool;

use super::*;
use crate::features::learning::lessons::verifier_audit::AuditSummary;

const MODEL: &str = "qwen3.5-27b";

async fn ready_review(pool: &SqlitePool, db: &Database) -> String {
    let mut program = lattice_learning::test_support::fixture();
    program.summary.status = lattice_learning::dto::LearningProgramStatus::Active;
    LearningRepository::new(db.clone())
        .create(&program)
        .await
        .unwrap();
    let lesson_id: String = sqlx::query_scalar(
        "SELECT id FROM learning_lessons WHERE program_id = ? ORDER BY ordinal LIMIT 1",
    )
    .bind(&program.summary.id)
    .fetch_one(pool)
    .await
    .unwrap();
    for job in ["job-1", "job-2"] {
        sqlx::query("INSERT INTO jobs (id, kind, subject_id, operation_id, payload_hash, status, requested_json, progress_total, progress_message, created_at) VALUES (?, 'lesson_preparation', ?, ?, 'hash', 'pending', '{}', 1, 'Queued', 1000)")
            .bind(job)
            .bind(&lesson_id)
            .bind(format!("op-{job}"))
            .execute(pool)
            .await
            .unwrap();
    }
    ClaimReviewRepository::new(db.clone())
        .set_lesson_review(
            &lesson_id,
            LessonReview {
                program_id: program.summary.id.clone(),
                job_id: "job-1".into(),
                state: LessonReviewState::Ready,
                draft_sha256: "old-draft".into(),
                checker_model: MODEL.into(),
                stop_reason: None,
            },
        )
        .await
        .unwrap();
    lesson_id
}

#[tokio::test]
async fn a_new_draft_written_while_the_hold_waits_for_the_write_lock_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(crate::tests::sqlite::file_options(
            dir.path().join("audit.db"),
        ))
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let db = Database::new(pool.clone());
    let lesson_id = ready_review(&pool, &db).await;
    let store = ClaimReviewRepository::new(db.clone());

    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    let holding = {
        let store = store.clone();
        let lesson_id = lesson_id.clone();
        tokio::spawn(async move {
            let outcome = AuditOutcome::Completed(AuditSummary::default());
            hold_for_review(&store, &lesson_id, &outcome, None).await
        })
    };
    tokio::time::sleep(Duration::from_millis(300)).await;
    sqlx::query(
        "UPDATE learning_lesson_reviews SET draft_sha256='new-draft', job_id='job-2' WHERE lesson_id=?",
    )
    .bind(&lesson_id)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    holding.await.unwrap().unwrap();

    let review = store.load(&lesson_id).await.unwrap().review.unwrap();
    assert_eq!(review.state, LessonReviewState::NeedsReview);
    assert_eq!(review.draft_sha256, "new-draft");
    assert_eq!(review.job_id, "job-2");
}
