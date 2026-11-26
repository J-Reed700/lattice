#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
//! The `learning.claims` reactor and claim addresses on `ContentEdits` and `ClaimReviewService`.
use super::claim_ledger_tests::{figure, lesson, section, unreadable, Course, Figures};
use super::GOOD;
use crate::assessment_engine::{
    LearningAssessmentCandidate, LearningAssessmentKey, LearningItemFormat,
};
use crate::assessment_repository::{insert_bank_items_in, BankItem};
use crate::edits::keys;
use crate::lessons::claim_reviews::dto::ClaimActionRequestDto;
use crate::lessons::claim_reviews::repository::ClaimReviewRepository;
use crate::lessons::claim_reviews::service::ClaimReviewService;
use crate::lessons::claim_reviews::teaching_findings::TeachingFindingsRepository;
use crate::review_findings::{FindingKind, FindingSite, ReviewFinding, SetAside};
use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::domain::learning::claim_review::{
    Actor, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimLocation, ClaimState, EventProvenance,
    Ledger, UnitKind, UnverifiedReason,
};
use lattice_core::shared::edits::{
    ContentEditOutcome, ContentEditPreview, ContentEditRequest, ContentRef, EditOp, EditOrigin,
    EditSource,
};
use lattice_core::shared::error::Result;
use serde_json::{json, Value};
use uuid::Uuid;

const NEW: &str = "DictReader keeps field-name whitespace exactly as the header row writes it.";
const CHECKED_ONE: &str = "Saving changes 1 statement that was checked against the sources. What you write is marked as written by you, not checked.";
const CHECKED_TWO: &str = "Saving changes 2 statements that were checked against the sources. What you write is marked as written by you, not checked.";

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

