#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
//! Undo and revert of claim actions and direct section edits through `ContentEdits`.
use super::claim_ledger_tests::{figure, lesson, section, unreadable, Course, Figures};
use super::GOOD;
use crate::edits::keys;
use crate::lessons::claim_reviews::dto::ClaimActionRequestDto;
use crate::lessons::claim_reviews::repository::ClaimReviewRepository;
use crate::lessons::claim_reviews::service::ClaimReviewService;
use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::domain::learning::claim_review::{
    Actor, Check, CheckResult, Checker, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimLocation,
    ClaimState, EventProvenance, EvidenceFingerprint, Ledger, RepairOutcome, UnitKind,
    UnverifiedReason,
};
use lattice_core::shared::edits::{
    ContentEditOutcome, ContentEditRequest, ContentRef, EditOp, EditSource,
    RevertContentEditRequest,
};
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};
use uuid::Uuid;

const NEW: &str = "DictReader keeps field-name whitespace exactly as the header row writes it.";
const ACTED_SINCE: &str =
    "A statement this change ended was acted on since. Undo that action in the review, then undo this change.";
const UNDO_BLOCKED: &str =
    "This section changed after that, so it can't be undone on its own. Edit the section to change it back.";

const ALPHA: &str = "Alpha comes first.";
const BRAVO: &str = "Bravo comes second.";
const CHARLIE: &str = "Charlie comes third.";
const DELTA: &str = "Delta comes fourth.";
const ECHO: &str = "Echo comes last.";

async fn unchecked() -> Result<Course> {
    let course = Course::new().await?;
    let prepared = course
        .prepare(&unreadable(), &lesson(&[GOOD, &figure(7)]))
        .await
        .expect("the lesson publishes, its statements not verified");
    course.publish(prepared).await?;
    Ok(course)
}

async fn verified_course(sections: &[&str]) -> Result<Course> {
    let course = Course::new().await?;
    let prepared = course
        .prepare(&Figures::new(), &lesson(sections))
        .await
        .expect("the lesson publishes");
    course.publish(prepared).await?;
    Ok(course)
}

async fn ledger(course: &Course) -> Result<Ledger> {
    ClaimReviewRepository::new(course.db.clone())
        .load(&course.lesson_id)
        .await
}

fn entry_for<'a>(ledger: &'a Ledger, statement: &str) -> &'a ClaimLedgerEntry {
    let found: Vec<_> = ledger
        .claims
        .iter()
        .filter(|entry| entry.statement() == statement)
        .collect();
    assert_eq!(found.len(), 1, "{statement}: {:?}", ledger.claims);
    found[0]
}

async fn section_body(course: &Course, ordinal: usize) -> Result<String> {
    Ok(sqlx::query_scalar::<_, String>(
        "SELECT body FROM learning_blocks WHERE lesson_id=? AND ordinal=?",
    )
    .bind(&course.lesson_id)
    .bind(i64::try_from(ordinal).unwrap())
    .fetch_one(&course.pool)
    .await?)
}

async fn stored_report(course: &Course) -> Result<(String, String)> {
    Ok(sqlx::query_as::<_, (String, String)>(
        "SELECT content_sha256, report_json FROM learning_lesson_verifications WHERE program_id=? AND lesson_id=?",
    )
    .bind(&course.program_id)
    .bind(&course.lesson_id)
    .fetch_one(&course.pool)
    .await?)
}

async fn apply_edit(
    course: &Course,
    content: ContentRef,
    ops: Vec<EditOp>,
) -> Result<ContentEditOutcome> {
    let edits = course.edits();
    let opened = edits.open(&content).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened?;
    let outcome = edits
        .apply(&ContentEditRequest {
            edit_id: Uuid::new_v4().to_string(),
            content,
            expected_revision: opened.revision,
            ops,
            source: EditSource::Learner,
            target: None,
        })
        .await;
    assert!(outcome.is_ok(), "{outcome:?}");
    outcome
}

fn replace(path: &str, value: &str) -> EditOp {
    EditOp::Replace {
        path: path.into(),
        value: json!(value),
    }
}

