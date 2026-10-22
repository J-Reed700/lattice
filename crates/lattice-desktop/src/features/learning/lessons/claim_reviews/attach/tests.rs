#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
//! Attaching evidence end to end over a real database, against an
//! in-memory ledger, a scripted web, frozen-copy service and judge (§5.4,
//! §12): the link failures of the §5.4 table (fixture
//! `fixtures/attach_link_failures.json`), a supporting and a contradicting
//! source, the other open claims, dedupe, provenance and the frozen copy.

use std::collections::HashSet;
use std::sync::Mutex;

use async_trait::async_trait;
use lattice_core::application::contracts::web::{
    FetchUrlContentOutput, WebSearchInput, WebSearchOutput,
};
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::application::ports::{
    ClaimReviewStore, LLMPort, LibraryDocumentText, LibraryPassageHit, LibraryPassagesPort,
    ReferenceSnapshot, ReferenceSnapshotPort, WebServiceTrait,
};
use lattice_core::domain::learning::claim_review::{
    Actor, AttachOutcome, Check, CheckResult, Checker, ClaimEvent, ClaimId, ClaimLedgerEntry,
    ClaimLocation, ClaimState, EventProvenance, EvidenceFingerprint, Ledger, LessonReview,
    Revision, SourceAddedBy, UnitKind, VerdictKind,
};
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};

use super::failures::AttachFailureDto;
use super::report::{ClaimAttachProgressDto, SnapshotNoteDto};
use super::{attach, attach_to, AttachDeps, AttachRequest, Evidence, Others};
use crate::features::learning::lessons::claim_reviews::dto::{AttachOutcomeDto, SourceAddedByDto};
use crate::features::learning::lessons::content_verification::tests::{
    fixture_completion, fixture_prompts,
};
use crate::features::learning::source_library::LearningSourceLibraryRepository;
use crate::features::learning::{repository::LearningRepository, tests};

const LESSON: &str = "lesson-1";
const WATER: &str = "Water boils at 100 °C at sea level.";
// The mercury claim shares no term with the water sources: under the subject
// rule a passage "mentions" a claim through any distinctive shared term, the
// unit "°C" included, and these tests need a source that is about another
// subject entirely.
const MERCURY: &str = "Mercury freezes at -38.83 degrees Celsius.";
const FIELD: &str =
    "Heat it. Water boils at 100 °C at sea level. Mercury freezes at -38.83 degrees Celsius.";
const NOTES: &str = "My boiling notes";
const PASTED: &str = "Measured in the kitchen at sea level: water boils at 100 °C, and the pot only gets hotter once it has boiled dry.";

fn fixture() -> Value {
    serde_json::from_str(include_str!("../fixtures/attach_link_failures.json")).unwrap()
}

fn provenance(actor: Actor) -> EventProvenance {
    EventProvenance {
        actor,
        at: 1,
        job_id: None,
        app_version: "test".into(),
        checker_model: Some("fixture-judge".into()),
    }
}

/// A claim the machine checked once without support: Open, so evidence may
/// be attached.
fn open_claim(id: &str, statement: &str) -> ClaimLedgerEntry {
    let mut entry = ClaimLedgerEntry::extracted(
        ClaimId(id.into()),
        provenance(Actor::Machine),
        ClaimLocation::locate(0, UnitKind::Block, "body", statement, FIELD),
        statement.into(),
    )
    .unwrap();
    entry
        .append(
            provenance(Actor::Machine),
            ClaimEvent::Checked(Check {
                checker: Checker::Local {
                    model: "fixture-judge".into(),
                },
                result: CheckResult::Failed {
                    kind: VerdictKind::Unsupported,
                },
                reason: "No saved source states it.".into(),
                fingerprint: EvidenceFingerprint::of("strict", "fixture-judge", &[]),
                evidence: Vec::new(),
                window_tokens: 128_000,
            }),
        )
        .unwrap();
    entry
}

/// The ledger port over memory, validating every event as the store does.
struct MemoryLedger {
    ledger: Mutex<Ledger>,
}