async fn preview_edit(
    course: &Course,
    content: ContentRef,
    ops: Vec<EditOp>,
) -> Result<ContentEditPreview> {
    let edits = course.edits();
    let opened = edits.open(&content).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened?;
    let preview = edits
        .preview(&ContentEditRequest {
            edit_id: Uuid::new_v4().to_string(),
            content,
            expected_revision: opened.revision,
            ops,
            source: EditSource::Learner,
            target: None,
        })
        .await;
    assert!(preview.is_ok(), "{preview:?}");
    preview
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
async fn a_rewritten_sentence_ends_its_statement_and_is_recorded_as_the_learners() -> Result<()> {
    let course = verified_course(&[GOOD, &figure(7)]).await?;
    let before_rev = ledger(&course).await?.revision.0;
    let old = entry_for(&ledger(&course).await?, GOOD).claim_id().clone();
    let content = keys::section_ref(&course.lesson_id, 0);

    let outcome = apply_edit(&course, content, vec![replace("/body", &section(NEW))]).await?;

    assert!(outcome.changed);
    assert!(outcome
        .affected
        .iter()
        .any(|name| name == keys::affected::CLAIMS));
    let after = ledger(&course).await?;
    assert_eq!(after.revision.0, before_rev + 1);
    let successor = entry_for(&after, NEW);
    assert!(matches!(successor.state(), ClaimState::Confirmed { .. }));
    assert!(successor
        .events()
        .iter()
        .any(|record| record.provenance.actor == Actor::User));
    let ended = after
        .claims
        .iter()
        .find(|entry| entry.claim_id() == &old)
        .unwrap();
    assert!(matches!(
        ended.state(),
        ClaimState::Edited {
            successor: Some(ref id),
            by: Actor::User,
            ..
        } if id == successor.claim_id()
    ));
    let report = serde_json::from_str::<Value>(&stored_report(&course).await?.1)?;
    let findings = report["findings"].as_array().unwrap();
    assert!(findings.iter().all(|f| f["claim_id"] != json!(old.0)));
    assert!(findings.iter().any(|f| {
        f["claim_id"] == json!(successor.claim_id().0)
            && f["resolution"]["kind"] == json!("confirmed")
    }));
    Ok(())
}

#[tokio::test]
async fn a_new_sentence_ends_nothing() -> Result<()> {
    let course = verified_course(&[GOOD]).await?;
    let before = ledger(&course).await?;
    let body = format!("{} Headers are never trimmed for you.", section(GOOD));
    let content = keys::section_ref(&course.lesson_id, 0);

    let preview = preview_edit(&course, content.clone(), vec![replace("/body", &body)]).await?;
    assert!(
        preview
            .consequences
            .iter()
            .all(|c| c.code != "claims.checked_statements"),
        "{:?}",
        preview.consequences
    );

    apply_edit(&course, content, vec![replace("/body", &body)]).await?;

    let after = ledger(&course).await?;
    assert_eq!(
        format!("{:?}", entry_for(&after, GOOD)),
        format!("{:?}", entry_for(&before, GOOD))
    );
    let added = entry_for(&after, "Headers are never trimmed for you.");
    assert!(matches!(added.state(), ClaimState::Confirmed { .. }));
    Ok(())
}

#[tokio::test]
async fn a_preview_counts_the_checked_statements_a_rewrite_ends() -> Result<()> {
    let course = verified_course(&[GOOD]).await?;
    let p_one = preview_edit(
        &course,
        keys::section_ref(&course.lesson_id, 0),
        vec![replace("/body", &section(NEW))],
    )
    .await?;
    let c_one = p_one
        .consequences
        .iter()
        .find(|c| c.code == "claims.checked_statements")
        .expect("one checked consequence");
    assert_eq!(c_one.message, CHECKED_ONE);

    let ledger_now = ledger(&course).await?;
    let verified_entry = entry_for(&ledger_now, GOOD);
    let body = section_body(&course, 0).await?;
    let second = ClaimId(Uuid::new_v4().to_string());
    let store = ClaimReviewRepository::new(course.db.clone());
    let prov = EventProvenance {
        actor: Actor::Machine,
        at: 2,
        job_id: None,
        app_version: "test".into(),
        checker_model: Some("fixture".into()),
    };
    let mut rev = store
        .append(
            &course.lesson_id,
            &second,
            prov.clone(),
            ClaimEvent::Extracted {
                location: ClaimLocation::locate(0, UnitKind::Block, "body", GOOD, &body),
                statement: "Header whitespace stays intact.".into(),
            },
            ledger_now.revision,
        )
        .await?;
    for record in verified_entry.events() {
        if matches!(
            record.event,
            ClaimEvent::Classified { .. }
                | ClaimEvent::Checked(_)
                | ClaimEvent::StandingAssessed { .. }
        ) {
            rev = store
                .append(
                    &course.lesson_id,
                    &second,
                    prov.clone(),
                    record.event.clone(),
                    rev,
                )
                .await?;
        }
    }
    assert!(matches!(
        entry_for(&ledger(&course).await?, "Header whitespace stays intact.").state(),
        ClaimState::ResolvedByEvidence { .. }
    ));

    let p_two = preview_edit(
        &course,
        keys::section_ref(&course.lesson_id, 0),
        vec![replace("/body", &section(NEW))],
    )
    .await?;
    let c_two = p_two
        .consequences
        .iter()
        .find(|c| c.code == "claims.checked_statements")
        .expect("two checked consequence");
    assert_eq!(c_two.message, CHECKED_TWO);
    Ok(())
}

async fn record_section_note(
    course: &Course,
    section_idx: usize,
    sentence: Option<&str>,
) -> Result<()> {
    let candidate = json!({
        "blocks": [
            {"title": "Section 0", "body": section(GOOD)},
            {"title": "Section 1", "body": section(&figure(7))}
        ],
        "questions": []
    });
    let mut finding = ReviewFinding::of_kind(
        FindingKind::Teaching,
        FindingSite::Section(section_idx),
        "A note.",
    )
    .with_summary("Check this sentence.");
    finding.sentence = sentence.map(str::to_owned);
    TeachingFindingsRepository::new(course.db.clone())
        .record(
            &course.lesson_id,
            "job",
            &candidate,
            &[SetAside {
                finding,
                repairs: 0,
            }],
            true,
        )
        .await
}

#[tokio::test]
async fn a_section_edit_marks_its_teaching_notes() -> Result<()> {
    let course = unchecked().await?;
    record_section_note(&course, 0, None).await?;
    let before = TeachingFindingsRepository::new(course.db.clone())
        .list(&course.lesson_id)
        .await?;
    assert!(!before[0].part_changed);

    apply_edit(
        &course,
        keys::section_ref(&course.lesson_id, 0),
        vec![replace("/body", &section(NEW))],
    )
    .await?;

    let after = TeachingFindingsRepository::new(course.db.clone())
        .list(&course.lesson_id)
        .await?;
    assert!(after[0].part_changed);
    Ok(())
}

#[tokio::test]
async fn a_sentence_removed_in_claim_review_marks_the_sections_teaching_notes() -> Result<()> {
    let course = unchecked().await?;
    record_section_note(&course, 0, None).await?;
    let ledger = ledger(&course).await?;
    let claim = entry_for(&ledger, GOOD).claim_id().clone();

    ClaimReviewService::new(course.db.clone())
        .resolve(
            &course.lesson_id,
            &claim.0,
            ClaimActionRequestDto::Remove,
            ledger.revision.0,
        )
        .await?;

    let after = TeachingFindingsRepository::new(course.db.clone())
        .list(&course.lesson_id)
        .await?;
    assert!(
        after[0].part_changed,
        "claim removal must mark the section's teaching notes changed"
    );
    Ok(())
}

#[tokio::test]
async fn a_claim_removal_is_one_edit_of_the_section() -> Result<()> {
    let course = unchecked().await?;
    let ledger = ledger(&course).await?;
    let claim = entry_for(&ledger, GOOD).claim_id().clone();

    ClaimReviewService::new(course.db.clone())
        .resolve(
            &course.lesson_id,
            &claim.0,
            ClaimActionRequestDto::Remove,
            ledger.revision.0,
        )
        .await?;

    assert!(!section_body(&course, 0).await?.contains(GOOD));
    let key = keys::section_ref(&course.lesson_id, 0).key;
    let origins: Vec<String> = sqlx::query_scalar(
        "SELECT origin_json FROM content_edits WHERE kind=? AND content_key=? ORDER BY seq",
    )
    .bind(keys::SECTION)
    .bind(&key)
    .fetch_all(&course.pool)
    .await?;
    assert_eq!(origins.len(), 1, "{origins:?}");
    let origin: EditOrigin = serde_json::from_str(&origins[0])?;
    assert_eq!(
        origin,
        EditOrigin::Action {
            name: "claim.remove".into()
        }
    );
    Ok(())
}

async fn insert_question(
    course: &Course,
    lesson_id: Option<&str>,
    origin: &'static str,
    prompt: &str,
    explanation: &str,
) -> Result<String> {
    let outcome = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO learning_outcome_definitions(id,program_id,module_id,lesson_id,title,description,ordinal,created_at) VALUES(?,?,NULL,?,?,?,0,0)",
    )
    .bind(&outcome)
    .bind(&course.program_id)
    .bind(lesson_id)
    .bind("CSV headers")
    .bind("Read CSV headers cleanly")
    .execute(&course.pool)
    .await?;
    let id = Uuid::new_v4().to_string();
    let item = BankItem {
        item: LearningAssessmentCandidate {
            id: id.clone(),
            outcome_ids: vec![outcome],
            format: LearningItemFormat::SingleChoice,
            difficulty: 3,
            prompt: prompt.into(),
            options: vec![
                "It keeps them.".into(),
                "It trims them.".into(),
                "It drops them.".into(),
                "It raises an error.".into(),
            ],
            left: Vec::new(),
            source_version_ids: Vec::new(),
            rubric: Vec::new(),
            key: LearningAssessmentKey::Choice(0),
            explanation: explanation.into(),
            distractor_rationales: Vec::new(),
        }
        .into(),
        source_quote: String::new(),
        lesson_id: lesson_id.map(str::to_owned),
        origin,
        review_status: "agreed",
    };
    let mut tx = course.db.begin_write().await?;
    insert_bank_items_in(&mut tx, &course.program_id, &[item], None).await?;
    tx.commit().await?;
    Ok(id)
}

