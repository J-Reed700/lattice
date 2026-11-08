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
use crate::lesson_evidence::{self, LearningLessonEvidenceDto};
use crate::lessons::claim_reviews::attach::{
    attach_to, AttachDeps, AttachRequest, Evidence, Others,
};
use crate::lessons::claim_reviews::dto::ClaimActionRequestDto;
use crate::lessons::claim_reviews::repository::ClaimReviewRepository;
use crate::lessons::claim_reviews::service::ClaimReviewService;
use crate::repository::LearningRepository;
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
            .contains("Only a section's text can be changed in a published lesson."),
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
async fn the_actions_a_published_lesson_offers_never_include_a_research_hint_or_an_undo_before_you_act_and_edit_and_remove_only_on_section_text(
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
        for never in ["research_with_hint", "undo"] {
            assert!(!offered.contains(&never.into()), "{item}");
        }
    }
    for item in claims {
        let statement = item["statement"].as_str().unwrap();
        let offered = actions(item);
        let section_text = statement == GOOD || statement == figure(7);
        assert_eq!(offered.contains(&"edit".into()), section_text, "{item}");
        assert_eq!(offered.contains(&"remove".into()), section_text, "{item}");
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

const CHANGED_WORDS: &str = "Only a section's text can be changed in a published lesson.";

/// Three published sections, each stating one statement not verified.
async fn three() -> Result<Course> {
    let course = Course::new().await?;
    let prepared = course
        .prepare(&unreadable(), &lesson(&[GOOD, &figure(7), &figure(8)]))
        .await
        .expect("the lesson publishes, its statements not verified");
    course.publish(prepared).await?;
    Ok(course)
}

fn three_statements() -> Vec<String> {
    vec![GOOD.into(), figure(7), figure(8)]
}

async fn ids_of(course: &Course, statements: &[String]) -> Result<Vec<String>> {
    let ledger = ledger(course).await?;
    Ok(statements
        .iter()
        .map(|statement| entry_for(&ledger, statement).claim_id().0.clone())
        .collect())
}

async fn act_many(
    course: &Course,
    ids: &[String],
    action: ClaimActionRequestDto,
    revision: i64,
) -> Result<i64> {
    ClaimReviewService::new(course.pool.clone())
        .resolve_many(&course.lesson_id, ids, action, revision)
        .await
}

/// The refusal a batch gets, with what it left written.
async fn batch_refused(
    course: &Course,
    ids: &[String],
    action: ClaimActionRequestDto,
    revision: i64,
) -> AppError {
    let before = stored(course).await.unwrap();
    let error = act_many(course, ids, action, revision).await.unwrap_err();
    assert_eq!(stored(course).await.unwrap(), before, "{error}");
    error
}

fn state_of<'a>(ledger: &'a Ledger, id: &str) -> &'a ClaimState {
    ledger
        .claims
        .iter()
        .find(|entry| entry.claim_id().0 == id)
        .unwrap()
        .state()
}

async fn offered_for(course: &Course, statement: &str) -> Result<Vec<String>> {
    let review = ClaimReviewService::new(course.pool.clone())
        .review(&course.program_id, &course.lesson_id)
        .await?
        .expect("the published lesson has its review");
    let review = serde_json::to_value(&review)?;
    let item = review["claims"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["statement"] == json!(statement))
        .unwrap_or_else(|| panic!("{statement}: {review}"));
    Ok(actions(item))
}

