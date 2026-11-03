//! The claim ledger drives the verify-and-repair loop (§4, §12): a claim gets
//! two machine attempts in its life, whatever evidence later checks bring, a
//! rewrite and then taking its sentence out, or its section when the sentence
//! cannot be taken out; the user's
//! verdicts are never re-litigated and are labelled examples of the checker's
//! accuracy (§15).
use super::*;
use crate::features::learning::lesson_drafts;
use crate::features::learning::lessons::claim_reviews::checker_examples::{
    CheckerExamples, Label, LabelSource,
};
use crate::features::learning::lessons::claim_reviews::dto::ClaimActionRequestDto;
use crate::features::learning::lessons::claim_reviews::repository::{
    ClaimReviewRepository, ShownPassage,
};
use crate::features::learning::lessons::claim_reviews::service::ClaimReviewService;
use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::domain::learning::claim_review::{
    text_sha256, Actor, Check, CheckResult, Checker, ClaimEvent, ClaimId, ClaimLedgerEntry,
    ClaimLocation, ClaimState, EventProvenance, EvidenceFingerprint, EvidenceSeen, LessonReview,
    LessonReviewState, RepairOutcome, Revision, UnitKind, VerdictKind,
};
use std::sync::atomic::{AtomicUsize, Ordering};

/// The draft's input hash in these fixtures.
const INPUT: &str = "ledger-fixture";

/// An unsupported figure no reference gives.
pub(super) fn figure(n: usize) -> String {
    format!("DictReader buffers {}00 rows before parsing.", n + 1)
}

/// The claim a fixture section states; a section of practice alone states
/// none.
fn statement_in(text: &str) -> Option<String> {
    if let Some(start) = text.find("DictReader buffers ") {
        let rest = &text[start..];
        return Some(rest[..=rest.find('.').unwrap()].to_owned());
    }
    [BAD, GOOD]
        .into_iter()
        .find(|statement| text.contains(*statement))
        .map(str::to_owned)
}

/// What the fixture says of a section that states nothing to check.
const NOTHING_TO_CHECK: &str = "The fixture's section holds only practice.";

/// A lesson of one-sentence sections.
/// Practice that states nothing to check, so each section holds one claim.
const PRACTICE: &str = " Try it yourself: open the sample file, read its first row, and compare each header name with the one you expected to see.";

/// A section's body: its one claim, then enough practice to be a section
/// the teaching review accepts (600 characters), as a real lesson's is.
pub(super) fn section(statement: &str) -> String {
    format!("{statement}{}", PRACTICE.repeat(5))
}

fn lesson(statements: &[&str]) -> Value {
    let blocks: Vec<_> = statements
        .iter()
        .enumerate()
        .map(|(index, statement)| {
            json!({"kind":"explanation","title":format!("Section {index}"),"body":section(statement),"rubric":[]})
        })
        .collect();
    json!({"blocks":blocks,"questions":[]})
}

/// Repairs that trade a failed claim for a new unsupported figure (or fix
/// it), a judge that never supports a figure, and counters for what ran.
pub(super) struct Figures {
    inner: Model,
    repair_to_good: bool,
    /// The claim inventory's revision never takes a claim out, though the
    /// section no longer states it.
    revision_keeps_claims: bool,
    fail_repair_at: Option<usize>,
    /// The first asks for a strict judgment, each with the correction the
    /// checker sends after an answer it cannot read, whose answers come back
    /// unreadable.
    unusable_asks: usize,
    asks: AtomicUsize,
    repairs: AtomicUsize,
    judged: Mutex<Vec<String>>,
    extracted: Mutex<Vec<Vec<usize>>>,
}

impl Figures {
    pub(super) fn new() -> Self {
        Self {
            inner: Model::new(),
            repair_to_good: false,
            revision_keeps_claims: false,
            fail_repair_at: None,
            unusable_asks: 0,
            asks: AtomicUsize::new(0),
            repairs: AtomicUsize::new(0),
            judged: Mutex::new(Vec::new()),
            extracted: Mutex::new(Vec::new()),
        }
    }

    pub(super) async fn respond(&self, prompt: &str) -> Result<String> {
        if prompt.starts_with("Extract lesson claims.") {
            let data = self::context(prompt);
            let units = data["units"].as_array().unwrap();
            self.extracted
                .lock()
                .unwrap()
                .push(serde_json::from_value(data["requestedIndices"].clone())?);
            return Ok(inventory::fixture_response(
                units
                    .iter()
                    .map(|unit| {
                        let passage = unit["passages"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|p| p["field"] == "/body")
                            .unwrap();
                        let statement = statement_in(passage["text"].as_str().unwrap());
                        match statement {
                            Some(statement) => json!({"index":unit["index"],"claims":[{"passageId":passage["id"],"statement":statement}],"nonFactualReason":""}),
                            None => json!({"index":unit["index"],"claims":[],"nonFactualReason":NOTHING_TO_CHECK}),
                        }
                    })
                    .collect(),
            )
            .to_string());
        }
        if prompt.starts_with("Update claims after a lesson edit.") {
            let data = self::context(prompt);
            let passages = data["revisedSection"]["passages"].as_array().unwrap();
            let passage = passages
                .iter()
                .find(|p| p["field"] == "/body")
                .unwrap_or(&passages[0]);
            let statement = statement_in(passage["text"].as_str().unwrap());
            let existing = data["existingClaims"].as_array().unwrap();
            let keeps = statement.is_none() && self.revision_keeps_claims;
            let changes: Vec<_> = existing
                .iter()
                .filter(|_| !keeps)
                .map(|claim| match &statement {
                    Some(statement) => json!({"claimId":claim["claimId"],"passageId":passage["id"],"statement":if claim["statement"] == json!(statement) { Value::Null } else { json!(statement) }}),
                    None => json!({"claimId":claim["claimId"],"passageId":Value::Null,"statement":Value::Null}),
                })
                .collect();
            let reason = if statement.is_some() || (keeps && !existing.is_empty()) {
                ""
            } else {
                NOTHING_TO_CHECK
            };
            return Ok(json!({"changes":changes,"nonFactualReason":reason}).to_string());
        }
        if prompt.starts_with("Repair verified lesson defects.") {
            let data = self::context(prompt);
            assert!(
                !prompt.contains("lastAttempt"),
                "a last attempt is cut in code, never sent"
            );
            let repair = self.repairs.fetch_add(1, Ordering::Relaxed);
            assert!(repair < 6, "repairs must end");
            if self.fail_repair_at == Some(repair) {
                return Err(AppError::Network(
                    "fixture server failure during a section repair".into(),
                ));
            }
            let body = data["candidate"]["blocks"][0]["body"].as_str().unwrap();
            let replacement = if self.repair_to_good {
                GOOD.to_owned()
            } else {
                figure(repair)
            };
            let after = body.replace(&statement_in(body).unwrap(), &replacement);
            return Ok(
                json!({"edits":[{"path":"/blocks/0/body","before":body,"after":after}]})
                    .to_string(),
            );
        }
        if prompt.starts_with("Source passages:") {
            let claim = prompt
                .split("\n\nClaim: ")
                .nth(1)
                .unwrap()
                .lines()
                .next()
                .unwrap()
                .to_owned();
            self.judged.lock().unwrap().push(claim.clone());
            if claim.starts_with("DictReader buffers ") {
                return Ok("unsupported\nReason: The reference does not state a buffer size.\nSource passage: none".into());
            }
        }
        self.inner.respond(prompt).await
    }
}

