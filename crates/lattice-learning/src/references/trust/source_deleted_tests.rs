//! Deleting a source reopens the statements that rest on it, in every lesson
//! review of the course, and a review read finds what an earlier deletion
//! left. No model is called: the reopens are the ledger's own rule.

use super::calibration_tests::{append, claim_entry, waiting_lesson};
use super::test_support::{add_page, article, course, id, page_signals, Course, Saved};
use crate::lesson_evidence::LearningClaimResolutionDto;
use crate::lessons::claim_reviews::dto::ClaimActionRequestDto;
use crate::lessons::claim_reviews::repository::ClaimReviewRepository;
use crate::lessons::claim_reviews::service::ClaimReviewService;
use crate::portability_dto::DeleteLearningSourceRequestDto;
use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::domain::learning::claim_review::{
    Actor, Check, CheckResult, Checker, ClaimEvent, ClaimId, ClaimLocation, ClaimState,
    EvidenceFingerprint, HumanBinding, LessonReviewState, ReopenCause, UnitKind, UserAction,
    VerdictKind,
};
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::Database;
use serde_json::json;

const MODEL: &str = "qwen3.5-27b";
const QUOTE: &str = "A conforming parser rejects a trailing comma.";
const SECOND: &str = "A conforming parser accepts a leading plus sign.";

async fn source(course: &Course, seed: &str) -> Result<Saved> {
    let url = format!("https://{seed}.example.org/page");
    add_page(
        course,
        &url,
        &article(seed, 8),
        page_signals(&url, json!({})),
    )
    .await
}

async fn delete(course: &Course, saved: &Saved) -> Result<()> {
    course
        .library
        .delete_source(&DeleteLearningSourceRequestDto {
            operation_id: id(),
            program_id: course.program_id.clone(),
            source_id: saved.source_id.clone(),
            expected_revision: 0,
            reason: "Removed for the test".into(),
        })
        .await
}

async fn delete_in_the_library_row(course: &Course, saved: &Saved) {
    sqlx::query("UPDATE learning_source_library SET deleted_at=1 WHERE program_id=? AND id=?")
        .bind(&course.program_id)
        .bind(&saved.source_id)
        .execute(&course.pool)
        .await
        .unwrap();
}

async fn extracted(database: &Database, lesson_id: &str, unit: usize, quote: &str) -> ClaimId {
    let claim = ClaimId(id());
    let event = ClaimEvent::Extracted {
        location: ClaimLocation::locate(unit, UnitKind::Block, "body", quote, quote),
        statement: quote.into(),
    };
    append(database, lesson_id, &claim, Actor::Machine, event).await;
    claim
}

fn local_check(result: CheckResult) -> ClaimEvent {
    ClaimEvent::Checked(Check {
        checker: Checker::Local {
            model: MODEL.into(),
        },
        result,
        reason: "Checked.".into(),
        fingerprint: EvidenceFingerprint::of("lesson-evidence-v21", MODEL, &[]),
        evidence: vec![],
        window_tokens: 32_768,
    })
}

/// A claim a check resolved by a passage of `version`.
async fn resolved_by(database: &Database, lesson_id: &str, version: &str) -> ClaimId {
    resolved_at(database, lesson_id, (0, QUOTE), version).await
}

/// A claim on `place` (unit and quote) a check resolved by `version`.
async fn resolved_at(
    database: &Database,
    lesson_id: &str,
    place: (usize, &str),
    version: &str,
) -> ClaimId {
    let claim = extracted(database, lesson_id, place.0, place.1).await;
    let result = CheckResult::Supported {
        source_version_id: version.into(),
        supporting_quote: place.1.into(),
    };
    append(
        database,
        lesson_id,
        &claim,
        Actor::Machine,
        local_check(result),
    )
    .await;
    claim
}

/// A claim the user confirmed, binding the confirmation to `version`.
async fn confirmed_on(database: &Database, lesson_id: &str, version: &str) -> ClaimId {
    let claim = extracted(database, lesson_id, 0, QUOTE).await;
    let failed = local_check(CheckResult::Failed {
        kind: VerdictKind::Unsupported,
    });
    append(database, lesson_id, &claim, Actor::Machine, failed).await;
    let confirm = ClaimEvent::UserAction {
        action: UserAction::Confirm {
            bound: HumanBinding::new(QUOTE, vec![version.to_owned()]),
        },
        note: None,
    };
    append(database, lesson_id, &claim, Actor::User, confirm).await;
    claim
}