impl MemoryLedger {
    fn new(program_id: &str, claims: Vec<ClaimLedgerEntry>) -> Self {
        Self {
            ledger: Mutex::new(Ledger {
                lesson_id: LESSON.into(),
                review: Some(LessonReview {
                    program_id: program_id.into(),
                    job_id: "job-1".into(),
                    state:
                        lattice_core::domain::learning::claim_review::LessonReviewState::NeedsReview,
                    draft_sha256: "draft".into(),
                    checker_model: "fixture-judge".into(),
                    stop_reason: None,
                }),
                revision: Revision(7),
                claims,
            }),
        }
    }

    fn claim(&self, id: &str) -> ClaimLedgerEntry {
        let ledger = self.ledger.lock().unwrap();
        ledger
            .claims
            .iter()
            .find(|entry| entry.claim_id().0 == id)
            .cloned()
            .unwrap()
    }

    fn revision(&self) -> Revision {
        self.ledger.lock().unwrap().revision
    }
}

#[async_trait]
impl ClaimReviewStore for MemoryLedger {
    async fn load(&self, _lesson_id: &str) -> Result<Ledger> {
        Ok(self.ledger.lock().unwrap().clone())
    }

    async fn append(
        &self,
        _lesson_id: &str,
        claim_id: &ClaimId,
        provenance: EventProvenance,
        event: ClaimEvent,
        expected_revision: Revision,
    ) -> Result<Revision> {
        let mut ledger = self.ledger.lock().unwrap();
        if ledger.revision != expected_revision {
            return Err(AppError::InvalidState("stale revision".into()));
        }
        let entry = ledger
            .claims
            .iter_mut()
            .find(|entry| entry.claim_id() == claim_id)
            .ok_or_else(|| AppError::NotFound("claim".into()))?;
        entry.append(provenance, event)?;
        ledger.revision = Revision(ledger.revision.0 + 1);
        Ok(ledger.revision)
    }

    async fn set_lesson_review(&self, _lesson_id: &str, review: LessonReview) -> Result<Revision> {
        let mut ledger = self.ledger.lock().unwrap();
        ledger.review = Some(review);
        ledger.revision = Revision(ledger.revision.0 + 1);
        Ok(ledger.revision)
    }
}

/// The page or the error the web service answers with.
enum Reply {
    Page {
        url: String,
        title: String,
        content: String,
    },
    Error(fn(String) -> AppError, String),
}

struct ScriptedWeb {
    reply: Reply,
    fetched: Mutex<usize>,
}

impl ScriptedWeb {
    fn of(case: &Value) -> Self {
        let fetch = &case["fetch"];
        let reply = if let Some(page) = fetch.get("page") {
            Reply::Page {
                url: case["url"].as_str().unwrap().into(),
                title: page["title"].as_str().unwrap().into(),
                content: page["content"].as_str().unwrap().into(),
            }
        } else {
            let error: fn(String) -> AppError = match fetch["error"].as_str().unwrap() {
                "network" => AppError::Network,
                "invalid_input" => AppError::InvalidInput,
                "invalid_url" => AppError::InvalidUrl,
                "service_not_available" => AppError::ServiceNotAvailable,
                other => panic!("unknown fixture error {other}"),
            };
            Reply::Error(error, fetch["message"].as_str().unwrap().into())
        };
        Self {
            reply,
            fetched: Mutex::new(0),
        }
    }

    fn article() -> Self {
        let article = &fixture()["article"];
        Self::of(&json!({"url": article["url"], "fetch": {"page": article}}))
    }
}

