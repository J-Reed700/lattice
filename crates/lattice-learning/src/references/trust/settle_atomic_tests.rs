//! A user's verdict on a claim and everything it settles (the claim's event,
//! the checker-accuracy example, the calibration outcomes) land together or
//! not at all: a failure in a later write leaves the earlier ones unwritten.

use super::calibration_tests::{
    append, below_the_bar, claim_entry, queued_below_the_bar, waiting_lesson,
};
use super::test_support::{course, Course};
use crate::lessons::claim_reviews::checker_examples::{
    CheckerExample, CheckerExamples, Label, LabelSource,
};
use crate::lessons::claim_reviews::dto::ClaimActionRequestDto;
use crate::lessons::claim_reviews::repository::ClaimReviewRepository;
use crate::lessons::claim_reviews::service::ClaimReviewService;
use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::application::ports::SourceTrustStore;
use lattice_core::domain::learning::claim_review::{
    text_sha256, Actor, Check, CheckResult, Checker, ClaimEvent, ClaimId, ClaimLocation,
    EvidenceFingerprint, EvidenceSeen, UnitKind, VerdictKind,
};
use lattice_core::shared::error::Result;

const MODEL: &str = "qwen3.5-27b";
const QUOTE: &str = "A conforming parser rejects a trailing comma.";

/// A claim the local checker flagged over a passage it compared.
async fn flagged_claim(course: &Course, lesson_id: &str) -> ClaimId {
    let claim = ClaimId(uuid::Uuid::new_v4().to_string());
    let evidence = vec![EvidenceSeen {
        source_version_id: "v-blog".into(),
        start_byte: 0,
        end_byte: QUOTE.len(),
        text_sha256: text_sha256(QUOTE),
    }];
    let events = [
        ClaimEvent::Extracted {
            location: ClaimLocation::locate(0, UnitKind::Block, "body", QUOTE, QUOTE),
            statement: QUOTE.into(),
        },
        ClaimEvent::Checked(Check {
            checker: Checker::Local {
                model: MODEL.into(),
            },
            result: CheckResult::Failed {
                kind: VerdictKind::Unsupported,
            },
            reason: "No passage says it.".into(),
            fingerprint: EvidenceFingerprint::of("lesson-evidence-v21", MODEL, &evidence),
            evidence,
            window_tokens: 32_768,
        }),
    ];
    for event in events {
        append(&course.pool, lesson_id, &claim, Actor::Machine, event).await;
    }
    claim
}

/// A claim the gate held below its bar, waiting for the user.
async fn held_claim(course: &Course, lesson_id: &str) -> ClaimId {
    queued_below_the_bar(&course.pool, lesson_id, below_the_bar("v-blog")).await
}

fn confirm() -> ClaimActionRequestDto {
    ClaimActionRequestDto::Confirm {
        note: Some("Checked by hand.".into()),
        source_version_ids: vec![],
    }
}

async fn act(
    course: &Course,
    lesson_id: &str,
    claim: &ClaimId,
    action: ClaimActionRequestDto,
) -> Result<i64> {
    let revision = ClaimReviewRepository::new(course.pool.clone())
        .load(lesson_id)
        .await?
        .revision
        .0;
    ClaimReviewService::new(course.database.clone())
        .resolve(lesson_id, &claim.0, action, revision)
        .await
}

/// Everything a verdict may write.
#[derive(Debug, PartialEq)]
struct Stored {
    events: usize,
    revision: i64,
    examples: usize,
    outcomes: usize,
}

async fn stored(course: &Course, lesson_id: &str, claim: &ClaimId) -> Result<Stored> {
    let revision = ClaimReviewRepository::new(course.pool.clone())
        .load(lesson_id)
        .await?
        .revision
        .0;
    Ok(Stored {
        events: claim_entry(&course.pool, lesson_id, claim)
            .await
            .events()
            .len(),
        revision,
        examples: CheckerExamples::new(course.database.clone())
            .list(None)
            .await?
            .len(),
        outcomes: course.store.outcomes().await?.len(),
    })
}

async fn refuse(course: &Course, statement: &str, table: &str) {
    sqlx::query(&format!(
        "CREATE TRIGGER refuse_for_test BEFORE {statement} ON {table} BEGIN SELECT RAISE(ABORT, 'refused for the test'); END"
    ))
    .execute(&course.pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn a_confirmation_writes_its_event_its_example_and_its_outcomes() -> Result<()> {
    let course = course().await?;
    let lesson_id = waiting_lesson(&course).await?;
    let flagged = flagged_claim(&course, &lesson_id).await;
    let held = held_claim(&course, &lesson_id).await;

    act(&course, &lesson_id, &flagged, confirm()).await?;
    act(&course, &lesson_id, &held, confirm()).await?;

    let stored = stored(&course, &lesson_id, &held).await?;
    assert_eq!(stored.examples, 1, "the flagged claim gave its example");
    assert_eq!(stored.outcomes, 1, "the held claim gave its outcome");
    Ok(())
}

#[tokio::test]
async fn a_confirmation_whose_outcome_cannot_be_recorded_is_not_recorded_at_all() -> Result<()> {
    let course = course().await?;
    let lesson_id = waiting_lesson(&course).await?;
    let held = held_claim(&course, &lesson_id).await;
    let before = stored(&course, &lesson_id, &held).await?;
    refuse(&course, "INSERT", "source_trust_outcomes").await;

    let result = act(&course, &lesson_id, &held, confirm()).await;

    assert!(result.is_err());
    assert_eq!(stored(&course, &lesson_id, &held).await?, before);
    Ok(())
}

#[tokio::test]
async fn a_confirmation_whose_example_cannot_be_recorded_is_not_recorded_at_all() -> Result<()> {
    let course = course().await?;
    let lesson_id = waiting_lesson(&course).await?;
    let flagged = flagged_claim(&course, &lesson_id).await;
    let before = stored(&course, &lesson_id, &flagged).await?;
    refuse(&course, "INSERT", "learning_checker_examples").await;

    let result = act(&course, &lesson_id, &flagged, confirm()).await;

    assert!(result.is_err());
    assert_eq!(stored(&course, &lesson_id, &flagged).await?, before);
    Ok(())
}

#[tokio::test]
async fn an_undo_whose_outcomes_cannot_be_forgotten_undoes_nothing() -> Result<()> {
    let course = course().await?;
    let lesson_id = waiting_lesson(&course).await?;
    let held = held_claim(&course, &lesson_id).await;
    act(&course, &lesson_id, &held, confirm()).await?;
    CheckerExamples::new(course.database.clone())
        .record(&CheckerExample {
            id: uuid::Uuid::new_v4().to_string(),
            program_id: course.program_id.clone(),
            lesson_id: lesson_id.clone(),
            claim_id: held.0.clone(),
            checker_model: MODEL.into(),
            context_window: 32_768,
            policy: "test-policy".into(),
            claim: QUOTE.into(),
            passages: vec![QUOTE.into()],
            local_verdict: "unsupported".into(),
            label: Label::True,
            label_source: LabelSource::UserConfirm,
            at: 1,
        })
        .await?;
    let before = stored(&course, &lesson_id, &held).await?;
    assert_eq!((before.examples, before.outcomes), (1, 1));
    refuse(&course, "DELETE", "source_trust_outcomes").await;

    let result = act(&course, &lesson_id, &held, ClaimActionRequestDto::Undo).await;

    assert!(result.is_err());
    assert_eq!(stored(&course, &lesson_id, &held).await?, before);
    Ok(())
}