#[async_trait::async_trait]
impl LLMPort for Figures {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let prompts = fixture_prompts(request);
        if prompts
            .iter()
            .any(|prompt| prompt.starts_with("Source passages:"))
        {
            let correcting = prompts
                .iter()
                .any(|prompt| prompt.contains("Your previous response was incomplete"));
            let ask = if correcting {
                self.asks.load(Ordering::Relaxed)
            } else {
                self.asks.fetch_add(1, Ordering::Relaxed) + 1
            };
            if ask <= self.unusable_asks {
                return Ok(CompletionResponse::from_text("no verdict here"));
            }
        }
        let mut replies = Vec::new();
        for prompt in prompts {
            replies.push(self.respond(&prompt).await?);
        }
        Ok(fixture_completion(request, replies))
    }
    fn model_name(&self) -> &str {
        "figures-fixture"
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

/// One preparation pass of the lesson in the job's draft scope.
pub(super) async fn prepare(
    repo: &crate::features::learning::curriculum_repository::LearningCurriculumRepository,
    job: &str,
    lesson_id: &str,
    model: &dyn LLMPort,
    first: Option<String>,
    sources: &[LearningSourceDto],
) -> Result<(String, LessonVerificationReport)> {
    lesson_drafts::run(repo, job, lesson_id, async {
        let saved = lesson_drafts::resume(INPUT.into()).await?;
        let raw = match (first, saved) {
            (Some(raw), _) => {
                lesson_drafts::save(&raw).await?;
                raw
            }
            (None, saved) => saved.expect("a saved draft to resume"),
        };
        verify_and_repair(model, "{}", &json!({}), raw, sources, 3000).await
    })
    .await
}

fn repairs(entry: &ClaimLedgerEntry) -> Vec<(u8, bool, RepairOutcome)> {
    entry
        .events()
        .iter()
        .filter_map(|record| match &record.event {
            ClaimEvent::RepairAttempted {
                attempt,
                escalated,
                outcome,
                ..
            } => Some((*attempt, *escalated, outcome.clone())),
            _ => None,
        })
        .collect()
}

pub(super) fn waiting(entries: &[ClaimLedgerEntry]) -> Vec<&ClaimLedgerEntry> {
    entries
        .iter()
        .filter(|entry| entry.state().awaits_user())
        .collect()
}

#[tokio::test]
async fn a_last_attempt_that_cannot_take_the_sentence_out_ends_the_claim_removed_and_a_lesson_left_without_its_first_explanation_stops(
) -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let sources = [source()];
    let model = Figures::new();
    let error = prepare(
        &repo,
        &job,
        &lesson_id,
        &model,
        Some(candidate(BAD).to_string()),
        &sources,
    )
    .await
    .err()
    .unwrap()
    .to_string();
    assert!(
        error.starts_with("The sources do not support enough of this lesson"),
        "{error}"
    );
    assert!(error.contains("first explanation"), "{error}");
    // A rewrite, then the last attempt, which the code makes: the sentence
    // is the whole of its field, so the section goes with it, and it was the
    // lesson's only explanation.
    assert_eq!(model.repairs.load(Ordering::Relaxed), 1);

    let store = ClaimReviewRepository::new(pool.clone());
    let ledger = store.load(&lesson_id).await?;
    let review = ledger.review.clone().unwrap();
    assert_eq!(review.state, LessonReviewState::Stopped);
    assert_eq!(review.job_id, job);
    let stop = review.stop_reason.unwrap();
    assert_eq!(stop.code, "sources_do_not_support");
    assert!(stop.fix.contains("Add sources"), "{}", stop.fix);
    // The rewrites kept the claim: one card, with both attempts on it, and it
    // is terminal: no claim waits for anyone.
    assert_eq!(ledger.claims.len(), 1, "{:?}", ledger.claims);
    let claim = &ledger.claims[0];
    assert_eq!(claim.statement(), figure(0));
    assert_eq!(claim.location().unit_index, 0);
    assert_eq!(claim.location().unit_kind, UnitKind::Block);
    assert_eq!(claim.location().field, "body");
    assert!(
        matches!(
            claim.state(),
            ClaimState::Removed {
                by: Actor::Machine,
                ..
            }
        ),
        "{:?}",
        claim.state()
    );
    assert!(waiting(&ledger.claims).is_empty());
    assert_eq!(
        repairs(claim),
        vec![
            (1, false, RepairOutcome::Rewritten),
            (2, true, RepairOutcome::Removed)
        ]
    );
    // Research ran once (no web here: nothing found) and is on the record.
    assert_eq!(
        claim
            .events()
            .iter()
            .filter(|record| matches!(
                record.event,
                ClaimEvent::Researched {
                    passages_found: 0,
                    ..
                }
            ))
            .count(),
        1
    );
    let shown = store.shown_passages(&lesson_id).await?;
    let passages = &shown[claim.claim_id()];
    assert!(!passages.is_empty());
    assert!(passages
        .iter()
        .all(|passage| sources[0].excerpt.contains(&passage.text)));
    // The job reads the review row: the lesson is held, and says why.
    let held = held_for_review(&pool, &lesson_id, &job).await?.unwrap();
    assert!(held.stopped);
    assert_eq!(held.claims, 0);
    assert_eq!(
        review_outcome(&[held]),
        (
            format!("needs_review:{lesson_id}"),
            "Stopped: The sources do not support enough of this lesson: its first explanation would have to be taken out.".to_owned()
        )
    );
    Ok(())
}

#[tokio::test]
async fn a_claim_still_failing_after_its_rewrite_is_taken_out_and_the_lesson_publishes(
) -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let sources = [source()];
    let model = Figures::new();
    // A second section keeps a claim: a lesson with none left is no
    // extraction the loop accepts.
    let (raw, report) = prepare(
        &repo,
        &job,
        &lesson_id,
        &model,
        Some(lesson(&[GOOD, BAD]).to_string()),
        &sources,
    )
    .await?;
    // One rewrite request; the last attempt is made by the code.
    assert_eq!(model.repairs.load(Ordering::Relaxed), 1);
    let lesson: Value = serde_json::from_str(&raw)?;
    assert_eq!(lesson["blocks"][0]["body"], section(GOOD));
    assert_eq!(lesson["blocks"][1]["body"], PRACTICE.repeat(5).trim_start());
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert!(held_for_review(&pool, &lesson_id, &job).await?.is_none());
    let ledger = ClaimReviewRepository::new(pool.clone())
        .load(&lesson_id)
        .await?;
    let gone = ledger
        .claims
        .iter()
        .find(|entry| matches!(entry.state(), ClaimState::Removed { .. }))
        .unwrap_or_else(|| {
            let held: Vec<_> = ledger
                .claims
                .iter()
                .map(|entry| (entry.statement(), entry.state()))
                .collect();
            panic!("no claim was removed: {held:?}")
        });
    assert!(matches!(
        gone.state(),
        ClaimState::Removed {
            by: Actor::Machine,
            ..
        }
    ));
    assert_eq!(
        repairs(gone),
        vec![
            (1, false, RepairOutcome::Rewritten),
            (2, true, RepairOutcome::Removed)
        ]
    );
    Ok(())
}

#[tokio::test]
async fn a_claim_cut_on_its_last_attempt_ends_removed_though_the_inventory_revision_keeps_its_statement(
) -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let sources = [source()];
    // The revision after the cut keeps every claim, as the 27B's did.
    let model = Figures {
        revision_keeps_claims: true,
        ..Figures::new()
    };
    let (raw, report) = prepare(
        &repo,
        &job,
        &lesson_id,
        &model,
        Some(lesson(&[GOOD, BAD]).to_string()),
        &sources,
    )
    .await?;
    assert_eq!(model.repairs.load(Ordering::Relaxed), 1);
    let lesson: Value = serde_json::from_str(&raw)?;
    assert_eq!(lesson["blocks"][1]["body"], PRACTICE.repeat(5).trim_start());
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert!(held_for_review(&pool, &lesson_id, &job).await?.is_none());
    let ledger = ClaimReviewRepository::new(pool.clone())
        .load(&lesson_id)
        .await?;
    assert!(waiting(&ledger.claims).is_empty());
    let cut = ledger
        .claims
        .iter()
        .find(|entry| entry.statement() == figure(0))
        .unwrap();
    assert!(
        matches!(
            cut.state(),
            ClaimState::Removed {
                by: Actor::Machine,
                ..
            }
        ),
        "{:?}",
        cut.state()
    );
    assert_eq!(
        repairs(cut),
        vec![
            (1, false, RepairOutcome::Rewritten),
            (2, true, RepairOutcome::Removed)
        ]
    );
    Ok(())
}

/// What the job says of a lesson none of whose statements is left.
const NOTHING_LEFT: &str =
    "The sources do not support enough of this lesson: every statement in it would have to be taken out.";