#[tokio::test]
async fn removing_a_published_statement_takes_its_sentence_out_cleanly_ends_every_claim_on_it_removed_by_you_and_updates_the_report(
) -> Result<()> {
    let course = unchecked().await?;
    let target = entry_for(&ledger(&course).await?, GOOD).claim_id().clone();
    let body = stored(&course).await?.blocks[0].clone();
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

    let revision = act(&course, &target, ClaimActionRequestDto::Remove).await?;

    let after = stored(&course).await?;
    assert_eq!(
        after.blocks[0],
        before.blocks[0].replacen(&format!("{GOOD} "), "", 1)
    );
    assert!(
        after.blocks[0].starts_with("Try it yourself"),
        "{}",
        after.blocks[0]
    );
    assert!(!after.blocks[0].contains("  "));
    assert_eq!(after.blocks[1], before.blocks[1]);
    assert_eq!(after.content_revision, before.content_revision);
    assert_eq!(revision, before.ledger_revision + 1);
    assert_eq!(after.ledger_revision, before.ledger_revision + 1);
    let ledger = ledger(&course).await?;
    for id in [&target, &overlap] {
        assert!(
            matches!(
                state_of(&ledger, &id.0),
                ClaimState::Removed {
                    by: Actor::User,
                    ..
                }
            ),
            "{id:?} is {:?}",
            state_of(&ledger, &id.0)
        );
    }
    assert_eq!(
        format!("{:?}", entry_for(&ledger, &figure(7))),
        other,
        "a claim the sentence does not overlap is untouched"
    );

    let now = read(&course).await?;
    assert!(now.claims.iter().all(|entry| entry["claim"] != json!(GOOD)));
    assert_eq!(now.view.claim_count, seen.view.claim_count - 1);
    assert_eq!(now.others(GOOD), seen.others(GOOD));
    assert_ne!(now.view.content_sha256, seen.view.content_sha256);
    assert_ne!(after.report, before.report);
    Ok(())
}

#[tokio::test]
async fn removing_a_published_statement_with_an_old_revision_writes_nothing() -> Result<()> {
    let course = unchecked().await?;
    let claim = entry_for(&ledger(&course).await?, GOOD).claim_id().clone();
    let revision = ledger(&course).await?.revision.0;
    let before = stored(&course).await?;

    let error = ClaimReviewService::new(course.pool.clone())
        .resolve(
            &course.lesson_id,
            &claim.0,
            ClaimActionRequestDto::Remove,
            revision + 1,
        )
        .await
        .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("This claim changed; reload and act again."),
        "{error}"
    );
    assert_eq!(stored(&course).await?, before);
    Ok(())
}

#[tokio::test]
async fn removing_a_questions_statement_from_a_published_lesson_is_refused_and_writes_nothing(
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

    let error = refused(&course, &claim, ClaimActionRequestDto::Remove).await;

    assert!(error.to_string().contains(CHANGED_WORDS), "{error}");
    Ok(())
}

#[tokio::test]
async fn removing_part_of_a_published_sentence_is_still_refused_and_writes_nothing() -> Result<()> {
    let course = unchecked().await?;
    let clause = "DictReader keeps whitespace in them";
    let body = format!(
        "The first row names the columns, and {clause}. {}",
        stored(&course).await?.blocks[0]
    );
    set_body(&course, 0, &body).await?;
    let claim = extract(
        &course,
        "clause",
        ClaimLocation::locate(0, UnitKind::Block, "body", clause, &body),
        clause,
    )
    .await?;

    let error = refused(&course, &claim, ClaimActionRequestDto::Remove).await;

    assert!(
        error
            .to_string()
            .contains("This claim is part of a sentence. Edit the sentence to say what goes."),
        "{error}"
    );
    Ok(())
}

#[tokio::test]
async fn the_undo_of_a_removal_in_a_published_lesson_puts_the_sentence_back_where_it_was_and_the_statement_as_it_was(
) -> Result<()> {
    let course = unchecked().await?;
    let claim = entry_for(&ledger(&course).await?, GOOD).claim_id().clone();
    let before = stored(&course).await?;
    let seen = read(&course).await?;
    let state = format!("{:?}", state_of(&ledger(&course).await?, &claim.0));
    act(&course, &claim, ClaimActionRequestDto::Remove).await?;
    assert!(offered_for(&course, GOOD).await?.contains(&"undo".into()));

    let revision = act(&course, &claim, ClaimActionRequestDto::Undo).await?;

    let after = stored(&course).await?;
    assert_eq!(
        after.blocks, before.blocks,
        "the text is as it was, whitespace and all"
    );
    assert_eq!(revision, before.ledger_revision + 2);
    assert_eq!(after.content_revision, before.content_revision);
    assert_eq!(
        format!("{:?}", state_of(&ledger(&course).await?, &claim.0)),
        state
    );
    let now = read(&course).await?;
    assert_eq!(
        now.claims, seen.claims,
        "the findings the removal dropped are back"
    );
    assert_eq!(now.view.claim_count, seen.view.claim_count);
    assert!(!offered_for(&course, GOOD).await?.contains(&"undo".into()));
    Ok(())
}