#[tokio::test]
async fn undoing_a_rewrite_restores_its_checked_statements() -> Result<()> {
    let course = verified_course(&[GOOD, &figure(7)]).await?;
    let before_body = section_body(&course, 0).await?;
    let before_findings =
        serde_json::from_str::<Value>(&stored_report(&course).await?.1)?["findings"].clone();
    let before_ledger = ledger(&course).await?;
    let old_entry = entry_for(&before_ledger, GOOD);
    let old_id = old_entry.claim_id().clone();
    let old_state = format!("{:?}", old_entry.state());
    let content = keys::section_ref(&course.lesson_id, 0);

    let applied = apply_edit(&course, content, vec![replace("/body", &section(NEW))]).await?;
    let after_edit = ledger(&course).await?;
    let learner_claim = entry_for(&after_edit, NEW).claim_id().clone();

    let edits = course.edits();
    let reverted = edits
        .revert(&RevertContentEditRequest {
            revert_id: Uuid::new_v4().to_string(),
            edit_id: applied.edit_id,
            expected_revision: applied.revision,
            and_later: false,
        })
        .await;
    assert!(reverted.is_ok(), "{reverted:?}");

    assert_eq!(section_body(&course, 0).await?, before_body);
    let after_findings =
        serde_json::from_str::<Value>(&stored_report(&course).await?.1)?["findings"].clone();
    assert_eq!(after_findings, before_findings);
    let after_revert = ledger(&course).await?;
    let restored = after_revert
        .claims
        .iter()
        .find(|c| c.claim_id() == &old_id)
        .unwrap();
    assert_eq!(format!("{:?}", restored.state()), old_state);
    let retired = after_revert
        .claims
        .iter()
        .find(|c| c.claim_id() == &learner_claim)
        .unwrap();
    assert!(matches!(
        retired.state(),
        ClaimState::Edited {
            successor: None,
            by: Actor::User,
            ..
        }
    ));
    Ok(())
}

#[tokio::test]
async fn a_revert_is_refused_when_an_ended_statement_was_acted_on_since() -> Result<()> {
    let course = unchecked().await?;
    let cut = "Rows are read lazily from the file.";
    let rewrite = "Rows are read from the file.";
    let body_now = format!("{} {rewrite}", section_body(&course, 0).await?);
    sqlx::query("UPDATE learning_blocks SET body=? WHERE lesson_id=? AND ordinal=0")
        .bind(&body_now)
        .bind(&course.lesson_id)
        .execute(&course.pool)
        .await?;
    let id = ClaimId("rewritten".into());
    let store = ClaimReviewRepository::new(course.db.clone());
    let prov = EventProvenance {
        actor: Actor::Machine,
        at: 1,
        job_id: None,
        app_version: "test".into(),
        checker_model: Some("fixture".into()),
    };
    let mut rev = ledger(&course).await?.revision;
    for event in [
        ClaimEvent::Extracted {
            location: ClaimLocation::locate(
                0,
                UnitKind::Block,
                "body",
                cut,
                &format!("{} {cut}", section_body(&course, 0).await?),
            ),
            statement: cut.into(),
        },
        ClaimEvent::Checked(Check {
            checker: Checker::Local {
                model: "fixture".into(),
            },
            result: CheckResult::Failed {
                kind: lattice_core::domain::learning::claim_review::VerdictKind::Unsupported,
            },
            reason: "Unsupported.".into(),
            fingerprint: EvidenceFingerprint::of("strict", "fixture", &[]),
            evidence: Vec::new(),
            window_tokens: 128_000,
        }),
        ClaimEvent::RepairAttempted {
            attempt: 1,
            escalated: false,
            before: cut.into(),
            after: Some(rewrite.into()),
            outcome: RepairOutcome::Rewritten,
            goal: None,
        },
        ClaimEvent::Superseded {
            successor: Some(ClaimId("rewrite".into())),
            by: Actor::Machine,
        },
    ] {
        rev = store
            .append(&course.lesson_id, &id, prov.clone(), event, rev)
            .await?;
    }

    let service = ClaimReviewService::new(course.db.clone());
    let rev = service
        .resolve(
            &course.lesson_id,
            &id.0,
            ClaimActionRequestDto::RestoreOriginal,
            rev.0,
        )
        .await?;
    service
        .resolve(
            &course.lesson_id,
            &id.0,
            ClaimActionRequestDto::Unconfirm,
            rev,
        )
        .await?;

    let key = keys::section_ref(&course.lesson_id, 0).key;
    let edit_ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM content_edits WHERE kind=? AND content_key=? ORDER BY seq DESC",
    )
    .bind(keys::SECTION)
    .bind(&key)
    .fetch_all(&course.pool)
    .await?;
    assert!(
        !edit_ids.is_empty(),
        "RestoreOriginal must record a content_edits row"
    );

    let edits = course.edits();
    let opened = edits.open(&keys::section_ref(&course.lesson_id, 0)).await?;
    let before_body = section_body(&course, 0).await?;
    let before_claims = format!("{:?}", ledger(&course).await?.claims);

    let result = edits
        .revert(&RevertContentEditRequest {
            revert_id: Uuid::new_v4().to_string(),
            edit_id: edit_ids[0].clone(),
            expected_revision: opened.revision,
            and_later: false,
        })
        .await;

    assert!(
        matches!(&result, Err(AppError::InvalidState(message)) if message == ACTED_SINCE),
        "{result:?}"
    );
    assert_eq!(section_body(&course, 0).await?, before_body);
    assert_eq!(
        format!("{:?}", ledger(&course).await?.claims),
        before_claims
    );
    Ok(())
}