/// A lesson of one section whose one claim fails its rewrite: the cut that
/// ends it leaves nothing the sources back. Returns the job, the lesson and
/// what the preparation ended with.
async fn prepared_to_nothing(
    pool: &sqlx::SqlitePool,
) -> Result<(
    crate::features::learning::curriculum_repository::LearningCurriculumRepository,
    String,
    String,
    String,
)> {
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(pool).await?;
    let model = Figures::new();
    let error = prepare(
        &repo,
        &job,
        &lesson_id,
        &model,
        Some(lesson(&[BAD]).to_string()),
        &[source()],
    )
    .await
    .err()
    .unwrap()
    .to_string();
    Ok((repo, job, lesson_id, error))
}

#[tokio::test]
async fn a_lesson_whose_every_statement_is_cut_stops_as_one_its_sources_do_not_support(
) -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (_, job, lesson_id, error) = prepared_to_nothing(&pool).await?;

    assert_eq!(error, NOTHING_LEFT);
    let ledger = ClaimReviewRepository::new(pool.clone())
        .load(&lesson_id)
        .await?;
    let review = ledger.review.clone().unwrap();
    assert_eq!(review.state, LessonReviewState::Stopped);
    assert_eq!(review.job_id, job);
    let stop = review.stop_reason.unwrap();
    assert_eq!(stop.code, "sources_do_not_support");
    assert!(stop.fix.contains("Add sources"), "{}", stop.fix);
    // The claim ends removed by the machine, as the claim of a section that
    // goes does, and nothing waits for anyone.
    let [claim] = ledger.claims.as_slice() else {
        panic!("one claim: {:?}", ledger.claims);
    };
    assert!(
        matches!(
            claim.state(),
            ClaimState::Removed {
                by: Actor::Machine,
                ..
            }
        ),
        "{:?}",
        claim.state()
    );
    assert_eq!(
        repairs(claim),
        vec![
            (1, false, RepairOutcome::Rewritten),
            (2, true, RepairOutcome::Removed)
        ]
    );
    assert!(waiting(&ledger.claims).is_empty());
    // The job reads the review row: the lesson is held, stopped, and says why.
    let held = held_for_review(&pool, &lesson_id, &job).await?.unwrap();
    assert!(held.stopped);
    assert_eq!(held.claims, 0);
    assert_eq!(
        review_outcome(&[held]),
        (
            format!("needs_review:{lesson_id}"),
            format!("Stopped: {NOTHING_LEFT}")
        )
    );
    Ok(())
}

#[tokio::test]
async fn the_stop_leaves_the_lesson_free_to_be_prepared_again() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson_id, error) = prepared_to_nothing(&pool).await?;
    assert_eq!(error, NOTHING_LEFT);
    // The worker completes the job as held, not failed.
    let held = held_for_review(&pool, &lesson_id, &job).await?.unwrap();
    let (result_ref, message) = review_outcome(&[held]);
    assert!(
        JobStore::new(pool.clone())
            .complete(&job, &result_ref, &message)
            .await?
    );
    let stopped = repo.job(&job).await?;
    assert_eq!(
        stopped.status,
        crate::features::learning::curriculum::LearningGenerationJobStatus::Completed
    );

    // Preparing the lesson again starts a new job, which takes the lesson
    // from its outline and publishes it once its sources back it.
    let program = crate::features::learning::repository::LearningRepository::new(pool.clone())
        .get(&stopped.program_id)
        .await?;
    let again = repo
        .start_lesson_job(
            &crate::features::learning::plan_dto::StartLearningGenerationJobRequestDto {
                operation_id: uuid::Uuid::new_v4().to_string(),
                program_id: program.summary.id,
                expected_revision: program.summary.revision,
                kind: crate::features::learning::curriculum::LearningGenerationJobKind::LessonPreparation,
                request_json: json!({"lessonIds":[lesson_id]}).to_string(),
                progress_total: 1,
            },
        )
        .await?;
    assert_ne!(again.id, job);
    assert_eq!(
        again.status,
        crate::features::learning::curriculum::LearningGenerationJobStatus::Pending
    );
    assert!(JobStore::new(pool.clone())
        .claim(&again.id)
        .await?
        .is_some());
    let (raw, report) = prepare(
        &repo,
        &again.id,
        &lesson_id,
        &Figures::new(),
        Some(lesson(&[GOOD]).to_string()),
        &[source()],
    )
    .await?;
    let lesson: Value = serde_json::from_str(&raw)?;
    assert_eq!(lesson["blocks"][0]["body"], section(GOOD));
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert!(held_for_review(&pool, &lesson_id, &again.id)
        .await?
        .is_none());
    Ok(())
}

#[tokio::test]
async fn the_calls_ceiling_does_not_rise_while_a_rewrite_fails_its_recheck_and_is_cut() -> Result<()>
{
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let sources = [source()];
    let model = Figures::new();

    let (prepared, trace) = crate::features::learning::lesson_progress::traced(prepare(
        &repo,
        &job,
        &lesson_id,
        &model,
        Some(lesson(&[GOOD, BAD]).to_string()),
        &sources,
    ))
    .await;
    prepared?;

    // The first check, the rewrite and its re-check were calls, and the
    // plan's ceiling, set once from the first pass, never moved up while the
    // later passes ran.
    assert_eq!(model.repairs.load(Ordering::Relaxed), 1);
    assert!(trace.calls >= 3, "{} calls", trace.calls);
    assert!(trace.only_came_down(), "{:?}", trace.ceilings);
    assert!(
        trace.within_the_ceiling(),
        "{} calls, ceilings {:?}",
        trace.calls,
        trace.ceilings
    );
    Ok(())
}

#[tokio::test]
async fn a_statement_whose_judge_never_answers_readably_keeps_its_text_and_attempts_and_the_job_stops(
) -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let sources = [source()];
    let model = Figures {
        unusable_asks: usize::MAX,
        ..Figures::new()
    };
    let draft = lesson(&[GOOD]).to_string();
    let error = prepare(
        &repo,
        &job,
        &lesson_id,
        &model,
        Some(draft.clone()),
        &sources,
    )
    .await
    .err()
    .unwrap()
    .to_string();
    assert!(error.contains("could not be read"), "{error}");
    assert_eq!(model.repairs.load(Ordering::Relaxed), 0, "never rewritten");

    let ledger = ClaimReviewRepository::new(pool.clone())
        .load(&lesson_id)
        .await?;
    let review = ledger.review.clone().unwrap();
    assert_eq!(review.state, LessonReviewState::Stopped);
    let stop = review.stop_reason.unwrap();
    assert_eq!(stop.code, "checker_unreadable");
    assert!(stop.message.contains("figures-fixture"), "{}", stop.message);
    assert!(stop.fix.contains("another verifier model"), "{}", stop.fix);
    // The statement is as it was: unchecked, asked twice, no attempt spent,
    // its text in the draft untouched.
    let [claim] = ledger.claims.as_slice() else {
        panic!("one claim: {:?}", ledger.claims);
    };
    assert_eq!(claim.state(), &ClaimState::Unchecked);
    assert_eq!(claim.unjudged(), 2);
    assert!(repairs(claim).is_empty());
    let saved = lesson_drafts::run(&repo, &job, &lesson_id, async {
        lesson_drafts::resume(INPUT.into()).await
    })
    .await?;
    assert_eq!(saved.as_deref(), Some(draft.as_str()));
    // The job reads the review row: the lesson is held, and says it stopped.
    let held = held_for_review(&pool, &lesson_id, &job).await?.unwrap();
    assert!(held.stopped);
    assert_eq!(
        review_outcome(&[held]).1,
        "Stopped: the checking model's answers could not be read for 1 claim"
    );
    Ok(())
}

#[tokio::test]
async fn a_statement_unreadable_once_and_readable_on_the_recheck_is_judged_normally() -> Result<()>
{
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let sources = [source()];
    let model = Figures {
        unusable_asks: 1,
        ..Figures::new()
    };
    let (raw, report) = prepare(
        &repo,
        &job,
        &lesson_id,
        &model,
        Some(lesson(&[GOOD]).to_string()),
        &sources,
    )
    .await?;

    assert_eq!(model.repairs.load(Ordering::Relaxed), 0);
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    let lesson: Value = serde_json::from_str(&raw)?;
    assert_eq!(lesson["blocks"][0]["body"], section(GOOD));
    let ledger = ClaimReviewRepository::new(pool.clone())
        .load(&lesson_id)
        .await?;
    let [claim] = ledger.claims.as_slice() else {
        panic!("one claim: {:?}", ledger.claims);
    };
    assert!(matches!(
        claim.state(),
        ClaimState::ResolvedByEvidence { .. }
    ));
    assert_eq!(claim.unjudged(), 0);
    // Asked twice: no answer, then the judgment.
    let unanswered: Vec<bool> = claim
        .events()
        .iter()
        .filter_map(|record| match &record.event {
            ClaimEvent::Checked(check) => Some(check.result.is_unjudged()),
            _ => None,
        })
        .collect();
    assert_eq!(unanswered, [true, false]);
    assert!(held_for_review(&pool, &lesson_id, &job).await?.is_none());
    Ok(())
}