#[tokio::test]
async fn the_undo_of_a_removal_returns_every_claim_the_removal_ended_whichever_one_it_is_asked_on(
) -> Result<()> {
    for undo_on_overlap in [false, true] {
        let course = unchecked().await?;
        let target = entry_for(&ledger(&course).await?, GOOD).claim_id().clone();
        let body = stored(&course).await?.blocks[0].clone();
        let overlap = extract(
            &course,
            "overlap",
            ClaimLocation::locate(0, UnitKind::Block, "body", GOOD, &body),
            "Field-name whitespace is kept.",
        )
        .await?;
        let before = stored(&course).await?;
        let seen = read(&course).await?;
        let states = |ledger: &Ledger| {
            [&target, &overlap].map(|id| format!("{:?}", state_of(ledger, &id.0)))
        };
        let was = states(&ledger(&course).await?);
        act(&course, &target, ClaimActionRequestDto::Remove).await?;

        let asked = if undo_on_overlap { &overlap } else { &target };
        act(&course, asked, ClaimActionRequestDto::Undo).await?;

        let after = stored(&course).await?;
        assert_eq!(after.blocks, before.blocks);
        assert_eq!(states(&ledger(&course).await?), was);
        assert_eq!(read(&course).await?.claims, seen.claims);
    }
    Ok(())
}

