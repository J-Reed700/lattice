//! A user's hint (acceptance design §2.10): the request for one research round
//! and one reading of what it found by the course's page. The claim is the
//! user's, so it takes no step; the reading is its check, and the first judged
//! check after the hint answers it.
use super::acceptance_switch_tests::{
    claim, lesson_of, local_check, open, walk, RESEARCHING, SOURCED,
};
use super::claim_ledger_tests::{prepare, section};
use super::class_checks_tests::Classes;
use super::*;
use crate::features::learning::curriculum_repository::LearningCurriculumRepository;
use crate::features::learning::lesson_drafts;
use crate::features::learning::lessons::claim_reviews::dto::ClaimActionRequestDto;
use crate::features::learning::lessons::claim_reviews::repository::ClaimReviewRepository;
use crate::features::learning::lessons::claim_reviews::service::ClaimReviewService;
use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::domain::learning::acceptance::{Outcome, Reading, Step};
use lattice_core::domain::learning::claim_review::{
    CheckResult, ClaimEvent, ClaimLedgerEntry, ClaimState, ReopenCause, VerdictKind,
};

/// The draft's input hash in these fixtures, as `prepare` resumes it.
const INPUT: &str = "ledger-fixture";
/// The first sentence of the page the research fixture serves.
const CLAIM: &str =
    "Random assignment reduces confounding in comparisons between treatment groups.";
/// A sentence on the same subject that the page does not state.
const UNSTATED: &str = "Random assignment guarantees equal outcomes in every trial.";
const HINT: &str = "random assignment and confounding";

/// A model that extracts and sorts as `Classes` does, plans the research round
/// it is told to, selects the first result and reads a page: a claim is
/// stated when the page holds its words.
struct Searching {
    inner: Classes,
    /// What the research planner answers. None, and its prompt is one the
    /// fixture has no answer for: the round reaches nothing.
    plan: Mutex<Option<String>>,
    /// What the reference selector answers.
    selection: Mutex<String>,
    judged: Mutex<Vec<String>>,
}