#[tokio::test]
async fn a_claim_the_checker_did_not_answer_for_is_asked_again_once_and_then_stops_the_lesson(
) -> Result<()> {
    let model = Model::new();
    let mut ledger = claim_ledger::ClaimLedger::open(&model, "{}").await?;
    let lesson = cell_lesson(CELL);
    let mut unanswered = passage_finding(CELL, ATP, true, CELL_SOURCE);
    unanswered.verdict = ClaimVerdict::Unverified;
    unanswered.supporting_quote = None;
    let report = pass_report(vec![unanswered]);

    ledger.observe(&lesson, &report).await?;
    assert!(matches!(ledger.decide(), claim_ledger::Decision::Recheck));
    ledger.observe(&lesson, &report).await?;
    assert!(matches!(ledger.decide(), claim_ledger::Decision::Stop(1)));

    let id = ledger.finding_claims()[0].clone();
    let entry = ledger.entry(&id).unwrap();
    assert_eq!(entry.state(), &ClaimState::Unchecked);
    assert_eq!(entry.unjudged(), 2);
    assert!(repairs(entry).is_empty());
    Ok(())
}

#[tokio::test]
async fn an_edit_is_applied_on_resume_and_only_its_section_is_extracted_again() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let sources = [source()];
    let model = Figures {
        fail_repair_at: Some(0),
        ..Figures::new()
    };
    let raw = lesson(&[GOOD, BAD]).to_string();
    let error = prepare(&repo, &job, &lesson_id, &model, Some(raw), &sources)
        .await
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("fixture server failure"), "{error}");
    let store = ClaimReviewRepository::new(pool.clone());
    let ledger = store.load(&lesson_id).await?;
    let edited = ledger
        .claims
        .iter()
        .find(|entry| entry.location().unit_index == 1)
        .unwrap();
    let edited = edited.claim_id().clone();
    ClaimReviewService::new(pool.clone())
        .resolve(
            &lesson_id,
            &edited.0,
            ClaimActionRequestDto::Edit {
                before: BAD.into(),
                after: GOOD.into(),
            },
            ledger.revision.0,
        )
        .await?;

    let extractions = model.extracted.lock().unwrap().len();
    let (raw, report) = prepare(&repo, &job, &lesson_id, &model, None, &sources).await?;
    let lesson: Value = serde_json::from_str(&raw)?;
    assert_eq!(lesson["blocks"][1]["body"], section(GOOD));
    assert_eq!(lesson["blocks"][0]["body"], section(GOOD));
    // The untouched section's claims are reused, not extracted again.
    assert!(model.extracted.lock().unwrap()[extractions..]
        .iter()
        .all(|units| !units.contains(&0)));
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    let ledger = store.load(&lesson_id).await?;
    let old = ledger
        .claims
        .iter()
        .find(|entry| *entry.claim_id() == edited)
        .unwrap();
    let ClaimState::Edited {
        successor: Some(successor),
        ..
    } = old.state()
    else {
        panic!("{:?}", old.state());
    };
    let successor = ledger
        .claims
        .iter()
        .find(|entry| entry.claim_id() == successor)
        .unwrap();
    assert!(matches!(
        successor.state(),
        ClaimState::ResolvedByEvidence { .. }
    ));
    let examples = CheckerExamples::new(pool.clone()).list(None).await?;
    assert_eq!(examples.len(), 1);
    assert_eq!(examples[0].label_source, LabelSource::UserEdit);
    assert_eq!(examples[0].label, Label::False);
    Ok(())
}

#[tokio::test]
async fn a_claim_confirmed_while_its_repair_was_interrupted_is_not_sent_on_resume() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let sources = [source()];
    let model = Figures {
        repair_to_good: true,
        fail_repair_at: Some(1),
        ..Figures::new()
    };
    let raw = lesson(&[BAD, BAD]).to_string();
    let error = prepare(&repo, &job, &lesson_id, &model, Some(raw), &sources)
        .await
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("fixture server failure"), "{error}");
    let store = ClaimReviewRepository::new(pool.clone());
    let ledger = store.load(&lesson_id).await?;
    let second = ledger
        .claims
        .iter()
        .find(|entry| entry.location().unit_index == 1)
        .unwrap();
    // A contradicted claim is confirmed only with a note (D3).
    let service = ClaimReviewService::new(pool.clone());
    let confirm = |note: Option<&str>| ClaimActionRequestDto::Confirm {
        note: note.map(str::to_owned),
        source_version_ids: vec![],
    };
    assert!(service
        .resolve(
            &lesson_id,
            &second.claim_id().0,
            confirm(None),
            ledger.revision.0
        )
        .await
        .is_err());
    service
        .resolve(
            &lesson_id,
            &second.claim_id().0,
            confirm(Some("The 2026 maintainer notes say so.")),
            ledger.revision.0,
        )
        .await?;

    let (raw, report) = prepare(&repo, &job, &lesson_id, &model, None, &sources).await?;
    assert_eq!(
        model.repairs.load(Ordering::Relaxed),
        2,
        "the confirmed claim's repair is dropped"
    );
    let lesson: Value = serde_json::from_str(&raw)?;
    assert_eq!(lesson["blocks"][0]["body"], section(GOOD));
    assert_eq!(lesson["blocks"][1]["body"], section(BAD));
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    let confirmed = report.findings.iter().find(|f| f.unit == 1).unwrap();
    assert!(matches!(
        &confirmed.resolution,
        Some(resolution::Resolution::Confirmed { note: Some(_), .. })
    ));
    let ledger = store.load(&lesson_id).await?;
    let first = ledger
        .claims
        .iter()
        .find(|entry| entry.location().unit_index == 0)
        .unwrap();
    assert_eq!(repairs(first), vec![(1, false, RepairOutcome::Rewritten)]);
    assert!(matches!(
        first.state(),
        ClaimState::ResolvedByEvidence { .. }
    ));
    Ok(())
}