#[tokio::test]
async fn undoing_a_removal_restores_the_sentence_and_its_checked_statement() -> Result<()> {
    let course = verified_course(&[GOOD]).await?;
    let before_body = section_body(&course, 0).await?;
    let before_ledger = ledger(&course).await?;
    let entry = entry_for(&before_ledger, GOOD);
    let claim_id = entry.claim_id().clone();
    let before_state = format!("{:?}", entry.state());
    let service = ClaimReviewService::new(course.db.clone());

    let rev = service
        .resolve(
            &course.lesson_id,
            &claim_id.0,
            ClaimActionRequestDto::Remove,
            before_ledger.revision.0,
        )
        .await?;
    assert!(!section_body(&course, 0).await?.contains(GOOD));

    service
        .resolve(
            &course.lesson_id,
            &claim_id.0,
            ClaimActionRequestDto::Undo,
            rev,
        )
        .await?;

    assert_eq!(section_body(&course, 0).await?, before_body);
    let after = ledger(&course).await?;
    let restored = after
        .claims
        .iter()
        .find(|c| c.claim_id() == &claim_id)
        .unwrap();
    assert_eq!(format!("{:?}", restored.state()), before_state);

    let key = keys::section_ref(&course.lesson_id, 0).key;
    let reverts: Vec<String> = sqlx::query_scalar(
        "SELECT reverts_edit_id FROM content_edits WHERE kind=? AND content_key=? AND reverts_edit_id IS NOT NULL",
    )
    .bind(keys::SECTION)
    .bind(&key)
    .fetch_all(&course.pool)
    .await?;
    assert_eq!(reverts.len(), 1, "{reverts:?}");
    Ok(())
}

#[tokio::test]
async fn an_undo_is_refused_once_the_section_changed_after_it() -> Result<()> {
    let course = unchecked().await?;
    let before_ledger = ledger(&course).await?;
    let claim_id = entry_for(&before_ledger, GOOD).claim_id().clone();
    let service = ClaimReviewService::new(course.db.clone());

    service
        .resolve(
            &course.lesson_id,
            &claim_id.0,
            ClaimActionRequestDto::Remove,
            before_ledger.revision.0,
        )
        .await?;

    let content = keys::section_ref(&course.lesson_id, 0);
    let new_body = format!("{} Learner addition.", section_body(&course, 0).await?);
    apply_edit(&course, content, vec![replace("/body", &new_body)]).await?;

    let rev = ledger(&course).await?.revision.0;
    let before_body = section_body(&course, 0).await?;
    let result = service
        .resolve(
            &course.lesson_id,
            &claim_id.0,
            ClaimActionRequestDto::Undo,
            rev,
        )
        .await;

    assert!(
        matches!(&result, Err(AppError::InvalidState(message)) if message == UNDO_BLOCKED),
        "{result:?}"
    );
    assert_eq!(section_body(&course, 0).await?, before_body);
    Ok(())
}