impl Searching {
    fn new(statements: &[&'static str]) -> Self {
        Self {
            inner: Classes::new(statements),
            plan: Mutex::new(None),
            selection: Mutex::new(research::tests::selection(&[0])),
            judged: Mutex::new(Vec::new()),
        }
    }

    fn plans(&self, plan: Option<Value>) {
        *self.plan.lock().unwrap() = plan.map(|plan| plan.to_string());
    }

    fn selects(&self, selection: Value) {
        *self.selection.lock().unwrap() = selection.to_string();
    }

    /// The claims the strict judge was asked about.
    fn judged(&self) -> Vec<String> {
        self.judged.lock().unwrap().clone()
    }

    async fn respond(&self, prompt: &str) -> Result<String> {
        if prompt.starts_with("Plan focused reference searches") {
            let plan = self.plan.lock().unwrap().clone();
            if let Some(plan) = plan {
                return Ok(plan);
            }
        }
        if prompt.starts_with("Select trustworthy references") {
            return Ok(self.selection.lock().unwrap().clone());
        }
        if prompt.starts_with("Source passages:") {
            let (passages, tail) = prompt.split_once("\n\nClaim: ").expect("a claim to check");
            let claim = tail.split("\n\n").next().unwrap_or_default();
            self.judged.lock().unwrap().push(claim.to_owned());
            return Ok(if passages.contains(claim) {
                format!("supported\nReason: The page states it.\nSource quote: {claim}\nSource passage: passage-0")
            } else {
                "unsupported\nReason: The page does not say so.\nSource quote: none".into()
            });
        }
        self.inner.respond(prompt).await
    }
}

#[async_trait::async_trait]
impl LLMPort for Searching {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let mut replies = Vec::new();
        for prompt in fixture_prompts(request) {
            replies.push(self.respond(&prompt).await?);
        }
        Ok(fixture_completion(request, replies))
    }
    fn model_name(&self) -> &str {
        "searching-fixture"
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

/// A plan of one search, for the gap of the claim the round is for.
fn one_search() -> Value {
    json!({"needs":[{"gap":0,"kind":"web","query":"random assignment confounding official guidance"}]})
}

type Scope<'a> = (&'a LearningCurriculumRepository, &'a str, &'a str);

/// A lesson of one section whose statement waited for research when the user
/// gave a hint on it: the claim is the user's now, and holds the hint. Gives
/// the lesson.
async fn hinted(pool: &sqlx::SqlitePool, scope: Scope<'_>, statement: &str) -> Result<String> {
    hinted_in(pool, scope, lesson_of(&section(statement)), statement).await
}

/// [`hinted`] in `lesson`, whose first section states `statement`.
async fn hinted_in(
    pool: &sqlx::SqlitePool,
    (repo, job, lesson_id): Scope<'_>,
    lesson: Value,
    statement: &str,
) -> Result<String> {
    let body = lesson["blocks"][0]["body"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let raw = lesson.to_string();
    lesson_drafts::run(repo, job, lesson_id, async {
        lesson_drafts::resume(INPUT.into()).await?;
        lesson_drafts::save(&raw).await?;
        let mut ledger = open().await?;
        let id = claim(
            &mut ledger,
            "hinted",
            (statement, statement),
            &body,
            SOURCED,
        )
        .await?;
        walk(&mut ledger, &id, Class::Sourced, &RESEARCHING).await?;
        let failed = CheckResult::Failed {
            kind: VerdictKind::Unsupported,
        };
        let check = local_check(failed, "No source says so.", vec![]);
        ledger.append(&id, ClaimEvent::Checked(check)).await?;
        let revision = ClaimReviewRepository::new(pool.clone())
            .load(lesson_id)
            .await?
            .revision
            .0;
        ClaimReviewService::new(pool.clone())
            .resolve(
                lesson_id,
                &id.0,
                ClaimActionRequestDto::ResearchWithHint { hint: HINT.into() },
                revision,
            )
            .await?;
        Ok(())
    })
    .await?;
    Ok(raw)
}

/// A lesson of one section whose statement was verified on a source the course
/// has since lost, and which the user gave a hint on: the claim is the user's,
/// and its latest check passed. Gives the lesson.
async fn reopened_and_hinted(
    pool: &sqlx::SqlitePool,
    (repo, job, lesson_id): Scope<'_>,
    statement: &str,
) -> Result<String> {
    const LOST: &str = "a-lost-source";
    let body = section(statement);
    let raw = lesson_of(&body).to_string();
    lesson_drafts::run(repo, job, lesson_id, async {
        lesson_drafts::resume(INPUT.into()).await?;
        lesson_drafts::save(&raw).await?;
        let mut ledger = open().await?;
        let id = claim(
            &mut ledger,
            "hinted",
            (statement, statement),
            &body,
            SOURCED,
        )
        .await?;
        let verified = CheckResult::Supported {
            source_version_id: LOST.into(),
            supporting_quote: statement.into(),
        };
        let check = local_check(verified, "The source states it.", vec![]);
        ledger.append(&id, ClaimEvent::Checked(check)).await?;
        let stands = [(Step::Read(Reading::Parts), Outcome::Stands)];
        walk(&mut ledger, &id, Class::Sourced, &stands).await?;
        let reopened = ClaimEvent::Reopened {
            cause: ReopenCause::SourceDeleted(LOST.into()),
        };
        ledger.append(&id, reopened).await?;
        let revision = ClaimReviewRepository::new(pool.clone())
            .load(lesson_id)
            .await?
            .revision
            .0;
        ClaimReviewService::new(pool.clone())
            .resolve(
                lesson_id,
                &id.0,
                ClaimActionRequestDto::ResearchWithHint { hint: HINT.into() },
                revision,
            )
            .await?;
        Ok(())
    })
    .await?;
    Ok(raw)
}

/// One run of the loop over the lesson, with research on the page the fixture
/// serves. The lesson is resumed when `first` is none.
async fn resumed(
    pool: &sqlx::SqlitePool,
    scope: Scope<'_>,
    model: &Searching,
    first: Option<String>,
) -> Result<Result<(String, LessonVerificationReport)>> {
    let (repo, job, lesson_id) = scope;
    let program_id = repo.job(job).await?.program_id;
    Ok(research::run(
        pool.clone(),
        program_id,
        "Research methods".into(),
        Some(research::tests::web(false)),
        prepare(repo, job, lesson_id, model, first, &[source()]),
    )
    .await)
}

/// The lesson's one claim as the store holds it.
async fn held(pool: &sqlx::SqlitePool, lesson_id: &str) -> Result<ClaimLedgerEntry> {
    let ledger = ClaimReviewRepository::new(pool.clone())
        .load(lesson_id)
        .await?;
    let [entry] = ledger.claims.as_slice() else {
        panic!("one claim: {:?}", ledger.claims);
    };
    Ok(entry.clone())
}

/// What was recorded of the claim after the user's hint.
fn after_the_hint(entry: &ClaimLedgerEntry) -> Vec<&'static str> {
    entry
        .events()
        .iter()
        .skip_while(|record| !matches!(record.event, ClaimEvent::UserAction { .. }))
        .skip(1)
        .map(|record| record.event.kind_name())
        .collect()
}

#[tokio::test]
async fn a_hint_the_user_gave_is_searched_and_a_page_that_states_the_claim_verifies_it_with_no_step_recorded(
) -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let scope = (&repo, job.as_str(), lesson_id.as_str());
    let model = Searching::new(&[CLAIM]);
    model.plans(Some(one_search()));
    let raw = hinted(&pool, scope, CLAIM).await?;
    let before = held(&pool, &lesson_id).await?;
    assert!(matches!(before.state(), ClaimState::Queued { .. }));