async fn cause_of(database: &Database, lesson_id: &str, claim: &ClaimId) -> Option<ReopenCause> {
    match claim_entry(database, lesson_id, claim).await.state() {
        ClaimState::Reopened { cause, .. } => Some(cause.clone()),
        _ => None,
    }
}

async fn events_written(database: &Database, lesson_id: &str) -> usize {
    ClaimReviewRepository::new(database.clone())
        .load(lesson_id)
        .await
        .unwrap()
        .claims
        .iter()
        .map(|entry| entry.events().len())
        .sum()
}

async fn revision(database: &Database, lesson_id: &str) -> i64 {
    ClaimReviewRepository::new(database.clone())
        .load(lesson_id)
        .await
        .unwrap()
        .revision
        .0
}

#[tokio::test]
async fn deleting_a_source_reopens_the_statements_that_rest_on_it() -> Result<()> {
    let course = course().await?;
    let lesson_id = waiting_lesson(&course).await?;
    let doomed = source(&course, "doomed").await?;
    let other = source(&course, "other").await?;
    let resolved = resolved_by(&course.database, &lesson_id, &doomed.version_id).await;
    let confirmed = confirmed_on(&course.database, &lesson_id, &doomed.version_id).await;
    let elsewhere = confirmed_on(&course.database, &lesson_id, &other.version_id).await;
    let before = revision(&course.database, &lesson_id).await;

    delete(&course, &doomed).await?;

    let cause = ReopenCause::SourceDeleted(doomed.version_id.clone());
    assert_eq!(
        cause_of(&course.database, &lesson_id, &resolved).await,
        Some(cause.clone()),
        "a claim resolved by the deleted source's version reopens"
    );
    assert_eq!(
        cause_of(&course.database, &lesson_id, &confirmed).await,
        Some(cause),
        "a confirmation bound to the deleted source's version reopens"
    );
    assert!(
        matches!(
            claim_entry(&course.database, &lesson_id, &elsewhere)
                .await
                .state(),
            ClaimState::Confirmed { .. }
        ),
        "a confirmation bound to another source is untouched"
    );
    assert!(
        revision(&course.database, &lesson_id).await > before,
        "the review's revision advanced"
    );
    Ok(())
}

#[tokio::test]
async fn deleting_a_source_holds_a_ready_lesson_for_review() -> Result<()> {
    let course = course().await?;
    let lesson_id = waiting_lesson(&course).await?;
    let doomed = source(&course, "doomed").await?;
    resolved_by(&course.database, &lesson_id, &doomed.version_id).await;
    let store = ClaimReviewRepository::new(course.database.clone());
    let mut review = store.load(&lesson_id).await?.review.unwrap();
    review.state = LessonReviewState::Ready;
    store.set_lesson_review(&lesson_id, review).await?;

    delete(&course, &doomed).await?;

    let held = store.load(&lesson_id).await?.review.unwrap();
    assert_eq!(held.state, LessonReviewState::NeedsReview);
    Ok(())
}

#[tokio::test]
async fn reading_the_review_reopens_what_an_earlier_deletion_left_and_only_once() -> Result<()> {
    let course = course().await?;
    let lesson_id = waiting_lesson(&course).await?;
    let doomed = source(&course, "doomed").await?;
    let other = source(&course, "other").await?;
    let resolved = resolved_by(&course.database, &lesson_id, &doomed.version_id).await;
    let confirmed = confirmed_on(&course.database, &lesson_id, &doomed.version_id).await;
    let elsewhere = confirmed_on(&course.database, &lesson_id, &other.version_id).await;
    delete_in_the_library_row(&course, &doomed).await;
    let service = ClaimReviewService::new(course.database.clone());
    let before = revision(&course.database, &lesson_id).await;

    service.review(&course.program_id, &lesson_id).await?;

    let cause = ReopenCause::SourceDeleted(doomed.version_id.clone());
    assert_eq!(
        cause_of(&course.database, &lesson_id, &resolved).await,
        Some(cause.clone())
    );
    assert_eq!(
        cause_of(&course.database, &lesson_id, &confirmed).await,
        Some(cause)
    );
    assert_eq!(
        cause_of(&course.database, &lesson_id, &elsewhere).await,
        None
    );
    let after = revision(&course.database, &lesson_id).await;
    assert!(after > before, "the review's revision advanced");

    let written = events_written(&course.database, &lesson_id).await;
    service.review(&course.program_id, &lesson_id).await?;
    assert_eq!(events_written(&course.database, &lesson_id).await, written);
    assert_eq!(revision(&course.database, &lesson_id).await, after);
    Ok(())
}