async fn three_sentences(course: &Course) -> Result<[ClaimId; 3]> {
    let body = [ALPHA, BRAVO, CHARLIE, DELTA, ECHO].join(" ");
    sqlx::query("UPDATE learning_blocks SET body=? WHERE lesson_id=? AND ordinal=0")
        .bind(&body)
        .bind(&course.lesson_id)
        .execute(&course.pool)
        .await?;
    let store = ClaimReviewRepository::new(course.db.clone());
    let prov = EventProvenance {
        actor: Actor::Machine,
        at: 1,
        job_id: None,
        app_version: "test".into(),
        checker_model: Some("fixture".into()),
    };
    let mut ids = Vec::new();
    for (name, sentence) in [("bravo", BRAVO), ("charlie", CHARLIE), ("delta", DELTA)] {
        let id = ClaimId(name.into());
        let mut rev = ledger(course).await?.revision;
        for event in [
            ClaimEvent::Extracted {
                location: ClaimLocation::locate(0, UnitKind::Block, "body", sentence, &body),
                statement: sentence.into(),
            },
            ClaimEvent::PublishedUnverified {
                reason: UnverifiedReason::Unchecked,
            },
        ] {
            rev = store
                .append(&course.lesson_id, &id, prov.clone(), event, rev)
                .await?;
        }
        ids.push(id);
    }
    Ok(ids.try_into().unwrap())
}

#[tokio::test]
async fn a_batch_of_removals_is_undone_newest_first() -> Result<()> {
    let course = unchecked().await?;
    let [bravo, charlie, _] = three_sentences(&course).await?;
    let before_body = section_body(&course, 0).await?;
    let rev = ledger(&course).await?.revision.0;
    let service = ClaimReviewService::new(course.db.clone());
    let ids = vec![bravo.0.clone(), charlie.0.clone()];

    let removed_rev = service
        .resolve_many(&course.lesson_id, &ids, ClaimActionRequestDto::Remove, rev)
        .await?;
    assert_eq!(removed_rev, rev + 2);
    assert_eq!(
        section_body(&course, 0).await?,
        [ALPHA, DELTA, ECHO].join(" ")
    );

    let key = keys::section_ref(&course.lesson_id, 0).key;
    let forward_ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM content_edits WHERE kind=? AND content_key=? ORDER BY seq ASC",
    )
    .bind(keys::SECTION)
    .bind(&key)
    .fetch_all(&course.pool)
    .await?;
    assert_eq!(forward_ids.len(), 2);

    let undone_rev = service
        .resolve_many(
            &course.lesson_id,
            &ids,
            ClaimActionRequestDto::Undo,
            removed_rev,
        )
        .await?;
    assert_eq!(undone_rev, removed_rev + 2);
    assert_eq!(section_body(&course, 0).await?, before_body);

    let reverts: Vec<String> = sqlx::query_scalar(
        "SELECT reverts_edit_id FROM content_edits WHERE kind=? AND content_key=? AND reverts_edit_id IS NOT NULL ORDER BY seq ASC",
    )
    .bind(keys::SECTION)
    .bind(&key)
    .fetch_all(&course.pool)
    .await?;
    assert_eq!(
        reverts,
        vec![forward_ids[1].clone(), forward_ids[0].clone()]
    );
    Ok(())
}

#[tokio::test]
async fn a_batch_undo_that_cannot_reach_a_statement_writes_nothing() -> Result<()> {
    let course = unchecked().await?;
    let [bravo, charlie, delta] = three_sentences(&course).await?;
    let service = ClaimReviewService::new(course.db.clone());
    let mut rev = ledger(&course).await?.revision.0;
    for id in [&bravo, &charlie, &delta] {
        rev = service
            .resolve(&course.lesson_id, &id.0, ClaimActionRequestDto::Remove, rev)
            .await?;
    }
    let before_body = section_body(&course, 0).await?;
    let before_claims = format!("{:?}", ledger(&course).await?.claims);

    let result = service
        .resolve_many(
            &course.lesson_id,
            &[bravo.0.clone(), delta.0.clone()],
            ClaimActionRequestDto::Undo,
            rev,
        )
        .await;

    assert!(
        matches!(&result, Err(AppError::InvalidState(message)) if message == UNDO_BLOCKED),
        "{result:?}"
    );
    assert_eq!(section_body(&course, 0).await?, before_body);
    assert_eq!(
        format!("{:?}", ledger(&course).await?.claims),
        before_claims
    );
    assert_eq!(ledger(&course).await?.revision.0, rev);
    Ok(())
}