    resumed(&pool, scope, &model, Some(raw)).await??;

    let entry = held(&pool, &lesson_id).await?;
    assert!(
        matches!(entry.state(), ClaimState::ResolvedByEvidence { .. }),
        "{:?}",
        entry.state()
    );
    // The round was the hint's, and its reading the claim's check: no step.
    let after = after_the_hint(&entry);
    assert_eq!(after.first(), Some(&"researched"), "{after:?}");
    assert!(after.contains(&"checked"), "{after:?}");
    assert!(!after.contains(&"stepped"), "{after:?}");
    assert_eq!(entry.decided(), before.decided());
    // The round counts the reading's passages from the source it saved.
    let found = entry
        .events()
        .iter()
        .find_map(|record| match &record.event {
            ClaimEvent::Researched { passages_found, .. } => Some(*passages_found),
            _ => None,
        });
    assert!(found.is_some_and(|found| found > 0), "{found:?}");
    let judged = model.judged();
    assert!(
        !judged.is_empty() && judged.iter().all(|asked| asked == CLAIM),
        "{judged:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_hint_on_a_claim_whose_latest_check_passed_is_still_searched_and_the_page_that_states_it_verifies_it(
) -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let scope = (&repo, job.as_str(), lesson_id.as_str());
    let model = Searching::new(&[CLAIM]);
    model.plans(Some(one_search()));
    let raw = reopened_and_hinted(&pool, scope, CLAIM).await?;
    assert!(matches!(
        held(&pool, &lesson_id).await?.state(),
        ClaimState::Reopened { .. }
    ));

    let finished = resumed(&pool, scope, &model, Some(raw)).await?;

    let entry = held(&pool, &lesson_id).await?;
    let queries = entry
        .events()
        .iter()
        .find_map(|record| match &record.event {
            ClaimEvent::Researched { queries, .. } => Some(queries.len()),
            _ => None,
        });
    assert!(
        queries.is_some_and(|queries| queries > 0),
        "the round searched for the claim the hint names: {queries:?}"
    );
    assert!(
        matches!(
            entry.state(),
            ClaimState::ResolvedByEvidence { source_version_id, .. } if source_version_id != "a-lost-source"
        ),
        "{:?}",
        entry.state()
    );
    finished?;
    Ok(())
}

#[tokio::test]
async fn a_hint_whose_page_does_not_state_the_claim_leaves_it_the_users_with_the_readings_reason(
) -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let scope = (&repo, job.as_str(), lesson_id.as_str());
    let model = Searching::new(&[UNSTATED]);
    model.plans(Some(one_search()));
    let raw = hinted(&pool, scope, UNSTATED).await?;
    let before = held(&pool, &lesson_id).await?;

    let finished = resumed(&pool, scope, &model, Some(raw)).await?;

    assert!(finished.is_err(), "the claim still waits for the user");
    let entry = held(&pool, &lesson_id).await?;
    let ClaimState::Queued { reason, .. } = entry.state() else {
        panic!("the claim stays the user's: {:?}", entry.state());
    };
    assert!(reason.contains("does not say so"), "{reason}");
    assert_ne!(entry.state(), before.state());
    let after = after_the_hint(&entry);
    assert!(after.contains(&"checked"), "{after:?}");
    assert!(!after.contains(&"stepped"), "{after:?}");
    Ok(())
}

