//! The claim review end to end (§6, §12), over a real database and job
//! store: a claim the checker flags twice waits for the user in a completed
//! job; the user attaches a page that supports it (or confirms it), Resume
//! continues the job, the pass takes the user's resolution without asking the
//! model again, and the lesson publishes. Resume with nothing changed is
//! refused.

use super::claim_ledger_tests::{prepare, waiting, Figures};
use super::*;
use crate::features::learning::curriculum::LearningGenerationJobStatus;
use crate::features::learning::curriculum_repository::LearningCurriculumRepository;
use crate::features::learning::dto::{
    LearningBlockDto, LearningBlockKind, LearningPreparation, PreparedLearningLesson,
};
use crate::features::learning::lessons::claim_reviews::attach::{
    attach, AttachDeps, AttachRequest, Evidence,
};
use crate::features::learning::lessons::claim_reviews::checker_examples::{
    CheckerExamples, Label, LabelSource,
};
use crate::features::learning::lessons::claim_reviews::dto::ClaimActionRequestDto;
use crate::features::learning::lessons::claim_reviews::repository::ClaimReviewRepository;
use crate::features::learning::lessons::claim_reviews::service::{
    ClaimReviewService, NOTHING_CHANGED,
};
use crate::features::learning::repository::LearningRepository;
use async_trait::async_trait;
use lattice_core::application::contracts::web::{
    FetchUrlContentOutput, WebSearchInput, WebSearchOutput,
};
use lattice_core::application::ports::{
    ClaimReviewStore, LibraryDocumentText, LibraryPassageHit, LibraryPassagesPort,
    ReferenceSnapshot, ReferenceSnapshotPort, WebServiceTrait,
};
use lattice_core::domain::learning::claim_review::{ClaimId, ClaimState, LessonReviewState};
use std::sync::atomic::{AtomicUsize, Ordering};

const PAGE_URL: &str = "https://docs.example.org/csv/dictreader-buffering";

/// The figures fixture (a judge that never supports a figure, repairs that
/// trade one figure for another), except that a claim a passage states word
/// for word is supported: the page the user attaches. Counts every claim
/// judgement, so a check the pass reuses is seen not to reach the model.
pub(super) struct Supporting {
    figures: Figures,
    judged: AtomicUsize,
}

impl Supporting {
    pub(super) fn new() -> Self {
        Self {
            figures: Figures::new(),
            judged: AtomicUsize::new(0),
        }
    }

    pub(super) fn judged(&self) -> usize {
        self.judged.load(Ordering::Relaxed)
    }

    async fn respond(&self, prompt: &str) -> Result<String> {
        let judged = prompt
            .strip_prefix("Source passages:\n")
            .and_then(|rest| rest.split_once("\n\nClaim: "));
        if let Some((evidence, rest)) = judged {
            let claim = rest.lines().next().unwrap_or_default();
            self.judged.fetch_add(1, Ordering::Relaxed);
            let stating = evidence
                .split("[passage-")
                .skip(1)
                .find(|passage| passage.contains(claim))
                .and_then(|passage| passage.split(']').next());
            if let Some(id) = stating {
                return Ok(format!(
                    "supported\nReason: The added page states this.\nSource quote: {claim}\nSource passage: passage-{id}"
                ));
            }
        }
        self.figures.respond(prompt).await
    }
}