#[tokio::test]
async fn an_edited_question_ends_the_statements_on_its_changed_fields() -> Result<()> {
    let course = unchecked().await?;
    let expl = "Header names keep spaces intact.";
    let question_id = insert_question(
        &course,
        Some(&course.lesson_id),
        "lesson",
        "How does DictReader handle spaces?",
        expl,
    )
    .await?;
    let store = ClaimReviewRepository::new(course.db.clone());
    let claim_id = ClaimId(Uuid::new_v4().to_string());
    let prov = EventProvenance {
        actor: Actor::Machine,
        at: 1,
        job_id: None,
        app_version: "test".into(),
        checker_model: Some("fixture".into()),
    };
    let rev = ledger(&course).await?.revision;
    let rev = store
        .append(
            &course.lesson_id,
            &claim_id,
            prov.clone(),
            ClaimEvent::Extracted {
                location: ClaimLocation::locate(2, UnitKind::AnswerKey, "explanation", expl, expl),
                statement: expl.into(),
            },
            rev,
        )
        .await?;
    store
        .append(
            &course.lesson_id,
            &claim_id,
            prov,
            ClaimEvent::PublishedUnverified {
                reason: UnverifiedReason::Unchecked,
            },
            rev,
        )
        .await?;

    let candidate_val = json!({
        "blocks": [
            {"title": "Section 0", "body": section(GOOD)},
            {"title": "Section 1", "body": section(&figure(7))}
        ],
        "questions": [{
            "prompt": "How does DictReader handle spaces?",
            "options": ["It keeps them.", "It trims them.", "It drops them.", "It raises an error."],
            "explanation": expl
        }]
    });
    TeachingFindingsRepository::new(course.db.clone())
        .record(
            &course.lesson_id,
            "job",
            &candidate_val,
            &[SetAside {
                finding: ReviewFinding::of_kind(
                    FindingKind::AnswerKey,
                    FindingSite::Question(0),
                    "Check explanation.",
                ),
                repairs: 0,
            }],
            true,
        )
        .await?;

    let new_expl = "Header names preserve every space verbatim.";
    apply_edit(
        &course,
        keys::question_ref(&question_id),
        vec![replace("/explanation", new_expl)],
    )
    .await?;

    let after = ledger(&course).await?;
    let ended = after
        .claims
        .iter()
        .find(|entry| entry.claim_id() == &claim_id)
        .unwrap();
    let written = entry_for(&after, new_expl);
    assert_eq!(written.location().unit_index, 2);
    assert_eq!(written.location().unit_kind, UnitKind::AnswerKey);
    assert_eq!(written.location().field, "explanation");
    assert!(matches!(
        ended.state(),
        ClaimState::Edited {
            successor: Some(ref id),
            by: Actor::User,
            ..
        } if id == written.claim_id()
    ));
    let notes = TeachingFindingsRepository::new(course.db.clone())
        .list(&course.lesson_id)
        .await?;
    let q_note = notes
        .iter()
        .find(|n| n.site == TeachingFindingSiteDto::Question && n.unit_index == Some(0))
        .unwrap();
    assert!(q_note.part_changed);
    Ok(())
}