#[tokio::test]
async fn human_verdicts_on_flagged_claims_are_labelled_examples_that_export_as_claim_pairs(
) -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let program_id = repo.job(&job).await?.program_id;
    let sentences = [
        "DictReader buffers 100 rows before parsing.",
        "DictReader caches every header.",
        "DictReader strips quotes from field names.",
    ];
    let kinds = [
        VerdictKind::Unsupported,
        VerdictKind::Unsupported,
        VerdictKind::Contradicted,
    ];
    let draft = lesson(&sentences).to_string();
    lesson_drafts::run(&repo, &job, &lesson_id, async {
        lesson_drafts::resume(INPUT.into()).await?;
        lesson_drafts::save(&draft).await
    })
    .await?;
    let store = ClaimReviewRepository::new(pool.clone());
    let mut revision = store
        .set_lesson_review(
            &lesson_id,
            LessonReview {
                program_id: program_id.clone(),
                job_id: job.clone(),
                state: LessonReviewState::NeedsReview,
                draft_sha256: digest(&draft),
                checker_model: "figures-fixture".into(),
                stop_reason: None,
            },
        )
        .await?;
    let machine = || EventProvenance {
        actor: Actor::Machine,
        at: 1,
        job_id: Some(job.clone()),
        app_version: "fixture".into(),
        checker_model: Some("figures-fixture".into()),
    };
    let seen = vec![EvidenceSeen {
        source_version_id: "source-1".into(),
        start_byte: 0,
        end_byte: SOURCE.len(),
        text_sha256: text_sha256(SOURCE),
    }];
    let ids: Vec<ClaimId> = (0..sentences.len())
        .map(|index| ClaimId(format!("claim-{index}")))
        .collect();
    for (index, (sentence, kind)) in sentences.iter().zip(&kinds).enumerate() {
        let id = &ids[index];
        let location = ClaimLocation::locate(index, UnitKind::Block, "body", *sentence, sentence);
        revision = store
            .append(
                &lesson_id,
                id,
                machine(),
                ClaimEvent::Extracted {
                    location,
                    statement: (*sentence).into(),
                },
                revision,
            )
            .await?;
        let check = Check {
            checker: Checker::Local {
                model: "figures-fixture".into(),
            },
            result: CheckResult::Failed { kind: kind.clone() },
            reason: "The reference does not say so.".into(),
            fingerprint: EvidenceFingerprint::of(evidence_policy(), "figures-fixture", &seen),
            evidence: seen.clone(),
            window_tokens: 128_000,
        };
        revision = store
            .append(
                &lesson_id,
                id,
                machine(),
                ClaimEvent::Checked(check),
                revision,
            )
            .await?;
        store
            .show_passages(
                &lesson_id,
                id,
                &[ShownPassage {
                    source_version_id: "source-1".into(),
                    start_byte: 0,
                    end_byte: SOURCE.len(),
                    text: SOURCE.into(),
                    retrieval_kind: "lexical".into(),
                }],
            )
            .await?;
    }
    let service = ClaimReviewService::new(pool.clone());
    let mut revision = revision.0;
    let actions = [
        ClaimActionRequestDto::Confirm {
            note: None,
            source_version_ids: vec![],
        },
        ClaimActionRequestDto::Remove,
        ClaimActionRequestDto::Edit {
            before: "strips quotes from".into(),
            after: "keeps quotes in".into(),
        },
    ];
    for (id, action) in ids.iter().zip(actions) {
        revision = service.resolve(&lesson_id, &id.0, action, revision).await?;
    }
    let examples = CheckerExamples::new(pool.clone());
    let labelled = examples.list(Some(&program_id)).await?;
    // Each verdict is one example.
    assert_eq!(
        labelled
            .iter()
            .map(|example| (
                example.claim_id.as_str(),
                example.label_source,
                example.label
            ))
            .collect::<Vec<_>>(),
        vec![
            ("claim-0", LabelSource::UserConfirm, Label::True),
            ("claim-1", LabelSource::UserRemove, Label::False),
            ("claim-2", LabelSource::UserEdit, Label::False),
        ]
    );
    assert!(labelled.iter().all(|example| example.passages == [SOURCE]
        && example.checker_model == "figures-fixture"
        && example.context_window == 128_000));
    let accuracy = examples.accuracy().await?;
    assert_eq!(accuracy.len(), 1);
    assert_eq!(accuracy[0].labelled, 3);
    assert_eq!(accuracy[0].false_flags, 1);
    assert_eq!(accuracy[0].true_flags, 2);
    assert_eq!(accuracy[0].false_flag_rate, Some(1.0 / 3.0));
    // The export is the claim-pairs-v1 shape `live_claim_pairs_eval` reads.
    let lines = examples.export_lines(None).await?;
    assert_eq!(lines.len(), 3);
    let pairs: Vec<Value> = lines
        .iter()
        .map(|line| serde_json::from_str(line))
        .collect::<std::result::Result<_, _>>()?;
    for pair in &pairs {
        assert!(pair["id"].as_str().is_some());
        assert!(pair["claim"].as_str().is_some());
        assert_eq!(pair["passage"].as_str(), Some(SOURCE));
        assert!(pair["difficulty"].is_array());
    }
    assert_eq!(
        pairs
            .iter()
            .map(|pair| pair["label"].clone())
            .collect::<Vec<_>>(),
        vec![
            json!("supported"),
            json!("unsupported"),
            json!("contradicted")
        ]
    );
    // Undoing a verdict forgets the example it gave.
    service
        .resolve(&lesson_id, &ids[0].0, ClaimActionRequestDto::Undo, revision)
        .await?;
    assert_eq!(examples.list(None).await?.len(), 2);
    let ledger = store.load(&lesson_id).await?;
    assert_eq!(ledger.revision, Revision(revision + 1));
    Ok(())
}

#[test]
fn the_tally_separates_a_claim_awaiting_research_from_a_repair() {
    let machine = || EventProvenance {
        actor: Actor::Machine,
        at: 1,
        job_id: None,
        app_version: "fixture".into(),
        checker_model: Some("figures-fixture".into()),
    };
    let seen = vec![EvidenceSeen {
        source_version_id: "source-1".into(),
        start_byte: 0,
        end_byte: SOURCE.len(),
        text_sha256: text_sha256(SOURCE),
    }];
    let entry = |index: usize, failed: bool, researched: bool| {
        let sentence = figure(index);
        let mut entry = ClaimLedgerEntry::extracted(
            ClaimId(format!("claim-{index}")),
            machine(),
            ClaimLocation::locate(index, UnitKind::Block, "body", &sentence, &sentence),
            sentence.clone(),
        )
        .unwrap();
        if failed {
            let check = Check {
                checker: Checker::Local {
                    model: "figures-fixture".into(),
                },
                result: CheckResult::Failed {
                    kind: VerdictKind::Unsupported,
                },
                reason: "The reference does not say so.".into(),
                fingerprint: EvidenceFingerprint::of(evidence_policy(), "figures-fixture", &seen),
                evidence: seen.clone(),
                window_tokens: 128_000,
            };
            entry.append(machine(), ClaimEvent::Checked(check)).unwrap();
        }
        if researched {
            entry
                .append(
                    machine(),
                    ClaimEvent::Researched {
                        queries: vec![],
                        sources_added: vec![],
                        passages_found: 0,
                        round_ms_per_claim: None,
                    },
                )
                .unwrap();
        }
        entry
    };
    let entries = vec![
        entry(0, false, false),
        entry(1, true, false),
        // Researched on this evidence: a rewrite is next.
        entry(2, true, true),
    ];
    let tally = claim_ledger::claim_tally(&entries);
    assert_eq!(
        (
            tally.unchecked,
            tally.searching,
            tally.repairing,
            tally.waiting
        ),
        (1, 1, 1, 0)
    );
}

/// A biology paragraph under 600 characters, so the extractor is given it as
/// one passage, with three claims and a sentence no claim stands on (claim
/// sources design §18.8).
pub(super) const CELL: &str = "Mitochondria produce most of a cell's ATP through oxidative phosphorylation. They carry their own circular DNA, passed down from the mother. A typical human cell holds exactly twelve mitochondria. Their inner membrane folds into cristae that enlarge its surface.";
const ATP_SENTENCE: &str =
    "Mitochondria produce most of a cell's ATP through oxidative phosphorylation.";
const TWELVE_SENTENCE: &str = "A typical human cell holds exactly twelve mitochondria.";
/// The claims' statements, worded as an extractor words them.
const ATP: &str = "Mitochondria make most of a cell's ATP by oxidative phosphorylation.";
const TWELVE: &str = "A typical human cell contains exactly twelve mitochondria.";
const CELL_SOURCE: &str = "Mitochondria generate most cellular ATP by oxidative phosphorylation; the cristae of their inner membrane enlarge its surface.";

fn cell_lesson(body: &str) -> Value {
    json!({"blocks":[{"kind":"explanation","title":"Mitochondria","body":body,"rubric":[]}],"questions":[]})
}

/// The one passage the extractor is given for a section of `body`, as the
/// inventory cuts it.
pub(super) fn only_passage(body: &str) -> String {
    let input = super::super::inventory::inputs(&[json!({"kind":"explanation","body":body})]);
    let passages = input[0]["passages"].as_array().unwrap();
    assert_eq!(passages.len(), 1, "one passage: {passages:?}");
    passages[0]["text"].as_str().unwrap().to_owned()
}

/// A finding on `body`'s one passage as an older pass's inventory resolved
/// it, the passage being its quote; checked against `source`.
pub(super) fn passage_finding(
    body: &str,
    statement: &str,
    supported: bool,
    source: &str,
) -> Finding {
    Finding {
        unit: 0,
        quote: only_passage(body),
        statement: statement.into(),
        verdict: if supported {
            ClaimVerdict::Supported
        } else {
            ClaimVerdict::Unsupported
        },
        reason: "compared".into(),
        evidence: vec![EvidencePassage {
            source_id: "source-1".into(),
            text: source.into(),
            start_byte: 0,
            end_byte: source.len(),
            retrieval_kind: "lexical".into(),
            score: 1.0,
        }],
        supporting_quote: supported.then(|| source.to_owned()),
        resolution: None,
        trust: None,
        two_pass: Default::default(),
        label: None,
        class: None,
    }
}