async fn block(course: &Course, lesson_id: &str, ordinal: i64, body: &str) {
    sqlx::query("INSERT INTO learning_blocks(lesson_id,ordinal,kind,title,body,source_ids_json) VALUES(?,?,'explanation','Parsing',?,'[]')")
        .bind(lesson_id)
        .bind(ordinal)
        .bind(body)
        .execute(&course.pool)
        .await
        .unwrap();
}

fn passage(version: &str, text: &str) -> serde_json::Value {
    json!({
        "source_id": version, "text": text, "start_byte": 0, "end_byte": text.len(),
        "retrieval_kind": "lexical", "score": 1.0
    })
}

fn finding(
    unit: usize,
    quote: &str,
    claim: &ClaimId,
    evidence: Vec<serde_json::Value>,
) -> serde_json::Value {
    json!({
        "unit": unit, "quote": quote, "statement": quote, "verdict": "supported",
        "reason": "Checked.", "supporting_quote": quote, "claim_id": claim.0, "evidence": evidence
    })
}

/// A lesson with a statement resolved by a source that is then deleted, and
/// one a live source resolved whose finding also compared a passage of the
/// deleted one; the review is read, then the evidence report.
struct Lesson {
    course: Course,
    lesson_id: String,
    doomed: Saved,
    lost: ClaimId,
    kept: ClaimId,
    view: crate::lesson_evidence::LearningLessonEvidenceDto,
}

async fn deleted_source_lesson() -> Result<Lesson> {
    let course = course().await?;
    let lesson_id = waiting_lesson(&course).await?;
    block(&course, &lesson_id, 0, QUOTE).await;
    block(&course, &lesson_id, 1, SECOND).await;
    let doomed = source(&course, "doomed").await?;
    let live = source(&course, "live").await?;
    let lost = resolved_at(&course.database, &lesson_id, (0, QUOTE), &doomed.version_id).await;
    let kept = resolved_at(&course.database, &lesson_id, (1, SECOND), &live.version_id).await;
    let report = json!({
        "policy": "lesson-evidence-v21", "lesson_id": lesson_id, "content_sha256": "fixture",
        "checked_at": 1, "checker_model": MODEL, "sources": [],
        "findings": [
            finding(0, QUOTE, &lost, vec![passage(&doomed.version_id, QUOTE)]),
            finding(1, SECOND, &kept, vec![
                passage(&live.version_id, SECOND),
                passage(&doomed.version_id, SECOND),
            ]),
        ],
        "coverage": [], "coverage_audit": [], "executions": [], "issues": [],
        "retrieval_mode": "hybrid", "embedding_model": null, "unexecuted_languages": []
    });
    sqlx::query("INSERT INTO learning_lesson_verifications(lesson_id,program_id,policy,content_sha256,report_json,checked_at) VALUES(?,?,?,?,?,?)")
        .bind(&lesson_id)
        .bind(&course.program_id)
        .bind("lesson-evidence-v21")
        .bind("fixture")
        .bind(report.to_string())
        .bind(1_i64)
        .execute(&course.pool)
        .await
        .unwrap();
    delete_in_the_library_row(&course, &doomed).await;

    ClaimReviewService::new(course.database.clone())
        .review(&course.program_id, &lesson_id)
        .await?;
    let view = crate::lesson_evidence::get(&course.database, &course.program_id, &lesson_id)
        .await?
        .unwrap();

    Ok(Lesson {
        course,
        lesson_id,
        doomed,
        lost,
        kept,
        view,
    })
}