#[async_trait]
impl LLMPort for Supporting {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let mut replies = Vec::new();
        for prompt in fixture_prompts(request) {
            replies.push(self.respond(&prompt).await?);
        }
        Ok(fixture_completion(request, replies))
    }
    fn model_name(&self) -> &str {
        self.figures.model_name()
    }
    fn count_tokens(&self, text: &str) -> usize {
        self.figures.count_tokens(text)
    }
    fn max_context_tokens(&self) -> usize {
        self.figures.max_context_tokens()
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// The page the user links.
struct Page {
    content: String,
}

#[async_trait]
impl WebServiceTrait for Page {
    async fn search_web(&self, _input: &WebSearchInput) -> Result<WebSearchOutput> {
        Err(AppError::ServiceNotAvailable(
            "no search in this fixture".into(),
        ))
    }
    async fn fetch_url_content(&self, url: &str) -> Result<FetchUrlContentOutput> {
        self.fetch_reference_content(url).await
    }
    async fn fetch_reference_content(&self, url: &str) -> Result<FetchUrlContentOutput> {
        Ok(FetchUrlContentOutput {
            url: url.into(),
            title: Some("DictReader internals".into()),
            content: self.content.clone(),
            content_truncated: false,
            word_count: self.content.split_whitespace().count(),
            fetch_time_ms: 1.0,
            content_type: Some("text/html".into()),
            from_cache: false,
            read_via: Default::default(),
        })
    }
    fn validate_url(&self, _url: &str) -> Result<()> {
        Ok(())
    }
}

/// Every frozen copy is ready at once.
struct Frozen;

#[async_trait]
impl ReferenceSnapshotPort for Frozen {
    async fn request(&self, _url: &str, _retry: bool) -> Result<ReferenceSnapshot> {
        Ok(ReferenceSnapshot::Ready {
            sha256: "frozen".into(),
        })
    }
    async fn status(&self, _url: &str) -> Result<Option<ReferenceSnapshot>> {
        Ok(None)
    }
}

/// A library with no documents.
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

/// A course whose lesson waits for the user's review, its job completed.
pub(super) struct Waiting {
    pub(super) pool: sqlx::SqlitePool,
    pub(super) jobs: LearningCurriculumRepository,
    pub(super) program_id: String,
    pub(super) lesson_id: String,
    pub(super) job_id: String,
}

impl Waiting {
    /// The flagged claim: unsupported, corrected, rewritten, then handed to
    /// the user (§4); the job completes as the worker completes it (§6.1).
    pub(super) async fn new(model: &Supporting) -> Result<Self> {
        let course = Self::course().await?;
        course.hold(model).await?;
        Ok(course)
    }

    /// The course, its lesson not prepared yet.
    pub(super) async fn course() -> Result<Self> {
        let pool = crate::features::learning::tests::pool().await?;
        let (jobs, job_id, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
        let program_id = jobs.job(&job_id).await?.program_id;
        install_source(&pool, &program_id, SOURCE).await?;
        Ok(Self {
            pool,
            jobs,
            program_id,
            lesson_id,
            job_id,
        })
    }

    /// Prepares the lesson until its flagged claim waits for the user, and
    /// completes the job as the worker does.
    pub(super) async fn hold(&self, model: &Supporting) -> Result<()> {
        let error = prepare(
            &self.jobs,
            &self.job_id,
            &self.lesson_id,
            model,
            Some(candidate(BAD).to_string()),
            &self.references().await?,
        )
        .await
        .err()
        .unwrap()
        .to_string();
        assert!(error.starts_with("Needs your review: 1 claim"), "{error}");
        let held = held_for_review(&self.pool, &self.lesson_id, &self.job_id)
            .await?
            .unwrap();
        let (result_ref, message) = review_outcome(&[held]);
        assert!(
            JobStore::new(self.pool.clone())
                .complete(&self.job_id, &result_ref, &message)
                .await?
        );
        Ok(())
    }

    pub(super) async fn references(&self) -> Result<Vec<LearningSourceDto>> {
        LearningRepository::new(self.pool.clone())
            .verification_sources(&self.program_id)
            .await
    }

    pub(super) async fn revision(&self) -> Result<i64> {
        Ok(LearningRepository::new(self.pool.clone())
            .get(&self.program_id)
            .await?
            .summary
            .revision)
    }

    pub(super) fn service(&self) -> ClaimReviewService {
        ClaimReviewService::new(self.pool.clone())
    }

    pub(super) async fn resume(
        &self,
    ) -> Result<crate::features::learning::curriculum::LearningGenerationJob> {
        self.service()
            .resume(
                &self.program_id,
                &self.lesson_id,
                self.revision().await?,
                uuid::Uuid::new_v4().to_string(),
            )
            .await
    }

    /// Runs the resumed attempt's pass, then publishes the lesson as the
    /// worker does once every claim is resolved.
    pub(super) async fn run_and_publish(
        &self,
        model: &Supporting,
        job_id: &str,
    ) -> Result<PreparedLearningLesson> {
        assert!(JobStore::new(self.pool.clone())
            .claim(job_id)
            .await?
            .is_some());
        let (raw, report) = prepare(
            &self.jobs,
            job_id,
            &self.lesson_id,
            model,
            None,
            &self.references().await?,
        )
        .await?;
        assert!(report.issues.is_empty(), "{:?}", report.issues);
        let candidate: Value = serde_json::from_str(&raw)?;
        let mut lesson = PreparedLearningLesson {
            blocks: candidate["blocks"]
                .as_array()
                .unwrap()
                .iter()
                .map(|block| LearningBlockDto {
                    kind: LearningBlockKind::Explanation,
                    title: block["title"].as_str().unwrap().into(),
                    body: block["body"].as_str().unwrap().into(),
                    source_ids: Vec::new(),
                    rubric: Vec::new(),
                })
                .collect(),
            questions: Vec::new(),
            keys: Vec::new(),
            verification: None,
        };
        report.cite_user_evidence(&mut lesson);
        lesson.verification = Some(report.bind(&self.lesson_id, &lesson)?);
        self.jobs.seed_accepted(&self.program_id).await?;
        self.jobs
            .publish_prepared(
                job_id,
                self.revision().await?,
                &[(self.lesson_id.clone(), lesson.clone())],
                &[],
            )
            .await?;
        Ok(lesson)
    }

    pub(super) async fn assert_published(&self, job_id: &str) -> Result<()> {
        let program = LearningRepository::new(self.pool.clone())
            .get(&self.program_id)
            .await?;
        let lesson = program
            .modules
            .iter()
            .flat_map(|module| &module.lessons)
            .find(|lesson| lesson.id == self.lesson_id)
            .unwrap();
        assert_eq!(lesson.preparation, LearningPreparation::Ready);
        assert_eq!(
            self.jobs.job(job_id).await?.status,
            LearningGenerationJobStatus::Completed
        );
        let review = ClaimReviewRepository::new(self.pool.clone())
            .load(&self.lesson_id)
            .await?
            .review
            .unwrap();
        assert_eq!(review.state, LessonReviewState::Ready);
        assert_eq!(review.job_id, job_id);
        Ok(())
    }
}

#[tokio::test]
async fn evidence_the_user_links_resolves_the_claim_and_resume_publishes_the_lesson() -> Result<()>
{
    let model = Supporting::new();
    let course = Waiting::new(&model).await?;

    // The needs-review end state: a completed job naming the lesson, and the
    // review row saying it waits (§6.1).
    let stopped = course.jobs.job(&course.job_id).await?;
    assert_eq!(stopped.status, LearningGenerationJobStatus::Completed);
    let held_ref = format!("needs_review:{}", course.lesson_id);
    assert_eq!(stopped.result_id.as_deref(), Some(held_ref.as_str()));
    let store = ClaimReviewRepository::new(course.pool.clone());
    let ledger = store.load(&course.lesson_id).await?;
    assert_eq!(
        ledger.review.as_ref().unwrap().state,
        LessonReviewState::NeedsReview
    );
    let claim = waiting(&ledger.claims)[0];
    let claim_id: ClaimId = claim.claim_id().clone();

    // The user links a page that states the claim; the check supports it.
    let page = Page {
        content: format!(
            "The csv module reads a file through an iterator. {} This holds in the current release.",
            claim.statement()
        ),
    };
    let report = attach(
        &AttachDeps {
            pool: &course.pool,
            store: &store,
            web: &page,
            snapshots: &Frozen,
            documents: &NoLibrary,
            ranking: None,
            llm: &model,
            job_id: None,
        },
        AttachRequest {
            program_id: &course.program_id,
            lesson_id: &course.lesson_id,
            claim_id: &claim_id,
            expected_revision: ledger.revision,
            evidence: Evidence::Link {
                url: PAGE_URL.into(),
            },
        },
    )
    .await?;
    let version = report.source.unwrap().source_version_id;
    let ledger = store.load(&course.lesson_id).await?;
    let entry = ledger
        .claims
        .iter()
        .find(|entry| *entry.claim_id() == claim_id)
        .unwrap();
    assert!(
        matches!(entry.state(), ClaimState::ResolvedByEvidence { source_version_id, .. } if *source_version_id == version),
        "{:?}",
        entry.state()
    );
    // The card shows what was compared, and the overturned flag is a
    // labelled example: the page was not a reference before (§15).
    let shown = store.shown_passages(&course.lesson_id).await?;
    assert!(shown[&claim_id]
        .iter()
        .any(|passage| passage.source_version_id == version));
    let examples = CheckerExamples::new(course.pool.clone()).list(None).await?;
    assert_eq!(examples.len(), 1);
    assert_eq!(examples[0].label_source, LabelSource::EvidenceAdded);
    assert_eq!(examples[0].label, Label::RetrievalMiss);
    assert!(
        course
            .service()
            .changed_since_stop(&course.lesson_id)
            .await?
    );

    // Resume continues the completed job for the held lesson.
    let next = course.resume().await?;
    assert_eq!(next.status, LearningGenerationJobStatus::Pending);
    assert_eq!(
        next.retry_of_job_id.as_deref(),
        Some(course.job_id.as_str())
    );
    assert_eq!(next.progress_total, 1);

    // The pass takes the attached check as the claim's receipt, and the
    // lesson cites the page on the claim's section (§5.4).
    let judged = model.judged();
    let lesson = course.run_and_publish(&model, &next.id).await?;
    assert_eq!(
        model.judged(),
        judged,
        "the resolved claim is not rechecked"
    );
    assert!(lesson.blocks[0].source_ids.contains(&version));
    course.assert_published(&next.id).await
}

#[tokio::test]
async fn a_confirmed_claim_publishes_on_resume_without_a_check() -> Result<()> {
    let model = Supporting::new();
    let course = Waiting::new(&model).await?;
    let ledger = ClaimReviewRepository::new(course.pool.clone())
        .load(&course.lesson_id)
        .await?;
    let claim = waiting(&ledger.claims)[0].claim_id().0.clone();
    course
        .service()
        .resolve(
            &course.lesson_id,
            &claim,
            ClaimActionRequestDto::Confirm {
                note: None,
                source_version_ids: Vec::new(),
            },
            ledger.revision.0,
        )
        .await?;

    let next = course.resume().await?;
    assert_eq!(
        next.retry_of_job_id.as_deref(),
        Some(course.job_id.as_str())
    );
    let judged = model.judged();
    let lesson = course.run_and_publish(&model, &next.id).await?;
    assert_eq!(model.judged(), judged, "a confirmed claim is not rechecked");
    let report = lesson.verification.unwrap();
    assert!(report.findings.iter().any(|finding| matches!(
        finding.resolution,
        Some(general_knowledge::Resolution::Confirmed { .. })
    )));
    course.assert_published(&next.id).await
}

#[tokio::test]
async fn resume_with_nothing_changed_is_refused_and_starts_nothing() -> Result<()> {
    let model = Supporting::new();
    let course = Waiting::new(&model).await?;
    assert!(
        !course
            .service()
            .changed_since_stop(&course.lesson_id)
            .await?
    );
    let review = course
        .service()
        .review(&course.program_id, &course.lesson_id)
        .await?
        .expect("the held lesson has a review");
    assert!(
        !review.changed_since_stop,
        "the panel keeps Resume disabled"
    );
    assert!(
        course
            .service()
            .review(&course.program_id, "no-pass-yet")
            .await?
            .is_none(),
        "no review before the first pass is null, not an error"
    );

    let refused = course.resume().await.unwrap_err();
    assert!(
        matches!(&refused, AppError::InvalidState(message) if message == NOTHING_CHANGED),
        "{refused}"
    );
    let jobs = course.jobs.jobs(&course.program_id).await?;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].id, course.job_id);
    assert_eq!(jobs[0].status, LearningGenerationJobStatus::Completed);
    Ok(())
}