/// A pass's report of `findings`, its one source active.
pub(super) fn pass_report(findings: Vec<Finding>) -> LessonVerificationReport {
    LessonVerificationReport {
        policy: String::new(),
        lesson_id: String::new(),
        content_sha256: String::new(),
        checker_model: String::new(),
        checked_at: 0,
        sources: vec![SourceBinding {
            id: "source-1".into(),
            sha256: String::new(),
        }],
        coverage: vec![],
        coverage_audit: vec![],
        findings,
        executions: vec![],
        confirmed_examples: None,
        issues: vec![],
        retrieval_mode: String::new(),
        embedding_model: None,
        page_budget: None,
        unexecuted_languages: vec![],
    }
}

/// The events a pass added to a claim's log.
fn added(entry: &ClaimLedgerEntry, from: usize) -> Vec<&ClaimEvent> {
    entry.events()[from..]
        .iter()
        .map(|record| &record.event)
        .collect()
}

#[tokio::test]
async fn a_location_that_narrows_in_unchanged_text_changes_nothing_but_the_location() -> Result<()>
{
    let model = Model::new();
    let mut ledger = claim_ledger::ClaimLedger::open(&model, "{}").await?;
    let id = ClaimId("atp".into());
    // As older code stored it: located by its whole passage, verified.
    ledger
        .append(
            &id,
            ClaimEvent::Extracted {
                location: ClaimLocation::locate(0, UnitKind::Block, "body", CELL, CELL),
                statement: ATP.into(),
            },
        )
        .await?;
    let seen = vec![EvidenceSeen {
        source_version_id: "source-1".into(),
        start_byte: 0,
        end_byte: CELL_SOURCE.len(),
        text_sha256: text_sha256(CELL_SOURCE),
    }];
    ledger
        .append(
            &id,
            ClaimEvent::Checked(Check {
                checker: Checker::Local {
                    model: model.model_name().into(),
                },
                result: CheckResult::Supported {
                    source_version_id: "source-1".into(),
                    supporting_quote: CELL_SOURCE.into(),
                },
                reason: "compared".into(),
                fingerprint: EvidenceFingerprint::of(evidence_policy(), model.model_name(), &seen),
                evidence: seen,
                window_tokens: model.max_context_tokens(),
            }),
        )
        .await?;
    let before = ledger.entry(&id).unwrap().clone();

    ledger
        .observe(
            &cell_lesson(CELL),
            &pass_report(vec![passage_finding(CELL, ATP, true, CELL_SOURCE)]),
        )
        .await?;

    let entry = ledger.entry(&id).unwrap();
    let added = added(entry, before.events().len());
    let [ClaimEvent::Extracted {
        location,
        statement,
    }] = added.as_slice()
    else {
        panic!("one Extracted and nothing else: {added:?}");
    };
    let at = CELL.find(ATP_SENTENCE).unwrap();
    assert_eq!(location.quote, ATP_SENTENCE);
    assert_eq!(location.span, Some((at, at + ATP_SENTENCE.len())));
    assert_eq!(statement, ATP);
    assert!(matches!(
        entry.state(),
        ClaimState::ResolvedByEvidence { .. }
    ));
    assert_eq!(entry.state(), before.state());
    assert_eq!(entry.basis(), before.basis());
    Ok(())
}

#[tokio::test]
async fn a_verified_claim_is_not_rejudged_on_reselected_evidence_while_its_sentence_stands(
) -> Result<()> {
    let model = Model::new();
    let mut ledger = claim_ledger::ClaimLedger::open(&model, "{}").await?;
    let lesson = cell_lesson(CELL);
    ledger
        .observe(
            &lesson,
            &pass_report(vec![passage_finding(CELL, ATP, true, CELL_SOURCE)]),
        )
        .await?;
    let id = ledger.finding_claims()[0].clone();
    let verified = ledger.entry(&id).unwrap().clone();
    assert_eq!(verified.location().quote, ATP_SENTENCE);
    assert!(matches!(
        verified.state(),
        ClaimState::ResolvedByEvidence { .. }
    ));

    // Re-extraction rewords the statement over the same sentence, and the
    // check, on other passages, fails: nothing new about the claim.
    let reworded = "Most of a cell's ATP comes from oxidative phosphorylation in mitochondria.";
    let other = "Glycolysis in the cytoplasm also yields a little ATP.";
    ledger
        .observe(
            &lesson,
            &pass_report(vec![passage_finding(CELL, reworded, false, other)]),
        )
        .await?;
    let entry = ledger.entry(&id).unwrap();
    let events = added(entry, verified.events().len());
    assert!(
        matches!(events.as_slice(), [ClaimEvent::Extracted { statement, .. }] if statement == reworded),
        "{events:?}"
    );
    assert_eq!(entry.state(), verified.state());
    assert_eq!(entry.basis(), verified.basis());

    // The sentence changed: the check applies.
    let changed = CELL.replace("produce most", "produce nearly all");
    let from = entry.events().len();
    ledger
        .observe(
            &cell_lesson(&changed),
            &pass_report(vec![passage_finding(&changed, ATP, false, other)]),
        )
        .await?;
    let entry = ledger.entry(&id).unwrap();
    assert!(added(entry, from)
        .iter()
        .any(|event| matches!(event, ClaimEvent::Checked(_))));
    assert!(matches!(entry.state(), ClaimState::Open { .. }));
    Ok(())
}

#[tokio::test]
async fn a_verified_claim_whose_citation_was_repaired_is_decided_by_the_next_pass() -> Result<()> {
    let model = Model::new();
    let mut ledger = claim_ledger::ClaimLedger::open(&model, "{}").await?;
    let lesson = cell_lesson(CELL);
    ledger
        .observe(
            &lesson,
            &pass_report(vec![passage_finding(CELL, ATP, true, CELL_SOURCE)]),
        )
        .await?;
    let id = ledger.finding_claims()[0].clone();
    // Its section now cites the passage found for it elsewhere (§5.1).
    let cited = "Glycolysis in the cytoplasm also yields a little ATP.";
    ledger
        .append(
            &id,
            ClaimEvent::CitationRepaired {
                cited: vec!["source-1".into()],
                source_version_id: "source-1".into(),
                quote: cited.into(),
                added_source: false,
            },
        )
        .await?;
    let from = ledger.entry(&id).unwrap().events().len();

    // The next pass reads the new citation, which does not support it.
    ledger
        .observe(
            &lesson,
            &pass_report(vec![passage_finding(CELL, ATP, false, cited)]),
        )
        .await?;

    let entry = ledger.entry(&id).unwrap();
    let events = added(entry, from);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ClaimEvent::Checked(_))),
        "{events:?}"
    );
    assert!(
        matches!(entry.state(), ClaimState::Open { .. }),
        "{:?}",
        entry.state()
    );
    assert!(matches!(ledger.decide(), claim_ledger::Decision::Repair(_)));
    Ok(())
}

#[tokio::test]
async fn a_lesson_is_not_published_with_a_claim_its_last_pass_could_not_support() -> Result<()> {
    let model = Model::new();
    let mut ledger = claim_ledger::ClaimLedger::open(&model, "{}").await?;
    let lesson = cell_lesson(CELL);
    ledger
        .observe(
            &lesson,
            &pass_report(vec![passage_finding(CELL, ATP, true, CELL_SOURCE)]),
        )
        .await?;
    let id = ledger.finding_claims()[0].clone();
    // The last pass compares other passages and cannot support it. While
    // the lesson is prepared that leaves the verified claim alone (claim
    // sources design §18.4 ii), but the report it would publish says the
    // claim is unsupported.
    let other = "Glycolysis in the cytoplasm also yields a little ATP.";
    let last = pass_report(vec![passage_finding(CELL, ATP, false, other)]);
    ledger.observe(&lesson, &last).await?;
    assert!(matches!(
        ledger.entry(&id).unwrap().state(),
        ClaimState::ResolvedByEvidence { .. }
    ));

    assert!(
        !matches!(ledger.decide(), claim_ledger::Decision::Publish),
        "the report would carry {:?}",
        ledger.bind(last.clone()).issues
    );
    Ok(())
}

