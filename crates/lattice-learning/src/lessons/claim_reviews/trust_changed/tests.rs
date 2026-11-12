//! A published lesson is held for review in one write unit: a write that
//! lands while the hold waits for the write lock is neither undone nor
//! decided over.

use std::time::Duration;

use lattice_core::domain::learning::claim_review::{
    ClaimId, ClaimLocation, EvidenceFingerprint, HumanBinding, UnitKind, UserAction, VerdictKind,
};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

use super::super::repository::{append_in, bump_revision};
use super::*;
use crate::dto::LearningProgramStatus;
use crate::repository::LearningRepository;

const MODEL: &str = "qwen3.5-27b";
const VERIFIER: &str = "claude-haiku-5-5";
const QUOTE: &str = "A conforming parser rejects a trailing comma.";

fn provenance(actor: Actor) -> EventProvenance {
    EventProvenance {
        actor,
        at: 1_760_000_000_000,
        job_id: None,
        app_version: "1.0.0".into(),
        checker_model: Some(MODEL.into()),
    }
}

struct Published {
    db: Database,
    pool: SqlitePool,
    store: ClaimReviewRepository,
    lesson_id: String,
    claim: ClaimId,
    _file: tempfile::TempDir,
}

impl Published {
    fn hold(&self) -> tokio::task::JoinHandle<Result<()>> {
        let database = self.db.clone();
        let lesson_id = self.lesson_id.clone();
        tokio::spawn(async move { hold_if_published(&database, &lesson_id).await })
    }

    async fn review(&self) -> LessonReview {
        self.store
            .load(&self.lesson_id)
            .await
            .unwrap()
            .review
            .unwrap()
    }
}

/// A Ready review (draft "old-draft", job "job-1") whose one claim the
/// local checker supported and the Verifier disputed, so it waits for the
/// user and the ledger's end state is Needs review.
async fn published() -> Published {
    let file = tempfile::tempdir().unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(3)
        .connect_with(crate::test_support::file_options(
            file.path().join("held.db"),
        ))
        .await
        .unwrap();
    sqlx::migrate!("../lattice-desktop/migrations")
        .run(&pool)
        .await
        .unwrap();
    let db = Database::new(pool.clone());
    let mut program = crate::test_support::fixture();
    program.summary.status = LearningProgramStatus::Active;
    LearningRepository::new(db.clone())
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
    for job in ["job-1", "job-2"] {
        sqlx::query("INSERT INTO jobs (id, kind, subject_id, operation_id, payload_hash, status, requested_json, progress_total, progress_message, created_at) VALUES (?, 'lesson_preparation', ?, ?, 'hash', 'pending', '{}', 1, 'Queued', 1000)")
            .bind(job)
            .bind(&lesson_id)
            .bind(format!("op-{job}"))
            .execute(&pool)
            .await
            .unwrap();
    }
    let store = ClaimReviewRepository::new(db.clone());
    store
        .set_lesson_review(
            &lesson_id,
            LessonReview {
                program_id,
                job_id: "job-1".into(),
                state: LessonReviewState::Ready,
                draft_sha256: "old-draft".into(),
                checker_model: MODEL.into(),
                stop_reason: None,
            },
        )
        .await
        .unwrap();
    let claim = ClaimId("claim-1".into());
    let extracted = ClaimEvent::Extracted {
        location: ClaimLocation::locate(0, UnitKind::Block, "body", QUOTE, QUOTE),
        statement: QUOTE.into(),
    };
    let supported = ClaimEvent::Checked(Check {
        checker: Checker::Local {
            model: MODEL.into(),
        },
        result: CheckResult::Supported {
            source_version_id: "v1".into(),
            supporting_quote: QUOTE.into(),
        },
        reason: "The source states it.".into(),
        fingerprint: EvidenceFingerprint::of("lesson-evidence-v21", MODEL, &[]),
        evidence: vec![],
        window_tokens: 32_768,
    });
    let disputed = ClaimEvent::Checked(Check {
        checker: Checker::Verifier {
            model: VERIFIER.into(),
        },
        result: CheckResult::Failed {
            kind: VerdictKind::Unsupported,
        },
        reason: "Nothing supports it.".into(),
        fingerprint: EvidenceFingerprint::of("lesson-evidence-v21", VERIFIER, &[]),
        evidence: vec![],
        window_tokens: 32_768,
    });
    for event in [extracted, supported, disputed] {
        let revision = store.load(&lesson_id).await.unwrap().revision;
        store
            .append(
                &lesson_id,
                &claim,
                provenance(Actor::Machine),
                event,
                revision,
            )
            .await
            .unwrap();
    }
    let ledger = store.load(&lesson_id).await.unwrap();
    assert_eq!(end_state(&ledger.claims), LessonReviewState::NeedsReview);
    Published {
        db,
        pool,
        store,
        lesson_id,
        claim,
        _file: file,
    }
}

#[tokio::test]
async fn a_new_draft_written_while_the_hold_waits_for_the_write_lock_is_kept() {
    let fixture = published().await;

    let mut tx = fixture.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    let holding = fixture.hold();
    tokio::time::sleep(Duration::from_millis(300)).await;
    sqlx::query(
        "UPDATE learning_lesson_reviews SET draft_sha256='new-draft', job_id='job-2' WHERE lesson_id=?",
    )
    .bind(&fixture.lesson_id)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    holding.await.unwrap().unwrap();

    let review = fixture.review().await;
    assert_eq!(review.state, LessonReviewState::NeedsReview);
    assert_eq!(review.draft_sha256, "new-draft");
    assert_eq!(review.job_id, "job-2");
}

#[tokio::test]
async fn a_claim_fixed_while_the_hold_waits_for_the_write_lock_keeps_the_lesson_ready() {
    let fixture = published().await;

    let mut tx = fixture.db.begin_write().await.unwrap();
    let holding = fixture.hold();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let confirm = ClaimEvent::UserAction {
        action: UserAction::Confirm {
            bound: HumanBinding::new(QUOTE, vec!["v1".to_owned()]),
        },
        note: None,
    };
    append_in(
        &mut tx,
        &fixture.lesson_id,
        &fixture.claim,
        provenance(Actor::User),
        confirm,
    )
    .await
    .unwrap();
    bump_revision(&mut tx, &fixture.lesson_id).await.unwrap();
    tx.commit().await.unwrap();
    holding.await.unwrap().unwrap();

    let ledger = fixture.store.load(&fixture.lesson_id).await.unwrap();
    assert_eq!(end_state(&ledger.claims), LessonReviewState::Ready);
    assert_eq!(ledger.review.unwrap().state, LessonReviewState::Ready);
}