#[tokio::test]
async fn a_hint_whose_round_saved_nothing_is_answered_by_its_research_alone_and_nothing_is_read(
) -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let scope = (&repo, job.as_str(), lesson_id.as_str());
    let model = Searching::new(&[CLAIM]);
    // The search runs, and the selector finds no result worth saving.
    model.plans(Some(one_search()));
    model.selects(json!({"decisions":[]}));
    let raw = hinted(&pool, scope, CLAIM).await?;
    let before = held(&pool, &lesson_id).await?;

    let finished = resumed(&pool, scope, &model, Some(raw)).await?;

    assert!(finished.is_err(), "the claim still waits for the user");
    let entry = held(&pool, &lesson_id).await?;
    assert_eq!(after_the_hint(&entry), ["researched"]);
    assert_eq!(entry.state(), before.state());
    assert!(model.judged().is_empty(), "{:?}", model.judged());
    Ok(())
}

#[tokio::test]
async fn a_hint_whose_round_reached_nothing_is_tried_again_by_the_next_run() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let scope = (&repo, job.as_str(), lesson_id.as_str());
    // The planner has no answer: the round reaches nothing.
    let model = Searching::new(&[CLAIM]);
    let raw = hinted(&pool, scope, CLAIM).await?;
    let before = held(&pool, &lesson_id).await?;

    let first = resumed(&pool, scope, &model, Some(raw)).await?;

    assert!(first.is_err(), "the claim still waits for the user");
    let entry = held(&pool, &lesson_id).await?;
    assert!(
        after_the_hint(&entry).is_empty(),
        "a round that reached nothing records nothing: {:?}",
        after_the_hint(&entry)
    );
    assert_eq!(entry.state(), before.state());

    // The next run's planner answers, and the hint is searched.
    model.plans(Some(one_search()));
    resumed(&pool, scope, &model, None).await??;

    let entry = held(&pool, &lesson_id).await?;
    assert!(
        matches!(entry.state(), ClaimState::ResolvedByEvidence { .. }),
        "{:?}",
        entry.state()
    );
    assert_eq!(after_the_hint(&entry).first(), Some(&"researched"));
    Ok(())
}

#[tokio::test]
async fn a_hint_whose_page_verifies_the_claim_on_a_source_its_section_does_not_cite_leaves_the_published_section_citing_that_source(
) -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let scope = (&repo, job.as_str(), lesson_id.as_str());
    let model = Searching::new(&[CLAIM]);
    model.plans(Some(one_search()));
    // The section cites a page of the course; the hint finds another.
    let mut lesson = lesson_of(&section(CLAIM));
    lesson["blocks"][0]["sourceIndex"] = json!(0);
    lesson["blocks"][0]["quote"] = json!(GOOD);
    let raw = hinted_in(&pool, scope, lesson, CLAIM).await?;

    let (published, _) = resumed(&pool, scope, &model, Some(raw)).await??;

    let published: Value = serde_json::from_str(&published)?;
    let block = &published["blocks"][0];
    assert_eq!(block["quote"], GOOD, "the section's own citation stands");
    // The claim is read against the page the hint found: the sentence it
    // states cites it, as the source the round saved after the course's own.
    let program_id = repo.job(&job).await?.program_id;
    let mut sources = vec![source()];
    sources.extend(
        crate::features::learning::repository::LearningRepository::new(pool.clone())
            .verification_sources(&program_id)
            .await?,
    );
    let references = ReferenceCollection::lexical(&sources)?;
    let citations = section_citations(&published, &references)?;
    let read = citations.of_claim(0, CLAIM);
    assert!(
        read.iter().any(|passage| {
            passage.text.contains(CLAIM)
                && Some(passage.source_id.as_str()) == citations.source_at(1)
        }),
        "{read:?}"
    );
    Ok(())
}