#[tokio::test]
async fn a_failed_claim_sharing_its_sentence_with_a_verified_one_is_rewritten_and_both_are_checked_again(
) -> Result<()> {
    let model = Model::new();
    let mut ledger = claim_ledger::ClaimLedger::open(&model, "{}").await?;
    let overstated = "Mitochondria produce all of a cell's ATP.";
    let before = cell_lesson(CELL);
    let report = pass_report(vec![
        passage_finding(CELL, ATP, true, CELL_SOURCE),
        passage_finding(CELL, overstated, false, CELL_SOURCE),
    ]);
    ledger.observe(&before, &report).await?;
    let ids = ledger.finding_claims().to_vec();
    let (verified, shared) = (ids[0].clone(), ids[1].clone());
    assert_eq!(ledger.cotenants(&shared), std::slice::from_ref(&verified));
    assert_eq!(ledger.cotenants(&verified), std::slice::from_ref(&shared));

    // The rewrite is told what else the sentence says.
    let references = ReferenceCollection::lexical(&[])?;
    let context = repair_context(
        "{}",
        &before,
        &report,
        &[],
        &ledger,
        std::slice::from_ref(&shared),
        &references,
    )?;
    assert_eq!(context["failedClaims"][0]["alsoStates"], json!([ATP]));
    assert!(context["failedClaims"][0].get("lastAttempt").is_none());

    // The repair is the failed claim's; the claim that passed rides on it.
    ledger
        .begin_repairs(&before, std::slice::from_ref(&shared))
        .await?;
    assert_eq!(ledger.pending.repairs.len(), 2);
    let rider = ledger
        .pending
        .repairs
        .iter()
        .find(|record| record.claim_id == verified)
        .unwrap();
    assert_eq!(rider.rides_on.as_ref(), Some(&shared));

    // The sentence is rewritten, and both statements stand on the new one.
    let rewritten =
        "Mitochondria make most, though not all, of a cell's ATP by oxidative phosphorylation.";
    let after = CELL.replace(ATP_SENTENCE, rewritten);
    let both = "Mitochondria make most, though not all, of a cell's ATP.";
    ledger
        .observe(
            &cell_lesson(&after),
            &pass_report(vec![
                passage_finding(&after, ATP, false, CELL_SOURCE),
                passage_finding(&after, both, false, CELL_SOURCE),
            ]),
        )
        .await?;

    // Each is checked once on the new text, and takes the attempts the
    // rewrite left: the next is their last.
    for id in [&verified, &shared] {
        let entry = ledger.entry(id).unwrap();
        assert_eq!(entry.location().quote, rewritten);
        assert!(
            matches!(
                entry.state(),
                ClaimState::Open {
                    kind: VerdictKind::Unsupported,
                    machine_attempts_left: 1,
                    ..
                }
            ),
            "{:?}",
            entry.state()
        );
        assert!(entry.next_repair_escalates());
    }
    assert!(ledger
        .entry(&verified)
        .unwrap()
        .events()
        .iter()
        .any(|record| matches!(
            &record.event,
            ClaimEvent::RepairAttempted {
                attempt: 1,
                escalated: false,
                outcome: RepairOutcome::Rewritten,
                ..
            }
        )));
    assert!(ledger.pending.repairs.is_empty());
    assert_eq!(*model.judge_calls.lock().unwrap(), 0);
    Ok(())
}

#[tokio::test]
async fn a_claim_that_passed_on_a_sentence_cut_for_another_is_removed_by_the_machine() -> Result<()>
{
    let model = Model::new();
    let mut ledger = claim_ledger::ClaimLedger::open(&model, "{}").await?;
    let overstated = "Mitochondria produce all of a cell's ATP.";
    let before = cell_lesson(CELL);
    ledger
        .observe(
            &before,
            &pass_report(vec![
                passage_finding(CELL, ATP, true, CELL_SOURCE),
                passage_finding(CELL, overstated, false, CELL_SOURCE),
            ]),
        )
        .await?;
    let ids = ledger.finding_claims().to_vec();
    let (verified, shared) = (ids[0].clone(), ids[1].clone());
    // The failed claim has had its rewrite.
    ledger
        .append(
            &shared,
            ClaimEvent::RepairAttempted {
                attempt: 1,
                escalated: false,
                before: ATP_SENTENCE.into(),
                after: None,
                outcome: RepairOutcome::Rejected {
                    reason: "unusable".into(),
                },
            },
        )
        .await?;
    let claim_ledger::Decision::Repair(targets) = ledger.decide() else {
        panic!("the failed claim is repaired");
    };
    assert_eq!(targets, std::slice::from_ref(&shared));
    assert!(ledger.entry(&shared).unwrap().next_repair_escalates());

    // Its last attempt cuts the sentence: no repair record rides on it, and
    // the claim that passed goes with the sentence.
    ledger.begin_repairs(&before, &targets).await?;
    assert_eq!(ledger.pending.repairs.len(), 1);
    let cut = CELL.replace(&format!("{ATP_SENTENCE} "), "");
    ledger
        .observe(&cell_lesson(&cut), &pass_report(vec![]))
        .await?;

    for id in [&shared, &verified] {
        assert!(
            matches!(
                ledger.entry(id).unwrap().state(),
                ClaimState::Removed {
                    by: Actor::Machine,
                    ..
                }
            ),
            "{:?}",
            ledger.entry(id).unwrap().state()
        );
    }
    assert_eq!(
        repairs(ledger.entry(&shared).unwrap()),
        vec![
            (
                1,
                false,
                RepairOutcome::Rejected {
                    reason: "unusable".into()
                }
            ),
            (2, true, RepairOutcome::Removed)
        ]
    );
    assert!(repairs(ledger.entry(&verified).unwrap()).is_empty());
    Ok(())
}

#[tokio::test]
async fn remove_and_edit_act_on_the_located_sentence() -> Result<()> {
    use lattice_core::domain::learning::claim_review::UserAction;
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let program_id = repo.job(&job).await?.program_id;
    // The lesson writes the ATP sentence with markdown and a citation.
    let raw_atp =
        "Mitochondria produce most of a cell's **ATP** through oxidative phosphorylation [1].";
    let body = CELL.replace(ATP_SENTENCE, raw_atp);
    let draft = cell_lesson(&body).to_string();
    lesson_drafts::run(&repo, &job, &lesson_id, async {
        lesson_drafts::resume(INPUT.into()).await?;
        lesson_drafts::save(&draft).await
    })
    .await?;
    let store = ClaimReviewRepository::new(pool.clone());
    let mut revision = store
        .set_lesson_review(
            &lesson_id,
            LessonReview {
                program_id,
                job_id: job.clone(),
                state: LessonReviewState::NeedsReview,
                draft_sha256: digest(&draft),
                checker_model: "figures-fixture".into(),
                stop_reason: None,
            },
        )
        .await?;
    let machine = || EventProvenance {
        actor: Actor::Machine,
        at: 1,
        job_id: Some(job.clone()),
        app_version: "fixture".into(),
        checker_model: Some("figures-fixture".into()),
    };
    let seen = vec![EvidenceSeen {
        source_version_id: "source-1".into(),
        start_byte: 0,
        end_byte: CELL_SOURCE.len(),
        text_sha256: text_sha256(CELL_SOURCE),
    }];
    // Both claims as older code stored them: located by the whole passage.
    for (id, statement) in [("twelve", TWELVE), ("atp", ATP)] {
        let id = ClaimId(id.into());
        revision = store
            .append(
                &lesson_id,
                &id,
                machine(),
                ClaimEvent::Extracted {
                    location: ClaimLocation::locate(0, UnitKind::Block, "body", &body, &body),
                    statement: statement.into(),
                },
                revision,
            )
            .await?;
        let check = Check {
            checker: Checker::Local {
                model: "figures-fixture".into(),
            },
            result: CheckResult::Failed {
                kind: VerdictKind::Unsupported,
            },
            reason: "The reference does not say so.".into(),
            fingerprint: EvidenceFingerprint::of(evidence_policy(), "figures-fixture", &seen),
            evidence: seen.clone(),
            window_tokens: 128_000,
        };
        revision = store
            .append(
                &lesson_id,
                &id,
                machine(),
                ClaimEvent::Checked(check),
                revision,
            )
            .await?;
    }
    let service = ClaimReviewService::new(pool.clone());
    let revision = service
        .resolve(
            &lesson_id,
            "twelve",
            ClaimActionRequestDto::Remove,
            revision.0,
        )
        .await?;
    // The card sends the sentence as it shows it: plain text.
    let after = "Mitochondria make most of a cell's ATP [1].";
    service
        .resolve(
            &lesson_id,
            "atp",
            ClaimActionRequestDto::Edit {
                before: ATP_SENTENCE.into(),
                after: after.into(),
            },
            revision,
        )
        .await?;
    let ledger = store.load(&lesson_id).await?;
    let atp = ledger
        .claims
        .iter()
        .find(|entry| entry.claim_id().0 == "atp")
        .unwrap();
    assert!(atp.events().iter().any(|record| matches!(
        &record.event,
        ClaimEvent::UserAction {
            action: UserAction::Edit { before, .. },
            ..
        } if before == raw_atp
    )));

    // On resume the draft loses one sentence and has the other replaced; the
    // rest of the paragraph is as it was.
    let candidate = lesson_drafts::run(&repo, &job, &lesson_id, async {
        let mut ledger = claim_ledger::ClaimLedger::open(&Model::new(), &draft).await?;
        let mut candidate: Value = serde_json::from_str(&draft)?;
        super::super::review_pass::apply_user_changes(&mut ledger, &mut candidate).await?;
        Ok(candidate)
    })
    .await?;
    assert_eq!(
        candidate["blocks"][0]["body"],
        body.replace(&format!("{TWELVE_SENTENCE} "), "")
            .replace(raw_atp, after)
    );
    Ok(())
}