#[async_trait]
impl WebServiceTrait for ScriptedWeb {
    async fn search_web(&self, _input: &WebSearchInput) -> Result<WebSearchOutput> {
        Err(AppError::ServiceNotAvailable(
            "no search in this fixture".into(),
        ))
    }
    async fn fetch_url_content(&self, url: &str) -> Result<FetchUrlContentOutput> {
        self.fetch_reference_content(url).await
    }
    async fn fetch_reference_content(&self, _url: &str) -> Result<FetchUrlContentOutput> {
        *self.fetched.lock().unwrap() += 1;
        match &self.reply {
            Reply::Page {
                url,
                title,
                content,
            } => Ok(FetchUrlContentOutput {
                url: url.clone(),
                title: Some(title.clone()),
                content: content.clone(),
                content_truncated: false,
                word_count: content.split_whitespace().count(),
                fetch_time_ms: 1.0,
                content_type: Some("text/html".into()),
                from_cache: false,
                read_via: Default::default(),
            }),
            Reply::Error(error, message) => Err(error(message.clone())),
        }
    }
    fn validate_url(&self, _url: &str) -> Result<()> {
        Ok(())
    }
}

struct Snapshots {
    answer: ReferenceSnapshot,
    requested: Mutex<Vec<String>>,
}

impl Snapshots {
    fn answering(answer: ReferenceSnapshot) -> Self {
        Self {
            answer,
            requested: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl ReferenceSnapshotPort for Snapshots {
    async fn request(&self, url: &str, _retry: bool) -> Result<ReferenceSnapshot> {
        self.requested.lock().unwrap().push(url.into());
        Ok(self.answer.clone())
    }
    async fn status(&self, _url: &str) -> Result<Option<ReferenceSnapshot>> {
        Ok(None)
    }
}

/// A library holding no documents: every source is ranked by keyword.
struct EmptyLibrary;

#[async_trait]
impl LibraryPassagesPort for EmptyLibrary {
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

/// The claim judge, answering every check with `verdict` and finding no
/// objection when it reviews an approval.
struct Judge {
    verdict: &'static str,
    checks: Mutex<usize>,
}

impl Judge {
    fn says(verdict: &'static str) -> Self {
        Self {
            verdict,
            checks: Mutex::new(0),
        }
    }
}

#[async_trait]
impl LLMPort for Judge {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let replies = fixture_prompts(request)
            .into_iter()
            .map(|_| {
                *self.checks.lock().unwrap() += 1;
                format!(
                    "{}\nReason: The fixture compared the passage.\nSource passage: passage-0",
                    self.verdict
                )
            })
            .collect();
        Ok(fixture_completion(request, replies))
    }
    fn model_name(&self) -> &str {
        "fixture-judge"
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

struct World {
    pool: sqlx::SqlitePool,
    program_id: String,
    ledger: MemoryLedger,
}

async fn world() -> World {
    let pool = tests::pool().await.unwrap();
    let repo = LearningRepository::new(pool.clone());
    let program = tests::fixture();
    repo.create(&program).await.unwrap();
    repo.accept(
        &crate::features::learning::dto::AcceptLearningProgramRequestDto {
            program_id: program.summary.id.clone(),
            expected_revision: 0,
            title: program.summary.title.clone(),
        },
    )
    .await
    .unwrap();
    let program_id = program.summary.id.clone();
    let ledger = MemoryLedger::new(
        &program_id,
        vec![open_claim("water", WATER), open_claim("mercury", MERCURY)],
    );
    World {
        pool,
        program_id,
        ledger,
    }
}

impl World {
    async fn attach(
        &self,
        web: &dyn WebServiceTrait,
        snapshots: &dyn ReferenceSnapshotPort,
        judge: &dyn LLMPort,
        claim: &str,
        evidence: Evidence,
    ) -> Result<super::report::ClaimAttachReportDto> {
        let library = EmptyLibrary;
        attach(
            &AttachDeps {
                pool: &self.pool,
                store: &self.ledger,
                web,
                snapshots,
                documents: &library,
                courses: None,
                llm: judge,
                job_id: None,
            },
            AttachRequest {
                program_id: &self.program_id,
                lesson_id: LESSON,
                claim_id: &ClaimId(claim.into()),
                expected_revision: self.ledger.revision(),
                evidence,
            },
        )
        .await
    }

    /// An attach checked for `others` only, with the stages it reported.
    async fn attach_scoped(
        &self,
        judge: &dyn LLMPort,
        claim: &str,
        evidence: Evidence,
        others: &[ClaimId],
    ) -> (
        super::report::ClaimAttachReportDto,
        Vec<ClaimAttachProgressDto>,
    ) {
        let library = EmptyLibrary;
        let web = ScriptedWeb::article();
        let snapshots = Snapshots::answering(ReferenceSnapshot::Pending);
        let stages = Mutex::new(Vec::new());
        let progress = |stage: ClaimAttachProgressDto| stages.lock().unwrap().push(stage);
        let report = attach_to(
            &AttachDeps {
                pool: &self.pool,
                store: &self.ledger,
                web: &web,
                snapshots: &snapshots,
                documents: &library,
                courses: None,
                llm: judge,
                job_id: None,
            },
            AttachRequest {
                program_id: &self.program_id,
                lesson_id: LESSON,
                claim_id: &ClaimId(claim.into()),
                expected_revision: self.ledger.revision(),
                evidence,
            },
            Others::Only(others),
            &progress,
        )
        .await
        .unwrap();
        (report, stages.into_inner().unwrap())
    }

    async fn source_count(&self) -> usize {
        LearningSourceLibraryRepository::new(self.pool.clone())
            .workspace(&self.program_id)
            .await
            .unwrap()
            .sources
            .len()
    }

    fn pasted(title: &str) -> Evidence {
        Evidence::Text {
            title: title.into(),
            text: PASTED.into(),
        }
    }
}

#[tokio::test]
async fn link_failures_give_the_cards_message_and_save_nothing() {
    let world = world().await;
    let snapshots = Snapshots::answering(ReferenceSnapshot::Pending);
    let judge = Judge::says("supported");
    let before = world.source_count().await;
    for case in fixture()["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let web = ScriptedWeb::of(case);
        let report = world
            .attach(
                &web,
                &snapshots,
                &judge,
                "water",
                Evidence::Link {
                    url: case["url"].as_str().unwrap().into(),
                },
            )
            .await
            .unwrap();
        let failure: AttachFailureDto = serde_json::from_value(case["expect"].clone()).unwrap();
        assert_eq!(report.failure.as_ref(), Some(&failure), "{name}");
        assert_eq!(report.lines, vec![failure.message.clone()], "{name}");
        assert!(
            report.source.is_none() && report.outcome.is_none(),
            "{name}"
        );
        assert_eq!(report.revision, 7, "{name}: nothing is recorded");
    }
    assert_eq!(world.source_count().await, before);
    assert_eq!(world.ledger.revision(), Revision(7));
    assert_eq!(*judge.checks.lock().unwrap(), 0);
    assert!(snapshots.requested.lock().unwrap().is_empty());
}

#[test]
fn an_article_with_a_subscribe_footer_is_not_a_wall() {
    let article = &fixture()["article"];
    let page = FetchUrlContentOutput {
        url: article["url"].as_str().unwrap().into(),
        title: Some(article["title"].as_str().unwrap().into()),
        content: article["content"].as_str().unwrap().into(),
        content_truncated: false,
        word_count: 0,
        fetch_time_ms: 0.0,
        content_type: None,
        from_cache: false,
        read_via: Default::default(),
    };
    assert!(AttachFailureDto::from_wall(&page.url, &page).is_none());
}

#[tokio::test]
async fn pasted_text_that_supports_the_claim_verifies_it_and_checks_the_others() {
    let world = world().await;
    let web = ScriptedWeb::article();
    let snapshots = Snapshots::answering(ReferenceSnapshot::Pending);
    let judge = Judge::says("supported");
    let report = world
        .attach(&web, &snapshots, &judge, "water", World::pasted(NOTES))
        .await
        .unwrap();

    assert_eq!(report.lines[0], format!("Verified against {NOTES}."));
    assert!(matches!(
        report.outcome,
        Some(AttachOutcomeDto::Checked { .. })
    ));
    let source = report.source.clone().unwrap();
    assert!(matches!(source.added_by, SourceAddedByDto::User));
    assert!(!report.already_saved);
    assert_eq!(report.snapshot, SnapshotNoteDto::NotApplicable);
    // The mercury claim has no passage in a text about boiling water.
    assert_eq!(report.others.tried, 1);
    assert_eq!(report.others.no_passage, 1);
    assert_eq!(
        report.lines[1],
        "Also checked 1 other open claim against this source: 0 verified, 1 no passage."
    );
    assert_eq!(report.revision, world.ledger.revision().0);

    let water = world.ledger.claim("water");
    let last = water.events().last().unwrap();
    assert_eq!(last.provenance.actor, Actor::User);
    let ClaimEvent::EvidenceAttached {
        source_version_id,
        result: AttachOutcome::Checked(check),
        ..
    } = &last.event
    else {
        panic!("expected EvidenceAttached, got {:?}", last.event);
    };
    assert_eq!(source_version_id, &source.source_version_id);
    assert!(matches!(check.result, CheckResult::Supported { .. }));
    assert!(check
        .evidence
        .iter()
        .all(|seen| seen.source_version_id == source.source_version_id));
    assert!(matches!(
        water.state(),
        ClaimState::ResolvedByEvidence { .. }
    ));
    // Nothing was recorded on a claim the source has no passage for.
    assert_eq!(world.ledger.claim("mercury").events().len(), 2);
    assert_eq!(*web.fetched.lock().unwrap(), 0);

    let saved = LearningSourceLibraryRepository::new(world.pool.clone())
        .saved_version(&world.program_id, &source.source_version_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved.added_by, SourceAddedBy::User);
}

#[tokio::test]
async fn the_same_text_again_is_checked_against_the_saved_source() {
    let world = world().await;
    let web = ScriptedWeb::article();
    let snapshots = Snapshots::answering(ReferenceSnapshot::Pending);
    let judge = Judge::says("unsupported");
    let first = world
        .attach(&web, &snapshots, &judge, "water", World::pasted(NOTES))
        .await
        .unwrap();
    let count = world.source_count().await;
    let second = world
        .attach(
            &web,
            &snapshots,
            &judge,
            "water",
            World::pasted("Another title"),
        )
        .await
        .unwrap();

    assert!(second.already_saved);
    assert_eq!(
        second.lines[0],
        format!("Already in your references as {NOTES}; checking against it.")
    );
    assert_eq!(
        second.source.unwrap().source_version_id,
        first.source.unwrap().source_version_id
    );
    assert_eq!(world.source_count().await, count);
}

#[tokio::test]
async fn a_contradicting_source_keeps_the_claim_for_the_user_with_its_reason() {
    let world = world().await;
    let web = ScriptedWeb::article();
    let snapshots = Snapshots::answering(ReferenceSnapshot::Pending);
    let judge = Judge::says("contradicted");
    let report = world
        .attach(&web, &snapshots, &judge, "water", World::pasted(NOTES))
        .await
        .unwrap();

    assert!(report.lines[0].starts_with(&format!("{NOTES} says otherwise")));
    let water = world.ledger.claim("water");
    assert!(!water.state().is_resolved());
    assert!(matches!(
        water.state().verdict(),
        Some(VerdictKind::Contradicted)
    ));
}

#[tokio::test]
async fn a_link_is_saved_as_the_users_and_frozen() {
    let world = world().await;
    let web = ScriptedWeb::article();
    let snapshots = Snapshots::answering(ReferenceSnapshot::Ready {
        sha256: "frozen-sha".into(),
    });
    let judge = Judge::says("supported");
    let url = fixture()["article"]["url"].as_str().unwrap().to_owned();
    let report = world
        .attach(
            &web,
            &snapshots,
            &judge,
            "water",
            Evidence::Link { url: url.clone() },
        )
        .await
        .unwrap();

    assert_eq!(report.snapshot, SnapshotNoteDto::Ready);
    assert_eq!(*snapshots.requested.lock().unwrap(), vec![url.clone()]);
    let source = report.source.unwrap();
    let saved = LearningSourceLibraryRepository::new(world.pool.clone())
        .saved_version(&world.program_id, &source.source_version_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved.snapshot_hash.as_deref(), Some("frozen-sha"));
    assert_eq!(saved.added_by, SourceAddedBy::User);

    // The same address again is not fetched again.
    let again = world
        .attach(&web, &snapshots, &judge, "mercury", Evidence::Link { url })
        .await
        .unwrap();
    assert!(again.already_saved);
    assert_eq!(*web.fetched.lock().unwrap(), 1);
}

#[tokio::test]
async fn a_failed_frozen_copy_keeps_the_text_and_says_so() {
    let world = world().await;
    let web = ScriptedWeb::article();
    let snapshots = Snapshots::answering(ReferenceSnapshot::Failed {
        reason: "the page blocked the capture".into(),
    });
    let judge = Judge::says("supported");
    let report = world
        .attach(
            &web,
            &snapshots,
            &judge,
            "water",
            Evidence::Link {
                url: fixture()["article"]["url"].as_str().unwrap().into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        report.lines.last().unwrap(),
        "Frozen copy not captured: the page blocked the capture; the text was saved."
    );
    assert!(report.source.is_some());
}

#[tokio::test]
async fn a_stale_review_is_refused_before_anything_is_saved() {
    let world = world().await;
    let web = ScriptedWeb::article();
    let snapshots = Snapshots::answering(ReferenceSnapshot::Pending);
    let judge = Judge::says("supported");
    let before = world.source_count().await;
    let library = EmptyLibrary;
    let refused = attach(
        &AttachDeps {
            pool: &world.pool,
            store: &world.ledger,
            web: &web,
            snapshots: &snapshots,
            documents: &library,
            courses: None,
            llm: &judge,
            job_id: None,
        },
        AttachRequest {
            program_id: &world.program_id,
            lesson_id: LESSON,
            claim_id: &ClaimId("water".into()),
            expected_revision: Revision(3),
            evidence: World::pasted(NOTES),
        },
    )
    .await;
    assert!(matches!(refused, Err(AppError::InvalidState(_))));
    assert_eq!(world.source_count().await, before);
}

#[tokio::test]
async fn a_claim_scope_checks_that_claim_alone_and_reports_each_stage() {
    let world = world().await;
    let judge = Judge::says("supported");
    let (report, stages) = world
        .attach_scoped(&judge, "water", World::pasted(NOTES), &[])
        .await;

    assert_eq!(report.others.tried, 0);
    assert_eq!(world.ledger.claim("mercury").events().len(), 2);
    let names: Vec<Value> = stages
        .iter()
        .map(|stage| serde_json::to_value(stage).unwrap())
        .collect();
    assert_eq!(
        names,
        [
            json!({ "stage": "saving" }),
            json!({ "stage": "checking", "done": 0, "total": 1, "statement": WATER }),
            json!({ "stage": "done" }),
        ]
    );
}

#[tokio::test]
async fn a_claims_scope_checks_the_chosen_claims_with_a_running_count() {
    let world = world().await;
    let judge = Judge::says("supported");
    let (report, stages) = world
        .attach_scoped(
            &judge,
            "water",
            World::pasted(NOTES),
            &[ClaimId("water".into()), ClaimId("mercury".into())],
        )
        .await;

    // The mercury claim is in scope; the pasted text has no passage for it.
    assert_eq!(report.others.tried, 1);
    assert_eq!(report.others.no_passage, 1);
    let checking: Vec<(u32, u32)> = stages
        .iter()
        .filter_map(|stage| match stage {
            ClaimAttachProgressDto::Checking { done, total, .. } => Some((*done, *total)),
            _ => None,
        })
        .collect();
    assert_eq!(checking, [(0, 2), (1, 2)]);
    assert!(matches!(stages.last(), Some(ClaimAttachProgressDto::Done)));
}