#[tokio::test]
async fn the_report_marks_source_removed_exactly_the_statements_the_review_reopened_for_it(
) -> Result<()> {
    let Lesson {
        course,
        lesson_id,
        doomed,
        lost,
        view,
        ..
    } = deleted_source_lesson().await?;
    let mut removed: Vec<String> = view
        .teaching_claims
        .iter()
        .filter(|shown| shown.resolution == LearningClaimResolutionDto::SourceRemoved)
        .filter_map(|shown| shown.claim_id.clone())
        .collect();
    removed.sort();
    assert_eq!(removed, vec![lost.0.clone()]);
    assert_eq!(
        cause_of(&course.database, &lesson_id, &lost).await,
        Some(ReopenCause::SourceDeleted(doomed.version_id.clone()))
    );
    Ok(())
}

#[tokio::test]
async fn a_statement_a_live_source_resolved_stays_verified_beside_a_deleted_passage() -> Result<()>
{
    let Lesson {
        course,
        lesson_id,
        kept,
        view,
        ..
    } = deleted_source_lesson().await?;
    let shown = view
        .teaching_claims
        .iter()
        .find(|shown| shown.claim_id.as_deref() == Some(kept.0.as_str()))
        .map(|shown| shown.resolution);
    assert_eq!(shown, Some(LearningClaimResolutionDto::Verified));
    assert_eq!(cause_of(&course.database, &lesson_id, &kept).await, None);
    Ok(())
}

async fn bound_versions(
    database: &Database,
    lesson_id: &str,
    claim: &ClaimId,
) -> Option<Vec<String>> {
    match claim_entry(database, lesson_id, claim).await.state() {
        ClaimState::Confirmed { bound, .. } => Some(bound.source_version_ids.clone()),
        _ => None,
    }
}

/// Deletes the source a claim was resolved by, then confirms the claim as the
/// review's Confirm does, naming `versions`.
async fn confirm_after_deletion(
    versions: impl FnOnce(&Saved, &Saved) -> Vec<String>,
) -> Result<(Course, String, Saved, Saved, ClaimId)> {
    let course = course().await?;
    let lesson_id = waiting_lesson(&course).await?;
    let doomed = source(&course, "doomed").await?;
    let live = source(&course, "live").await?;
    let claim = resolved_by(&course.database, &lesson_id, &doomed.version_id).await;
    delete(&course, &doomed).await?;
    let revision = revision(&course.database, &lesson_id).await;
    ClaimReviewService::new(course.database.clone())
        .resolve(
            &lesson_id,
            &claim.0,
            ClaimActionRequestDto::Confirm {
                note: None,
                source_version_ids: versions(&doomed, &live),
            },
            revision,
        )
        .await?;
    Ok((course, lesson_id, doomed, live, claim))
}

#[tokio::test]
async fn a_confirmation_made_after_a_deletion_stays_confirmed_on_every_read() -> Result<()> {
    let (course, lesson_id, doomed, _, claim) =
        confirm_after_deletion(|doomed, _| vec![doomed.version_id.clone()]).await?;
    let service = ClaimReviewService::new(course.database.clone());
    let mut written = None;
    for read in 0..2 {
        service.review(&course.program_id, &lesson_id).await?;
        let bound = bound_versions(&course.database, &lesson_id, &claim).await;
        let bound = bound.unwrap_or_else(|| panic!("read {read}: the claim is not confirmed"));
        assert!(
            !bound.contains(&doomed.version_id),
            "read {read}: the confirmation names the deleted version"
        );
        let now = events_written(&course.database, &lesson_id).await;
        assert_eq!(*written.get_or_insert(now), now, "read {read} wrote");
    }
    Ok(())
}

#[tokio::test]
async fn a_confirmation_drops_the_deleted_versions_and_keeps_the_rest_in_order() -> Result<()> {
    let (course, lesson_id, _, live, claim) = confirm_after_deletion(|doomed, live| {
        vec![
            live.version_id.clone(),
            doomed.version_id.clone(),
            "unknown-version".into(),
        ]
    })
    .await?;
    assert_eq!(
        bound_versions(&course.database, &lesson_id, &claim).await,
        Some(vec![live.version_id.clone(), "unknown-version".into()])
    );
    Ok(())
}
