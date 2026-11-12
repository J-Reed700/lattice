//! A code example's claim saved before its run was recorded is read back as
//! that run's claim.

use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::domain::learning::claim_review::{
    Actor, ClaimEvent, ClaimId, ClaimLocation, Class, ClassifiedBy, EventProvenance, LessonReview,
    LessonReviewState, UnitKind,
};
use sqlx::sqlite::SqlitePoolOptions;

use super::repository::ClaimReviewRepository;
use crate::dto::LearningProgramStatus;
use crate::repository::LearningRepository;

const STATEMENT: &str = "The code example run-1 runs to completion with the output it shows.";

fn provenance() -> EventProvenance {
    EventProvenance {
        actor: Actor::Machine,
        at: 1_760_000_000_000,
        job_id: None,
        app_version: "1.0.0".into(),
        checker_model: None,
    }
}

#[tokio::test]
async fn a_code_example_claim_saved_before_its_run_was_recorded_reads_back_as_that_runs_claim() {
    let file = tempfile::tempdir().unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(3)
        .connect_with(crate::test_support::file_options(
            file.path().join("example.db"),
        ))
        .await
        .unwrap();
    sqlx::migrate!("../lattice-desktop/migrations")
        .run(&pool)
        .await
        .unwrap();
    let mut program = crate::test_support::fixture();
    program.summary.status = LearningProgramStatus::Active;
    LearningRepository::new(pool.clone())
        .create(&program)
        .await
        .unwrap();
    let program_id = program.summary.id.clone();
    let lesson_id: String = sqlx::query_scalar(
        "SELECT id FROM learning_lessons WHERE program_id = ? ORDER BY ordinal LIMIT 1",
    )
    .bind(&program_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO jobs (id, kind, subject_id, operation_id, payload_hash, status, requested_json, progress_total, progress_message, created_at) VALUES ('job-1', 'lesson_preparation', ?, 'op-1', 'hash', 'pending', '{}', 1, 'Queued', 1000)")
        .bind(&lesson_id)
        .execute(&pool)
        .await
        .unwrap();
    let store = ClaimReviewRepository::new(pool.clone());
    store
        .set_lesson_review(
            &lesson_id,
            LessonReview {
                program_id,
                job_id: "job-1".into(),
                state: LessonReviewState::Ready,
                draft_sha256: "draft".into(),
                checker_model: "qwen3.5-27b".into(),
                stop_reason: None,
            },
        )
        .await
        .unwrap();

    let unclassified = ClaimId("claim-example".into());
    let classified = ClaimId("claim-statement".into());
    for (claim, classify) in [(&unclassified, false), (&classified, true)] {
        let extracted = ClaimEvent::Extracted {
            location: ClaimLocation::locate(0, UnitKind::Block, "body", STATEMENT, STATEMENT),
            statement: STATEMENT.into(),
        };
        let mut events = vec![extracted];
        if classify {
            events.push(ClaimEvent::Classified {
                class: Class::Sourced,
                by: ClassifiedBy::Model,
            });
        }
        for event in events {
            let revision = store.load(&lesson_id).await.unwrap().revision;
            store
                .append(&lesson_id, claim, provenance(), event, revision)
                .await
                .unwrap();
        }
    }

    sqlx::raw_sql(include_str!(
        "../../../../lattice-desktop/migrations/20261112000000_code_example_claims.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();

    let ledger = store.load(&lesson_id).await.unwrap();
    let example_of = |id: &ClaimId| {
        ledger
            .claims
            .iter()
            .find(|entry| entry.claim_id() == id)
            .map(|entry| entry.code_example().map(str::to_owned))
    };
    assert_eq!(example_of(&unclassified), Some(Some("run-1".to_owned())));
    assert_eq!(example_of(&classified), Some(None));
}