#[tokio::test]
async fn the_undo_of_a_removal_restores_the_exact_text_around_the_sentence() -> Result<()> {
    for (body, sentence) in [
        ("A one. B two. C three.", "B two."),
        ("A one. B two. C three.", "A one."),
        ("A one. B two. C three.", "C three."),
        (
            "A one.

B two.

C three.",
            "B two.",
        ),
        (
            "A one.

B two. C three.",
            "B two.",
        ),
        (
            "A one. B two.

C three.",
            "B two.",
        ),
        (
            "A one.

B two.

C three.",
            "B two.",
        ),
        ("A one. B two.", "B two."),
    ] {
        let course = unchecked().await?;
        set_body(&course, 0, body).await?;
        let claim = extract(
            &course,
            "line",
            ClaimLocation::locate(0, UnitKind::Block, "body", sentence, body),
            sentence,
        )
        .await?;
        act(&course, &claim, ClaimActionRequestDto::Remove).await?;
        assert_ne!(stored(&course).await?.blocks[0], body);

        act(&course, &claim, ClaimActionRequestDto::Undo).await?;

        assert_eq!(
            stored(&course).await?.blocks[0],
            body,
            "{sentence:?} in {body:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn the_undo_of_a_removal_is_refused_by_hand_when_the_section_changed_after_it() -> Result<()>
{
    let course = unchecked().await?;
    let claim = entry_for(&ledger(&course).await?, GOOD).claim_id().clone();
    act(&course, &claim, ClaimActionRequestDto::Remove).await?;
    set_body(&course, 0, "The section was rewritten by hand.").await?;

    let error = refused(&course, &claim, ClaimActionRequestDto::Undo).await;

    assert!(error.to_string().contains(AFTER_IT), "{error}");
    Ok(())
}

#[tokio::test]
async fn a_batch_undo_of_a_batch_remove_puts_every_sentence_back_in_one_step() -> Result<()> {
    let course = three().await?;
    let statements = three_statements();
    let ids = ids_of(&course, &statements).await?;
    let before = stored(&course).await?;
    let seen = read(&course).await?;
    let removed = act_many(
        &course,
        &ids,
        ClaimActionRequestDto::Remove,
        before.ledger_revision,
    )
    .await?;

    let undone = act_many(&course, &ids, ClaimActionRequestDto::Undo, removed).await?;

    assert_eq!(undone, removed + 1);
    let after = stored(&course).await?;
    assert_eq!(after.blocks, before.blocks);
    assert_eq!(after.ledger_revision, removed + 1);
    assert_eq!(read(&course).await?.claims, seen.claims);
    let ledger = ledger(&course).await?;
    for id in &ids {
        assert!(
            matches!(state_of(&ledger, id), ClaimState::Unverified { .. }),
            "{:?}",
            state_of(&ledger, id)
        );
    }
    Ok(())
}

#[tokio::test]
async fn the_undo_of_an_accept_in_a_published_lesson_is_offered_and_puts_the_statement_back_not_verified(
) -> Result<()> {
    let course = unchecked().await?;
    let claim = entry_for(&ledger(&course).await?, GOOD).claim_id().clone();
    assert!(!offered_for(&course, GOOD).await?.contains(&"undo".into()));

    act(&course, &claim, confirm()).await?;

    assert!(offered_for(&course, GOOD).await?.contains(&"undo".into()));
    assert!(!offered_for(&course, &figure(7))
        .await?
        .contains(&"undo".into()));
    assert_eq!(read(&course).await?.resolution(GOOD), "confirmed_by_you");

    act(&course, &claim, ClaimActionRequestDto::Undo).await?;

    let back = read(&course).await?;
    assert_eq!(back.resolution(GOOD), "not_verified");
    assert_eq!(back.on(GOOD)["unverifiedReason"], json!("unchecked"));
    assert!(!offered_for(&course, GOOD).await?.contains(&"undo".into()));
    Ok(())
}

#[tokio::test]
async fn a_batch_accept_confirms_every_statement_and_moves_the_revision_once() -> Result<()> {
    let course = three().await?;
    let statements = three_statements();
    let ids = ids_of(&course, &statements).await?;
    let before = stored(&course).await?;

    let revision = act_many(&course, &ids, confirm(), before.ledger_revision).await?;

    assert_eq!(revision, before.ledger_revision + 1);
    let after = stored(&course).await?;
    assert_eq!(after.ledger_revision, before.ledger_revision + 1);
    assert_eq!(after.blocks, before.blocks);
    let now = read(&course).await?;
    for statement in &statements {
        assert_eq!(now.resolution(statement), "confirmed_by_you", "{statement}");
    }
    assert_eq!(now.view.confirmed_count, 3);
    Ok(())
}

#[tokio::test]
async fn a_batch_with_one_statement_that_cannot_be_removed_writes_nothing_and_says_how_many(
) -> Result<()> {
    let course = unchecked().await?;
    let prompt = "Which setting keeps header whitespace?";
    let question = extract(
        &course,
        "question",
        ClaimLocation::locate(0, UnitKind::Question, "prompt", prompt, prompt),
        prompt,
    )
    .await?;
    let mut ids = ids_of(&course, &[GOOD.into(), figure(7)]).await?;
    ids.insert(1, question.0.clone());
    let revision = ledger(&course).await?.revision.0;

    let error = batch_refused(&course, &ids, ClaimActionRequestDto::Remove, revision).await;

    assert!(
        error.to_string().contains(&format!(
            "1 of these can't be removed: {CHANGED_WORDS} Nothing was changed."
        )),
        "{error}"
    );
    Ok(())
}

#[tokio::test]
async fn a_batch_where_none_can_be_removed_says_none_of_them_can() -> Result<()> {
    let course = unchecked().await?;
    let prompt = "Which setting keeps header whitespace?";
    let question = extract(
        &course,
        "question",
        ClaimLocation::locate(0, UnitKind::Question, "prompt", prompt, prompt),
        prompt,
    )
    .await?;
    let revision = ledger(&course).await?.revision.0;

    let error = batch_refused(
        &course,
        &[question.0],
        ClaimActionRequestDto::Remove,
        revision,
    )
    .await;

    assert!(
        error
            .to_string()
            .contains(&format!("None of these can be removed: {CHANGED_WORDS}")),
        "{error}"
    );
    assert!(
        !error.to_string().contains("Nothing was changed"),
        "{error}"
    );
    Ok(())
}

#[tokio::test]
async fn a_batch_remove_on_a_published_lesson_takes_every_sentence_out_in_one_step() -> Result<()> {
    let course = three().await?;
    let statements = three_statements();
    let ids = ids_of(&course, &statements).await?;
    let before = stored(&course).await?;
    let seen = read(&course).await?;

    let revision = act_many(
        &course,
        &ids,
        ClaimActionRequestDto::Remove,
        before.ledger_revision,
    )
    .await?;

    assert_eq!(revision, before.ledger_revision + 1);
    let after = stored(&course).await?;
    assert_eq!(after.ledger_revision, before.ledger_revision + 1);
    for (index, statement) in statements.iter().enumerate() {
        assert_eq!(
            after.blocks[index],
            before.blocks[index].replacen(&format!("{statement} "), "", 1)
        );
        assert!(after.blocks[index].starts_with("Try it yourself"));
    }
    let ledger = ledger(&course).await?;
    for id in &ids {
        assert!(
            matches!(
                state_of(&ledger, id),
                ClaimState::Removed {
                    by: Actor::User,
                    ..
                }
            ),
            "{:?}",
            state_of(&ledger, id)
        );
    }
    let now = read(&course).await?;
    assert_eq!(now.view.claim_count, seen.view.claim_count - 3);
    Ok(())
}

#[tokio::test]
async fn a_batch_remove_of_two_statements_on_one_sentence_takes_the_sentence_out_once() -> Result<()>
{
    let course = unchecked().await?;
    let target = entry_for(&ledger(&course).await?, GOOD).claim_id().clone();
    let body = stored(&course).await?.blocks[0].clone();
    let overlap = extract(
        &course,
        "overlap",
        ClaimLocation::locate(0, UnitKind::Block, "body", GOOD, &body),
        "Field-name whitespace is kept.",
    )
    .await?;
    let before = stored(&course).await?;

    let revision = act_many(
        &course,
        &[target.0.clone(), overlap.0.clone()],
        ClaimActionRequestDto::Remove,
        before.ledger_revision,
    )
    .await?;

    assert_eq!(revision, before.ledger_revision + 1);
    let after = stored(&course).await?;
    assert_eq!(
        after.blocks[0],
        before.blocks[0].replacen(&format!("{GOOD} "), "", 1)
    );
    let ledger = ledger(&course).await?;
    for id in [&target, &overlap] {
        assert!(matches!(
            state_of(&ledger, &id.0),
            ClaimState::Removed {
                by: Actor::User,
                ..
            }
        ));
    }
    Ok(())
}

#[tokio::test]
async fn a_batch_undo_with_the_revision_the_batch_returned_undoes_every_statement() -> Result<()> {
    let course = three().await?;
    let statements = three_statements();
    let ids = ids_of(&course, &statements).await?;
    let first = ledger(&course).await?.revision.0;
    let accepted = act_many(&course, &ids, confirm(), first).await?;

    let undone = act_many(&course, &ids, ClaimActionRequestDto::Undo, accepted).await?;

    assert_eq!(undone, accepted + 1);
    let now = read(&course).await?;
    for statement in &statements {
        assert_eq!(now.resolution(statement), "not_verified", "{statement}");
    }
    let ledger = ledger(&course).await?;
    for id in &ids {
        assert!(
            matches!(state_of(&ledger, id), ClaimState::Unverified { .. }),
            "{:?}",
            state_of(&ledger, id)
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_batch_undo_after_the_revision_moved_is_refused_and_writes_nothing() -> Result<()> {
    let course = three().await?;
    let statements = three_statements();
    let ids = ids_of(&course, &statements).await?;
    let first = ledger(&course).await?.revision.0;
    let accepted = act_many(&course, &ids, confirm(), first).await?;
    act(
        &course,
        &ClaimId(ids[0].clone()),
        ClaimActionRequestDto::Unconfirm,
    )
    .await?;

    let error = batch_refused(&course, &ids, ClaimActionRequestDto::Undo, accepted).await;

    assert!(
        error
            .to_string()
            .contains("Something changed since then. Undo them one at a time."),
        "{error}"
    );
    Ok(())
}

#[tokio::test]
async fn a_batch_asked_for_something_that_needs_one_statement_is_refused_and_writes_nothing(
) -> Result<()> {
    let course = three().await?;
    let ids = ids_of(&course, &three_statements()).await?;
    let revision = ledger(&course).await?.revision.0;
    let with_a_note = ClaimActionRequestDto::Confirm {
        note: Some("Checked by hand.".into()),
        source_version_ids: vec![],
    };

    for action in [edit(GOOD, NEW), with_a_note] {
        let error = batch_refused(&course, &ids, action, revision).await;
        assert!(
            error.to_string().contains("Choose one statement for that."),
            "{error}"
        );
    }
    Ok(())
}

const AFTER_IT: &str = "This section changed after you removed this sentence, so it can't go back on its own. Edit the section to add it back.";
const FIRST_UNDO: &str = "First undo the other sentences you removed from this section.";

const ALPHA: &str = "Alpha comes first.";
const BRAVO: &str = "Bravo comes second.";
const CHARLIE: &str = "Charlie comes third.";
const DELTA: &str = "Delta comes fourth.";
const ECHO: &str = "Echo comes last.";

fn five() -> String {
    [ALPHA, BRAVO, CHARLIE, DELTA, ECHO].join(" ")
}

/// The first section reads as `five`, with a claim on each of the three
/// sentences between the first and the last.
async fn adjacent(course: &Course) -> Result<[ClaimId; 3]> {
    let body = five();
    set_body(course, 0, &body).await?;
    let mut ids = Vec::new();
    for (id, sentence) in [("bravo", BRAVO), ("charlie", CHARLIE), ("delta", DELTA)] {
        let location = ClaimLocation::locate(0, UnitKind::Block, "body", sentence, &body);
        ids.push(extract(course, id, location, sentence).await?);
    }
    Ok(ids.try_into().unwrap())
}

fn in_order(body: &str) -> bool {
    let at: Vec<usize> = [ALPHA, BRAVO, CHARLIE, DELTA, ECHO]
        .iter()
        .filter_map(|sentence| body.find(sentence))
        .collect();
    at.windows(2).all(|pair| pair[0] < pair[1])
}

#[tokio::test]
async fn a_batch_undo_of_a_batch_remove_of_adjacent_statements_leaves_the_text_byte_for_byte_as_it_was(
) -> Result<()> {
    let course = unchecked().await?;
    let [bravo, charlie, _] = adjacent(&course).await?;
    let before = stored(&course).await?;
    let ids = vec![bravo.0.clone(), charlie.0.clone()];
    let removed = act_many(
        &course,
        &ids,
        ClaimActionRequestDto::Remove,
        before.ledger_revision,
    )
    .await?;
    assert_eq!(
        stored(&course).await?.blocks[0],
        [ALPHA, DELTA, ECHO].join(" ")
    );

    act_many(&course, &ids, ClaimActionRequestDto::Undo, removed).await?;

    assert_eq!(stored(&course).await?.blocks, before.blocks);
    Ok(())
}

#[tokio::test]
async fn adjacent_removals_undone_in_any_order_come_back_in_place_or_are_refused_until_the_ones_after_them_are_back(
) -> Result<()> {
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let course = unchecked().await?;
        let ids = adjacent(&course).await?;
        for id in &ids {
            act(&course, id, ClaimActionRequestDto::Remove).await?;
        }
        assert_eq!(stored(&course).await?.blocks[0], [ALPHA, ECHO].join(" "));

        let mut waiting = order.to_vec();
        while !waiting.is_empty() {
            let mut refused_now = Vec::new();
            for index in &waiting {
                let before = stored(&course).await?;
                match act(&course, &ids[*index], ClaimActionRequestDto::Undo).await {
                    Ok(_) => {
                        let body = stored(&course).await?.blocks[0].clone();
                        assert!(in_order(&body), "{order:?}: {body}");
                    }
                    Err(error) => {
                        assert!(error.to_string().contains(FIRST_UNDO), "{error}");
                        assert_eq!(stored(&course).await?, before, "{order:?}");
                        refused_now.push(*index);
                    }
                }
            }
            assert!(refused_now.len() < waiting.len(), "{order:?}: no progress");
            waiting = refused_now;
        }

        assert_eq!(stored(&course).await?.blocks[0], five(), "{order:?}");
    }
    Ok(())
}

#[tokio::test]
async fn the_undo_of_a_removal_is_refused_when_a_neighbour_was_edited_after_it_and_names_that_reason(
) -> Result<()> {
    let course = unchecked().await?;
    let [bravo, charlie, _] = adjacent(&course).await?;
    act(&course, &bravo, ClaimActionRequestDto::Remove).await?;
    act(
        &course,
        &charlie,
        edit(CHARLIE, "Delta replaces the third."),
    )
    .await?;

    let error = refused(&course, &bravo, ClaimActionRequestDto::Undo).await;

    assert!(error.to_string().contains(AFTER_IT), "{error}");
    assert!(!error.to_string().contains(FIRST_UNDO), "{error}");
    Ok(())
}
