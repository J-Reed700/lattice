#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
//! A published lesson's not-verified statements are the user's to settle
//! (settle-published design): the learner view follows each claim's current
//! state, and the user confirms, edits or sources a statement in place.
use super::claim_ledger_tests::{figure, lesson, unreadable, Course, Figures};
use super::{fixture_completion, fixture_prompts, GOOD};
use crate::features::learning::lesson_evidence::{self, LearningLessonEvidenceDto};
use crate::features::learning::lessons::claim_reviews::attach::{
    attach_to, AttachDeps, AttachRequest, Evidence, Others,
};
use crate::features::learning::lessons::claim_reviews::dto::ClaimActionRequestDto;
use crate::features::learning::lessons::claim_reviews::repository::ClaimReviewRepository;
use crate::features::learning::lessons::claim_reviews::service::ClaimReviewService;
use crate::features::learning::repository::LearningRepository;
use async_trait::async_trait;
use lattice_core::application::contracts::web::{
    FetchUrlContentOutput, WebSearchInput, WebSearchOutput,
};
use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::application::ports::{
    LLMPort, LibraryDocumentText, LibraryPassageHit, LibraryPassagesPort, ReferenceSnapshot,
    ReferenceSnapshotPort, WebServiceTrait,
};
use lattice_core::domain::learning::claim_review::{
    Actor, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimLocation, ClaimState, EventProvenance,
    Ledger, ReopenCause, TrustCause, UnitKind,
};
use lattice_core::domain::provenance::ClaimKind;
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};
use std::collections::HashSet;

/// The sentence the user writes in place of a statement.
const NEW: &str = "DictReader keeps field-name whitespace exactly as the header row writes it.";

/// A lesson of two sections whose statements the judge never read: both
/// are published not verified as `unchecked`.
async fn unchecked() -> Result<Course> {
    let course = Course::new().await?;
    let prepared = course
        .prepare(&unreadable(), &lesson(&[GOOD, &figure(7)]))
        .await
        .expect("the lesson publishes, its statements not verified");
    course.publish(prepared).await?;
    Ok(course)
}

/// A lesson whose one statement the sources state, published verified.
async fn verified() -> Result<Course> {
    let course = Course::new().await?;
    let prepared = course
        .prepare(&Figures::new(), &lesson(&[GOOD]))
        .await
        .expect("the lesson publishes");
    course.publish(prepared).await?;
    Ok(course)
}

/// The published lesson as the learner reads it now.
struct Read {
    view: LearningLessonEvidenceDto,
    claims: Vec<Value>,
}

async fn read(course: &Course) -> Result<Read> {
    let view = lesson_evidence::get(&course.pool, &course.program_id, &course.lesson_id)
        .await?
        .expect("the lesson has its evidence");
    let claims = view
        .teaching_claims
        .iter()
        .map(serde_json::to_value)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(Read { view, claims })
}

impl Read {
    fn on(&self, sentence: &str) -> &Value {
        let on: Vec<_> = self
            .claims
            .iter()
            .filter(|entry| entry["claim"] == json!(sentence))
            .collect();
        assert_eq!(on.len(), 1, "{sentence}: {:?}", self.claims);
        on[0]
    }

    fn resolution(&self, sentence: &str) -> &str {
        self.on(sentence)["resolution"].as_str().unwrap()
    }

    fn count(&self, resolution: &str) -> usize {
        self.claims
            .iter()
            .filter(|entry| entry["resolution"] == json!(resolution))
            .count()
    }

    /// Every claim but the one on `sentence`.
    fn others(&self, sentence: &str) -> Vec<&Value> {
        self.claims
            .iter()
            .filter(|entry| entry["claim"] != json!(sentence))
            .collect()
    }
}