#[tokio::test]
async fn a_bank_question_has_no_ledger_unit() -> Result<()> {
    let course = unchecked().await?;
    let bank_id = insert_question(
        &course,
        None,
        "bank",
        "Why does DictReader preserve headers?",
        "Preserving headers avoids losing columns.",
    )
    .await?;
    let before_events: usize = ledger(&course)
        .await?
        .claims
        .iter()
        .map(|c| c.events().len())
        .sum();

    apply_edit(
        &course,
        keys::question_ref(&bank_id),
        vec![replace(
            "/explanation",
            "Keeping exact headers avoids bugs.",
        )],
    )
    .await?;

    let after_events: usize = ledger(&course)
        .await?
        .claims
        .iter()
        .map(|c| c.events().len())
        .sum();
    assert_eq!(after_events, before_events);
    Ok(())
}

#[tokio::test]
async fn a_statement_carries_the_address_of_its_sentence() -> Result<()> {
    let course = unchecked().await?;
    let body = section_body(&course, 0).await?;

    let review = ClaimReviewService::new(course.db.clone())
        .review(&course.program_id, &course.lesson_id)
        .await?
        .expect("review");
    let item = review
        .claims
        .iter()
        .find(|c| c.statement == GOOD)
        .expect("claim for GOOD");

    assert!(
        item.location.address.is_some(),
        "claim location should carry an address"
    );
    let address = item.location.address.as_ref().unwrap();
    assert_eq!(address.content, keys::section_ref(&course.lesson_id, 0));
    assert_eq!(address.path.as_deref(), Some("/body"));
    assert!(
        address.fragment.is_some(),
        "expected sentence fragment selector"
    );
    let fragment = address.fragment.as_ref().unwrap();
    assert_eq!(fragment.exact, GOOD);
    assert!(fragment.holds(&body));
    Ok(())
}