// ── The calls the checks make, against the ceiling the panel shows (claim
// classes design §3.2, §3.11): the code makes only the calls the plan counts,
// and the plan counts every call the code makes ───────────────────────────

/// The figures fixture, counting the requests of the kinds the call plan
/// counts: sorting, claim checks, knowledge samples, rewrites and flow
/// repairs. Every other request (extraction, coverage, the teaching review) is
/// outside it.
struct Counting {
    inner: Figures,
    planned: AtomicUsize,
    /// Of those, the claim checks that read a statement part by part against
    /// the passages its sentence cites, and the strict ones that read a page.
    part_wise: AtomicUsize,
    strict: AtomicUsize,
}

impl Counting {
    fn new() -> Self {
        Self {
            inner: Figures::new(),
            planned: AtomicUsize::new(0),
            part_wise: AtomicUsize::new(0),
            strict: AtomicUsize::new(0),
        }
    }

    fn made(&self) -> u32 {
        u32::try_from(self.planned.load(Ordering::Relaxed)).unwrap_or(u32::MAX)
    }
}

/// Whether a prompt is a call of a kind the plan counts.
fn planned_kind(prompt: &str) -> bool {
    prompt.starts_with("You sort statements")
        || prompt.starts_with("Source passages:")
        || prompt.starts_with("You decide whether one statement is standard knowledge")
        || prompt.starts_with("Repair verified lesson defects.")
        || prompt.contains("\"verifiedSentences\"")
}

#[async_trait::async_trait]
impl LLMPort for Counting {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let prompts = fixture_prompts(request);
        if prompts.iter().any(|prompt| planned_kind(prompt)) {
            self.planned.fetch_add(1, Ordering::Relaxed);
            let checking = prompts.iter().any(|prompt| {
                prompt.starts_with("Source passages:")
                    && !prompt.contains("\n\nExplanatory statement: ")
            });
            let (system, _) = fixture_messages(request);
            if checking
                && system == lattice_core::application::services::claim_verification::BRIDGE_SYSTEM
            {
                self.part_wise.fetch_add(1, Ordering::Relaxed);
            } else if checking {
                self.strict.fetch_add(1, Ordering::Relaxed);
            }
        }
        self.inner.complete(request).await
    }
    fn model_name(&self) -> &str {
        self.inner.model_name()
    }
    fn count_tokens(&self, text: &str) -> usize {
        self.inner.count_tokens(text)
    }
    fn max_context_tokens(&self) -> usize {
        self.inner.max_context_tokens()
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// A lesson of `statements`, one to a section; each section cites the
/// source's passage unless its index is in `uncited`.
fn cited_lesson(statements: &[&str], uncited: &[usize]) -> Value {
    let blocks: Vec<Value> = statements
        .iter()
        .enumerate()
        .map(|(index, statement)| {
            let mut block = json!({
                "kind": "explanation",
                "title": format!("Section {index}"),
                "body": section(statement),
                "rubric": [],
            });
            if !uncited.contains(&index) {
                block["quote"] = json!(GOOD);
                block["sourceIndex"] = json!(0);
            }
            block
        })
        .collect();
    json!({"blocks": blocks, "questions": []})
}

/// A whole preparation of `cited_lesson(statements, uncited)`: the requests it
/// made and the plan's ceilings across it.
async fn prepared_counting(
    statements: &[&str],
    uncited: &[usize],
    sources: &[LearningSourceDto],
) -> Result<(Counting, crate::features::learning::lesson_progress::Trace)> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let model = Counting::new();
    let (prepared, trace) = crate::features::learning::lesson_progress::traced(prepare(
        &repo,
        &job,
        &lesson_id,
        &model,
        Some(cited_lesson(statements, uncited).to_string()),
        sources,
    ))
    .await;
    prepared?;
    Ok((model, trace))
}

/// A second page of the course on the subject of the lesson's statements:
/// what a page read beside a section's citation is read from, since a page
/// never shows again what the citation does.
fn beside_the_citation() -> LearningSourceDto {
    LearningSourceDto {
        id: uuid::Uuid::new_v4().to_string(),
        title: "CSV header handling".into(),
        url: Some("https://docs.python.org/3.14/library/csv.html#csv.DictReader".into()),
        excerpt: format!(
            "{GOOD} A header row that holds spaces keeps them in the field names DictReader gives each row."
        ),
        acquired_at: 0,
    }
}

/// The requests made are at most the ceiling the run ended on, and the
/// ceiling never rose along the way.
fn within_the_plan(model: &Counting, trace: &crate::features::learning::lesson_progress::Trace) {
    let ceiling = trace.ceilings.last().copied().unwrap();
    assert!(trace.only_came_down(), "{:?}", trace.ceilings);
    assert!(
        model.made() <= ceiling,
        "{} requests, ceilings {:?}",
        model.made(),
        trace.ceilings
    );
}

#[tokio::test]
async fn statements_their_citations_support_cost_one_call_each_and_no_page_is_read() -> Result<()> {
    let (model, trace) = prepared_counting(&[GOOD, GOOD, GOOD], &[], &[source()]).await?;

    within_the_plan(&model, &trace);
    assert_eq!(model.part_wise.load(Ordering::Relaxed), 3);
    assert_eq!(
        model.strict.load(Ordering::Relaxed),
        0,
        "a page beside what the citation supports is a call the plan does not count"
    );
    Ok(())
}

#[tokio::test]
async fn a_cited_statement_that_fails_reads_a_page_once_and_the_run_stays_within_the_plan(
) -> Result<()> {
    let (model, trace) =
        prepared_counting(&[GOOD, GOOD, BAD], &[], &[source(), beside_the_citation()]).await?;

    within_the_plan(&model, &trace);
    // The failing statement's page is its re-check against what research
    // found: once, and not for the two its citations support.
    assert_eq!(
        model.strict.load(Ordering::Relaxed),
        1,
        "{} part by part, {} requests the plan counts, ceilings {:?}",
        model.part_wise.load(Ordering::Relaxed),
        model.made(),
        trace.ceilings
    );
    Ok(())
}

#[tokio::test]
async fn a_statement_that_cites_nothing_is_checked_against_a_page_alone() -> Result<()> {
    let (model, trace) = prepared_counting(&[GOOD, GOOD, GOOD], &[2], &[source()]).await?;

    within_the_plan(&model, &trace);
    assert_eq!(model.part_wise.load(Ordering::Relaxed), 2);
    assert_eq!(model.strict.load(Ordering::Relaxed), 1);
    Ok(())
}