async fn ledger(course: &Course) -> Result<Ledger> {
    ClaimReviewRepository::new(course.pool.clone())
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

fn machine() -> EventProvenance {
    EventProvenance {
        actor: Actor::Machine,
        at: chrono::Utc::now().timestamp_millis(),
        job_id: None,
        app_version: "test".into(),
        checker_model: Some("fixture".into()),
    }
}

/// Starts a claim on the lesson's ledger, as a pass would have left it.
async fn extract(
    course: &Course,
    id: &str,
    location: ClaimLocation,
    statement: &str,
) -> Result<ClaimId> {
    let id = ClaimId(id.into());
    let revision = ledger(course).await?.revision;
    ClaimReviewRepository::new(course.pool.clone())
        .append(
            &course.lesson_id,
            &id,
            machine(),
            ClaimEvent::Extracted {
                location,
                statement: statement.into(),
            },
            revision,
        )
        .await?;
    Ok(id)
}

async fn act(course: &Course, claim: &ClaimId, action: ClaimActionRequestDto) -> Result<i64> {
    let revision = ledger(course).await?.revision.0;
    ClaimReviewService::new(course.pool.clone())
        .resolve(&course.lesson_id, &claim.0, action, revision)
        .await
}

fn confirm() -> ClaimActionRequestDto {
    ClaimActionRequestDto::Confirm {
        note: None,
        source_version_ids: vec![],
    }
}

fn edit(before: &str, after: &str) -> ClaimActionRequestDto {
    ClaimActionRequestDto::Edit {
        before: before.into(),
        after: after.into(),
    }
}

/// Everything an edit of the published text may write.
#[derive(Debug, PartialEq)]
struct Stored {
    blocks: Vec<String>,
    ledger: String,
    ledger_revision: i64,
    report: Option<String>,
    content_revision: i64,
    program_revision: String,
}

async fn stored(course: &Course) -> Result<Stored> {
    let blocks = sqlx::query_scalar::<_, String>(
        "SELECT body FROM learning_blocks WHERE lesson_id=? ORDER BY ordinal",
    )
    .bind(&course.lesson_id)
    .fetch_all(&course.pool)
    .await?;
    let report = sqlx::query_scalar::<_, String>(
        "SELECT report_json FROM learning_lesson_verifications WHERE program_id=? AND lesson_id=?",
    )
    .bind(&course.program_id)
    .bind(&course.lesson_id)
    .fetch_optional(&course.pool)
    .await?;
    let content_revision =
        sqlx::query_scalar::<_, i64>("SELECT content_revision FROM learning_programs WHERE id=?")
            .bind(&course.program_id)
            .fetch_one(&course.pool)
            .await?;
    let ledger = ledger(course).await?;
    let program = LearningRepository::new(course.pool.clone())
        .get(&course.program_id)
        .await?;
    Ok(Stored {
        blocks,
        ledger: format!("{:?}", ledger.claims),
        ledger_revision: ledger.revision.0,
        report,
        content_revision,
        program_revision: format!("{:?}", program.summary.revision),
    })
}

async fn set_body(course: &Course, ordinal: i64, body: &str) -> Result<()> {
    sqlx::query("UPDATE learning_blocks SET body=? WHERE lesson_id=? AND ordinal=?")
        .bind(body)
        .bind(&course.lesson_id)
        .bind(ordinal)
        .execute(&course.pool)
        .await?;
    Ok(())
}

#[tokio::test]
async fn a_published_statement_the_user_confirms_reads_confirmed_by_you_and_unconfirm_brings_it_back_not_verified(
) -> Result<()> {
    let course = unchecked().await?;
    let before = read(&course).await?;
    assert_eq!(before.resolution(GOOD), "not_verified");
    assert_eq!(before.count("not_verified"), 2);
    let claim = entry_for(&ledger(&course).await?, GOOD).claim_id().clone();

    act(&course, &claim, confirm()).await?;

    let confirmed = read(&course).await?;
    assert_eq!(confirmed.resolution(GOOD), "confirmed_by_you");
    assert_eq!(confirmed.on(GOOD)["unverifiedReason"], Value::Null);
    assert_eq!(confirmed.count("not_verified"), 1);
    assert_eq!(confirmed.view.confirmed_count, 1);
    assert_eq!(confirmed.view.claim_count, before.view.claim_count);
    assert_eq!(confirmed.resolution(&figure(7)), "not_verified");

    act(&course, &claim, ClaimActionRequestDto::Unconfirm).await?;

    let back = read(&course).await?;
    assert_eq!(back.resolution(GOOD), "not_verified");
    assert_eq!(back.on(GOOD)["unverifiedReason"], json!("unchecked"));
    assert_eq!(back.count("not_verified"), 2);
    assert_eq!(back.view.confirmed_count, 0);
    Ok(())
}

#[tokio::test]
async fn a_verified_statement_a_trust_change_reopened_reads_not_verified_as_unchecked() -> Result<()>
{
    let course = verified().await?;
    assert_eq!(read(&course).await?.resolution(GOOD), "verified");
    let claim = entry_for(&ledger(&course).await?, GOOD).claim_id().clone();
    let revision = ledger(&course).await?.revision;

    ClaimReviewRepository::new(course.pool.clone())
        .append(
            &course.lesson_id,
            &claim,
            machine(),
            ClaimEvent::Reopened {
                cause: ReopenCause::TrustChanged(TrustCause::Bar {
                    kind: ClaimKind::Specification,
                }),
            },
            revision,
        )
        .await?;

    let reopened = read(&course).await?;
    assert_eq!(reopened.resolution(GOOD), "not_verified");
    assert_eq!(reopened.on(GOOD)["unverifiedReason"], json!("unchecked"));
    assert_eq!(reopened.count("verified"), 0);
    Ok(())
}

#[tokio::test]
async fn a_stored_report_without_claim_ids_reads_as_it_was_stored_whatever_its_ledger_says(
) -> Result<()> {
    let course = verified().await?;
    let stored_report: String = sqlx::query_scalar(
        "SELECT report_json FROM learning_lesson_verifications WHERE program_id=? AND lesson_id=?",
    )
    .bind(&course.program_id)
    .bind(&course.lesson_id)
    .fetch_one(&course.pool)
    .await?;
    let mut report: Value = serde_json::from_str(&stored_report)?;
    for finding in report["findings"].as_array_mut().unwrap() {
        finding.as_object_mut().unwrap().remove("claim_id");
    }
    sqlx::query(
        "UPDATE learning_lesson_verifications SET report_json=? WHERE program_id=? AND lesson_id=?",
    )
    .bind(report.to_string())
    .bind(&course.program_id)
    .bind(&course.lesson_id)
    .execute(&course.pool)
    .await?;
    let before = serde_json::to_value(&read(&course).await?.view)?;
    assert_eq!(before["teachingClaims"][0]["resolution"], json!("verified"));
    let claim = entry_for(&ledger(&course).await?, GOOD).claim_id().clone();
    let revision = ledger(&course).await?.revision;
    ClaimReviewRepository::new(course.pool.clone())
        .append(
            &course.lesson_id,
            &claim,
            machine(),
            ClaimEvent::Reopened {
                cause: ReopenCause::TrustChanged(TrustCause::Bar {
                    kind: ClaimKind::Specification,
                }),
            },
            revision,
        )
        .await?;

    let after = serde_json::to_value(&read(&course).await?.view)?;

    assert_eq!(after, before);
    Ok(())
}

#[tokio::test]
async fn an_edit_of_a_published_statement_rewrites_that_sentence_and_the_user_stands_behind_the_new_one(
) -> Result<()> {
    let course = unchecked().await?;
    let sentence_before = ledger(&course).await?;
    let target = entry_for(&sentence_before, GOOD);
    let quote = target.location().quote.clone();
    let target = target.claim_id().clone();
    let body = stored(&course).await?.blocks[0].clone();
    // A second claim over the same sentence.
    let overlap = extract(
        &course,
        "overlap",
        ClaimLocation::locate(0, UnitKind::Block, "body", GOOD, &body),
        "Field-name whitespace is kept.",
    )
    .await?;
    let before = stored(&course).await?;
    let seen = read(&course).await?;
    let other = format!("{:?}", entry_for(&ledger(&course).await?, &figure(7)));

    act(&course, &target, edit(&quote, NEW)).await?;

    let after = stored(&course).await?;
    assert_eq!(after.blocks[0], before.blocks[0].replace(&quote, NEW));
    assert!(!after.blocks[0].contains(GOOD));
    assert_eq!(after.blocks[1], before.blocks[1]);
    assert_eq!(after.content_revision, before.content_revision);
    let ledger = ledger(&course).await?;
    let mut successors = Vec::new();
    for id in [&target, &overlap] {
        let entry = ledger.claims.iter().find(|e| e.claim_id() == id).unwrap();
        let ClaimState::Edited {
            successor: Some(successor),
            by: Actor::User,
            ..
        } = entry.state()
        else {
            panic!("{id:?} is {:?}", entry.state());
        };
        successors.push(successor.clone());
    }
    assert_eq!(successors[0], successors[1]);
    let successor = ledger
        .claims
        .iter()
        .find(|entry| entry.claim_id() == &successors[0])
        .unwrap();
    assert_eq!(successor.statement(), NEW);
    assert_eq!(successor.location().unit_index, 0);
    assert!(matches!(successor.state(), ClaimState::Confirmed { .. }));
    assert_eq!(
        format!("{:?}", entry_for(&ledger, &figure(7))),
        other,
        "a claim the sentence does not overlap is untouched"
    );

    let now = read(&course).await?;
    assert_eq!(now.resolution(NEW), "confirmed_by_you");
    assert_eq!(now.on(NEW)["claimId"], json!(successors[0].0));
    assert!(now.claims.iter().all(|entry| entry["claim"] != json!(GOOD)));
    assert_eq!(now.view.claim_count, seen.view.claim_count);
    assert_eq!(now.others(NEW), seen.others(GOOD));
    assert_ne!(now.view.content_sha256, seen.view.content_sha256);
    assert_ne!(after.report, before.report);
    Ok(())
}

#[tokio::test]
async fn an_edit_naming_a_sentence_as_plain_text_replaces_it_as_the_lesson_writes_it() -> Result<()>
{
    let course = unchecked().await?;
    let marked = "The **DictReader** class keeps field-name whitespace.";
    let plain = "The DictReader class keeps field-name whitespace.";
    let body = format!("{} {marked}", stored(&course).await?.blocks[0]);
    set_body(&course, 0, &body).await?;
    let claim = extract(
        &course,
        "marked",
        ClaimLocation::locate(0, UnitKind::Block, "body", marked, &body),
        plain,
    )
    .await?;
    let before = stored(&course).await?;

    act(&course, &claim, edit(plain, NEW)).await?;

    let after = stored(&course).await?;
    assert_eq!(after.blocks[0], before.blocks[0].replace(marked, NEW));
    assert!(!after.blocks[0].contains("**"));
    assert_eq!(after.blocks[1], before.blocks[1]);
    assert_eq!(after.content_revision, before.content_revision);
    Ok(())
}

/// The refusal an edit of `claim` gets, with what it left written.
async fn refused(course: &Course, claim: &ClaimId, action: ClaimActionRequestDto) -> AppError {
    let before = stored(course).await.unwrap();
    let error = act(course, claim, action).await.unwrap_err();
    assert_eq!(stored(course).await.unwrap(), before, "{error}");
    error
}

#[tokio::test]
async fn an_edit_with_blank_text_is_refused_and_writes_nothing() -> Result<()> {
    let course = unchecked().await?;
    let claim = entry_for(&ledger(&course).await?, GOOD).claim_id().clone();

    let error = refused(&course, &claim, edit(GOOD, "   ")).await;

    assert!(matches!(error, AppError::InvalidInput(_)), "{error:?}");
    Ok(())
}

#[tokio::test]
async fn an_edit_of_a_questions_claim_in_a_published_lesson_is_refused_and_writes_nothing(
) -> Result<()> {
    let course = unchecked().await?;
    let prompt = "Which setting keeps header whitespace?";
    let claim = extract(
        &course,
        "question",
        ClaimLocation::locate(0, UnitKind::Question, "prompt", prompt, prompt),
        prompt,
    )
    .await?;

    let error = refused(&course, &claim, edit(prompt, NEW)).await;

    assert!(
        error
            .to_string()
            .contains("Only a section's text can be edited in a published lesson."),
        "{error}"
    );
    Ok(())
}

#[tokio::test]
async fn an_edit_of_a_sentence_that_changed_since_the_claim_recorded_it_is_refused_and_writes_nothing(
) -> Result<()> {
    let course = unchecked().await?;
    let claim = entry_for(&ledger(&course).await?, GOOD).claim_id().clone();
    let body = stored(&course).await?.blocks[0].replace(GOOD, "A sentence written since.");
    set_body(&course, 0, &body).await?;

    let error = refused(&course, &claim, edit(GOOD, NEW)).await;

    assert!(
        error
            .to_string()
            .contains("This statement changed since you opened it; reload and try again."),
        "{error}"
    );
    Ok(())
}

fn actions(item: &Value) -> Vec<String> {
    item["availableActions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|action| action.as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn the_actions_a_published_lesson_offers_never_include_remove_or_a_research_hint_and_edit_only_on_section_text(
) -> Result<()> {
    let course = unchecked().await?;
    let prompt = "Which setting keeps header whitespace?";
    extract(
        &course,
        "question",
        ClaimLocation::locate(0, UnitKind::Question, "prompt", prompt, prompt),
        prompt,
    )
    .await?;
    extract(
        &course,
        "title",
        ClaimLocation::locate(0, UnitKind::Block, "title", "Section 0", "Section 0"),
        "Section 0 names the first step.",
    )
    .await?;

    let review = ClaimReviewService::new(course.pool.clone())
        .review(&course.program_id, &course.lesson_id)
        .await?
        .expect("the published lesson has its review");

    let review = serde_json::to_value(&review)?;
    let claims = review["claims"].as_array().unwrap();
    assert_eq!(claims.len(), 4);
    for item in claims {
        let offered = actions(item);
        for never in ["remove", "research_with_hint", "undo"] {
            assert!(!offered.contains(&never.into()), "{item}");
        }
    }
    for item in claims {
        let statement = item["statement"].as_str().unwrap();
        let offered = actions(item);
        let section_text = statement == GOOD || statement == figure(7);
        assert_eq!(offered.contains(&"edit".into()), section_text, "{item}");
        if section_text {
            assert!(offered.contains(&"confirm".into()), "{item}");
            assert!(offered.contains(&"attach_evidence".into()), "{item}");
        }
    }
    Ok(())
}

struct NoWeb;

#[async_trait]
impl WebServiceTrait for NoWeb {
    async fn search_web(&self, _input: &WebSearchInput) -> Result<WebSearchOutput> {
        Err(AppError::ServiceNotAvailable("no web here".into()))
    }
    async fn fetch_url_content(&self, _url: &str) -> Result<FetchUrlContentOutput> {
        Err(AppError::ServiceNotAvailable("no web here".into()))
    }
    async fn fetch_reference_content(&self, _url: &str) -> Result<FetchUrlContentOutput> {
        Err(AppError::ServiceNotAvailable("no web here".into()))
    }
    fn validate_url(&self, _url: &str) -> Result<()> {
        Ok(())
    }
}

struct NoSnapshots;

#[async_trait]
impl ReferenceSnapshotPort for NoSnapshots {
    async fn request(&self, _url: &str, _retry: bool) -> Result<ReferenceSnapshot> {
        Ok(ReferenceSnapshot::Pending)
    }
    async fn status(&self, _url: &str) -> Result<Option<ReferenceSnapshot>> {
        Ok(None)
    }
}

struct NoLibrary;

#[async_trait]
impl LibraryPassagesPort for NoLibrary {
    fn model_identity(&self) -> String {
        "none".into()
    }
    async fn document_text(&self, _document_id: &str) -> Result<Option<LibraryDocumentText>> {
        Ok(None)
    }
    async fn search(
        &self,
        _query: &str,
        _document_ids: &HashSet<String>,
        _limit: usize,
    ) -> Result<Vec<LibraryPassageHit>> {
        Ok(Vec::new())
    }
}

/// A judge that finds every statement supported by the passage it is shown.
struct Supports;

#[async_trait]
impl LLMPort for Supports {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let replies = fixture_prompts(request)
            .into_iter()
            .map(|_| {
                "supported\nReason: The fixture compared the passage.\nSource passage: passage-0"
                    .to_owned()
            })
            .collect();
        Ok(fixture_completion(request, replies))
    }
    fn model_name(&self) -> &str {
        "supports-fixture"
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    fn max_context_tokens(&self) -> usize {
        128_000
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn a_source_the_user_attaches_that_supports_a_published_not_verified_statement_makes_it_read_verified(
) -> Result<()> {
    let course = unchecked().await?;
    assert_eq!(read(&course).await?.resolution(GOOD), "not_verified");
    let claim = entry_for(&ledger(&course).await?, GOOD).claim_id().clone();
    let revision = ledger(&course).await?.revision;
    let store = ClaimReviewRepository::new(course.pool.clone());

    attach_to(
        &AttachDeps {
            pool: &course.pool,
            store: &store,
            web: &NoWeb,
            snapshots: &NoSnapshots,
            documents: &NoLibrary,
            courses: None,
            llm: &Supports,
            job_id: None,
        },
        AttachRequest {
            program_id: &course.program_id,
            lesson_id: &course.lesson_id,
            claim_id: &claim,
            expected_revision: revision,
            evidence: Evidence::Text {
                title: "My csv notes".into(),
                text: format!("{GOOD} Rows without a value read as None."),
            },
        },
        Others::Only(&[]),
        &|_| {},
    )
    .await?;

    let now = read(&course).await?;
    assert_eq!(now.resolution(GOOD), "verified");
    assert_eq!(now.on(GOOD)["unverifiedReason"], Value::Null);
    assert_eq!(now.resolution(&figure(7)), "not_verified");
    Ok(())
}
