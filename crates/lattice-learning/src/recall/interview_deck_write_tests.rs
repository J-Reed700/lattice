//! Making an interview deck is one write: the deck and its target land together,
//! and two calls at once make one deck.
use super::{
    study_dto::CreateInterviewStudyDeckRequestDto, study_repository::StudyRepository,
    study_service::create_interview_deck,
};
use crate::practice::practical_generation::INTERVIEW_PLAN_FILE_PATH;
use lattice_core::shared::persistence::Database;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

const PLAN: &str = r#"{
    "companyName": "Acme Corp",
    "targetRole": "Backend Engineer",
    "roundKind": "full_loop",
    "companySummary": "Streaming pipelines.",
    "learnerRole": "Candidate",
    "counterpartRole": "Interviewer",
    "questions": [
        {
            "id": "q-backpressure",
            "round": "technical_deep_dive",
            "question": "How do you handle backpressure in event streams?",
            "companyRationale": "Acme runs streaming pipelines.",
            "followUpProbes": [],
            "evaluationSignals": ["Bounds every queue"]
        },
        {
            "id": "q-conflict",
            "round": "behavioral",
            "question": "Tell me about a disagreement with a teammate.",
            "companyRationale": "Teams own services end to end.",
            "followUpProbes": [],
            "evaluationSignals": ["Specific situation"]
        }
    ]
}"#;

async fn open() -> (tempfile::TempDir, SqlitePool, Database) {
    let dir = tempfile::tempdir().unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(
            crate::test_support::file_options(dir.path().join("t.db")).foreign_keys(false),
        )
        .await
        .unwrap();
    let db = Database::new(pool.clone());
    sqlx::migrate!("../lattice-desktop/migrations")
        .run(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO learning_practical_activities(id,program_id,lesson_id,kind,title,brief,status,practice_mode,allowed_aids_json,outcome_ids_json,source_version_ids_json,rubric_json,runtime_kind,generator_model,created_at,updated_at) VALUES('ready','program','lesson','interview','Acme loop','','ready','practice','[]','[]','[]','[]','none','model',0,0)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO learning_practical_files(activity_id,ordinal,path,role,content,content_sha256,editable) VALUES('ready',0,?,'reference',?,'sha',0)")
        .bind(INTERVIEW_PLAN_FILE_PATH)
        .bind(PLAN)
        .execute(&pool)
        .await
        .unwrap();
    (dir, pool, db)
}

fn request(target_at: Option<i64>) -> CreateInterviewStudyDeckRequestDto {
    CreateInterviewStudyDeckRequestDto {
        program_id: "program".into(),
        activity_id: "ready".into(),
        target_at,
    }
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

fn tomorrow() -> i64 {
    chrono::Utc::now().timestamp_millis() + super::schedule::DAY_MS
}

#[tokio::test]
async fn a_target_that_cannot_be_saved_leaves_no_deck_behind() {
    let (_dir, pool, db) = open().await;
    sqlx::query("CREATE TRIGGER refuse_target BEFORE INSERT ON study_deck_targets BEGIN SELECT RAISE(ABORT, 'injected'); END")
        .execute(&pool)
        .await
        .unwrap();
    let repo = StudyRepository::new(db.clone());
    let result = create_interview_deck(&repo, &db, request(Some(tomorrow()))).await;
    assert!(result.is_err());
    assert_eq!(count(&pool, "study_decks").await, 0);
    assert_eq!(count(&pool, "study_cards").await, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn making_the_same_deck_at_once_makes_one_deck() {
    let (_dir, pool, db) = open().await;
    let target = tomorrow();
    let mut calls = tokio::task::JoinSet::new();
    for _ in 0..6 {
        let db = db.clone();
        calls.spawn(async move {
            let repo = StudyRepository::new(db.clone());
            create_interview_deck(&repo, &db, request(Some(target))).await
        });
    }
    while let Some(done) = calls.join_next().await {
        let deck = done.unwrap().unwrap();
        assert_eq!(deck.cards.len(), 2);
        assert_eq!(deck.target_at, Some(target));
    }
    assert_eq!(count(&pool, "study_decks").await, 1);
    assert_eq!(count(&pool, "study_cards").await, 2);
    assert_eq!(count(&pool, "study_deck_targets").await, 1);
}
