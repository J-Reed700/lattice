//! The claim ledger drives the verify-and-repair loop (§4, §12): a claim gets
//! two machine attempts in its life, whatever evidence later checks bring, a
//! rewrite and then taking its sentence out, or its section when the sentence
//! cannot be taken out; the user's
//! verdicts are never re-litigated and are labelled examples of the checker's
//! accuracy (§15).
use super::acceptance_switch_tests::{claim, local_check, open, stood, walk, REWRITING, SOURCED};
use super::class_checks_tests::Classes;
use super::*;
use crate::dto::{
    LearningBlockDto, LearningBlockKind, LearningPreparation, PreparedLearningLesson,
};
use crate::lesson_drafts;
use crate::lesson_evidence::LearningLessonEvidenceDto;
use crate::lessons::claim_reviews::checker_examples::{CheckerExamples, Label, LabelSource};
use crate::lessons::claim_reviews::dto::ClaimActionRequestDto;
use crate::lessons::claim_reviews::repository::{ClaimReviewRepository, ShownPassage};
use crate::lessons::claim_reviews::service::ClaimReviewService;
use crate::repository::LearningRepository;
use lattice_core::application::ports::claim_review::ClaimReviewStore;
use lattice_core::application::services::model_contract::CORRECTION_LEAD;
use lattice_core::domain::learning::acceptance::{Goal, Outcome, Reading, Step, Then};
use lattice_core::domain::learning::claim_review::{
    text_sha256, Actor, Check, CheckResult, Checker, ClaimEvent, ClaimId, ClaimLedgerEntry,
    ClaimLocation, ClaimState, EventProvenance, EvidenceFingerprint, EvidenceSeen, LessonReview,
    LessonReviewState, RepairOutcome, Revision, UnitKind, UnverifiedReason, VerdictKind,
};
use lattice_core::shared::persistence::Database;
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
    [BAD, GOOD, FIXED]
        .into_iter()
        .find(|statement| text.contains(*statement))
        .map(str::to_owned)
}

/// What the fixture says of a section that states nothing to check.
const NOTHING_TO_CHECK: &str = "The fixture's section holds only practice.";

/// A lesson of one-sentence sections.
/// Practice that states nothing to check, so each section holds one claim.
const PRACTICE: &str = " Try it yourself: open the sample file, read its first row, and compare each header name with the one you expected to see.";

/// A section's body: its one claim, then enough practice to be as long as a
/// real lesson's sections are.
pub(super) fn section(statement: &str) -> String {
    format!("{statement}{}", PRACTICE.repeat(5))
}

pub(super) fn lesson(statements: &[&str]) -> Value {
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
    fail_repair_at: Option<usize>,
    /// The first asks for a strict judgment, each with the correction the
    /// checker sends after an answer it cannot read, whose answers come back
    /// unreadable.
    unusable_asks: usize,
    /// The judge calls a figure contradicted, not just unsupported.
    contradicted: bool,
    /// What the research planner answers; None, and it gives no usable plan.
    plan: Option<String>,
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
            fail_repair_at: None,
            unusable_asks: 0,
            contradicted: false,
            plan: None,
            asks: AtomicUsize::new(0),
            repairs: AtomicUsize::new(0),
            judged: Mutex::new(Vec::new()),
            extracted: Mutex::new(Vec::new()),
        }
    }

    /// A judge that calls a figure contradicted, which is what takes a
    /// statement to its rewrite and its cut.
    pub(super) fn contradicting() -> Self {
        Self {
            contradicted: true,
            ..Self::new()
        }
    }

    /// A research planner that plans one web search for the first gap.
    pub(super) fn planning(query: &str) -> Self {
        Self {
            plan: Some(json!({"needs":[{"gap":0,"kind":"web","query":query}]}).to_string()),
            ..Self::new()
        }
    }

    pub(super) async fn respond(&self, prompt: &str) -> Result<String> {
        if let (Some(plan), true) = (
            &self.plan,
            prompt.starts_with("Plan focused reference searches"),
        ) {
            return Ok(plan.clone());
        }
        if prompt.starts_with("Extract lesson claims.") {
            let data = fixture_task(prompt).unwrap();
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
                            .find(|p| p["field"] == "/body" || p["field"] == "/explanation")
                            .unwrap();
                        let statement = statement_in(&passage_text(passage));
                        match statement {
                            Some(statement) => json!({"index":unit["index"],"claims":[{"sentenceId":sentence_id(passage, &statement),"statement":statement}],"nonFactualReason":""}),
                            None => json!({"index":unit["index"],"claims":[],"nonFactualReason":NOTHING_TO_CHECK}),
                        }
                    })
                    .collect(),
            )
            .to_string());
        }
        if let Some(reply) = super::question_review_response(prompt) {
            return Ok(reply);
        }
        if prompt.starts_with("Repair verified lesson defects.") {
            let data = fixture_task(prompt).unwrap();
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
            let (path, body) = match data["candidate"]["blocks"][0]["body"].as_str() {
                Some(body) => ("/blocks/0/body", body),
                None => (
                    "/questions/0/explanation",
                    data["candidate"]["questions"][0]["explanation"]
                        .as_str()
                        .unwrap(),
                ),
            };
            let replacement = if self.repair_to_good {
                FIXED.to_owned()
            } else {
                figure(repair)
            };
            let after = body.replace(&statement_in(body).unwrap(), &replacement);
            return Ok(json!({"edits":[{"path":path,"before":body,"after":after}]}).to_string());
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
                if self.contradicted {
                    return Ok(format!("contradicted\nReason: The reference gives another buffer size.\nSource quote: {GOOD}\nSource passage: passage-0"));
                }
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
            let correcting = serde_json::to_string(&request.input)?.contains(CORRECTION_LEAD);
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
    database: &Database,
    repo: &crate::curriculum_repository::LearningCurriculumRepository,
    job: &str,
    lesson_id: &str,
    model: &dyn LLMPort,
    first: Option<String>,
    sources: &[LearningSourceDto],
) -> Result<(String, LessonVerificationReport)> {
    lesson_drafts::run(database, repo, job, lesson_id, async {
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
async fn a_last_attempt_that_cannot_take_the_sentence_out_leaves_it_standing_and_a_lesson_left_without_its_first_explanation_publishes_it_unverified(
) -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let sources = [source()];
    let model = Figures::new();
    let (raw, report) = prepare(
        &db,
        &repo,
        &job,
        &lesson_id,
        &model,
        Some(candidate(BAD).to_string()),
        &sources,
    )
    .await
    .expect("the lesson publishes, its statement not verified");
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    // A rewrite, then the last attempt, which could only take the section
    // out, and it was the lesson's only explanation: the sentence stays.
    assert_eq!(model.repairs.load(Ordering::Relaxed), 1);
    let draft: Value = serde_json::from_str(&raw)?;
    assert!(draft["blocks"][0]["body"]
        .as_str()
        .unwrap()
        .contains(&figure(0)));

    let store = ClaimReviewRepository::new(db.clone());
    let ledger = store.load(&lesson_id).await?;
    assert!(held_for_review(&db, &lesson_id, &job).await?.is_none());
    // The rewrites kept the claim: one card, with its attempts on it, and
    // no claim waits for anyone.
    assert_eq!(ledger.claims.len(), 1, "{:?}", ledger.claims);
    let claim = &ledger.claims[0];
    assert_eq!(claim.statement(), figure(0));
    assert_eq!(claim.location().unit_index, 0);
    assert_eq!(claim.location().unit_kind, UnitKind::Block);
    assert_eq!(claim.location().field, "body");
    assert!(
        !matches!(
            claim.state(),
            ClaimState::Removed { .. } | ClaimState::Edited { .. }
        ),
        "{:?}",
        claim.state()
    );
    assert!(waiting(&ledger.claims).is_empty());
    assert_eq!(
        repairs(claim).first(),
        Some(&(1, false, RepairOutcome::Rewritten))
    );
    // Its own citation contradicts it: it went to its rewrite, not to research.
    assert!(!claim
        .events()
        .iter()
        .any(|record| matches!(record.event, ClaimEvent::Researched { .. })));
    let shown = store.shown_passages(&lesson_id).await?;
    let passages = &shown[claim.claim_id()];
    assert!(!passages.is_empty());
    assert!(passages
        .iter()
        .all(|passage| sources[0].excerpt.contains(&passage.text)));
    invariants::assert_prepared(&db, &lesson_id, &raw, &report).await?;
    Ok(())
}

#[tokio::test]
async fn a_claim_still_failing_after_its_rewrite_is_taken_out_and_the_lesson_publishes(
) -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let sources = [source()];
    let model = Figures::contradicting();
    // A second section keeps a claim: a lesson with none left is no
    // extraction the loop accepts.
    let (raw, report) = prepare(
        &db,
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
    assert!(held_for_review(&db, &lesson_id, &job).await?.is_none());
    let ledger = ClaimReviewRepository::new(db.clone())
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
    let cut = gone
        .cut_place()
        .expect("the cut recorded the sentence it took out and where it stood");
    assert!(!cut.sentence.is_empty() && !raw.contains(&cut.sentence));
    assert!(cut.before.is_some() || cut.after.is_some(), "{cut:?}");
    invariants::assert_prepared(&db, &lesson_id, &raw, &report).await?;
    Ok(())
}

#[test]
fn pending_work_saved_before_cuts_were_recorded_still_reads() {
    let saved = r#"{"previous":null,"repairs":[]}"#;
    let pending: claim_ledger::Pending = serde_json::from_str(saved).unwrap();
    assert!(pending.cuts.is_empty());
    assert_eq!(
        serde_json::to_string(&pending).unwrap(),
        r#"{"previous":null,"repairs":[]}"#,
        "no cuts, nothing new on disk"
    );
}

#[tokio::test]
async fn a_lesson_whose_every_statement_would_be_cut_publishes_them_unverified_as_ones_its_sources_do_not_support(
) -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let (raw, report) = prepare(
        &db,
        &repo,
        &job,
        &lesson_id,
        &Figures::new(),
        Some(lesson(&[BAD]).to_string()),
        &[source()],
    )
    .await
    .expect("the lesson publishes, its statement not verified");
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    let draft: Value = serde_json::from_str(&raw)?;
    assert!(draft["blocks"][0]["body"]
        .as_str()
        .unwrap()
        .contains(&figure(0)));

    let ledger = ClaimReviewRepository::new(db.clone())
        .load(&lesson_id)
        .await?;
    assert!(held_for_review(&db, &lesson_id, &job).await?.is_none());
    // The claim is not cut: its sentence stays, and nothing waits for anyone.
    let [claim] = ledger.claims.as_slice() else {
        panic!("one claim: {:?}", ledger.claims);
    };
    assert!(
        !matches!(
            claim.state(),
            ClaimState::Removed { .. } | ClaimState::Edited { .. }
        ),
        "{:?}",
        claim.state()
    );
    assert_eq!(
        repairs(claim).first(),
        Some(&(1, false, RepairOutcome::Rewritten))
    );
    assert!(waiting(&ledger.claims).is_empty());
    invariants::assert_prepared(&db, &lesson_id, &raw, &report).await?;
    Ok(())
}

#[tokio::test]
async fn a_lesson_stopped_on_checks_made_under_other_rules_completes_its_job_as_held_and_is_free_to_be_prepared_again(
) -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let stale = lesson(&[BAD]).to_string();
    // An earlier version of the app decided a claim on this sentence under
    // other checking rules.
    lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
        lesson_drafts::resume(INPUT.into()).await?;
        lesson_drafts::save(&stale).await?;
        let mut ledger = claim_ledger::ClaimLedger::open(&Model::new(), &stale).await?;
        let old = claim(&mut ledger, "old", (BAD, BAD), &section(BAD), SOURCED).await?;
        let stepped = ClaimEvent::Stepped {
            policy: "acceptance-v0".into(),
            step: Step::Read(Reading::Parts),
            outcome: Outcome::Unproven,
            then: Then::Node {
                name: "s.research".into(),
                step: Step::Research,
            },
            reason: String::new(),
            rewrite_ahead: true,
        };
        assert!(ledger.append(&old, stepped).await?);
        Ok(())
    })
    .await?;
    let error = prepare(
        &db,
        &repo,
        &job,
        &lesson_id,
        &Figures::new(),
        Some(stale),
        &[source()],
    )
    .await
    .expect_err("a fault stops the lesson")
    .to_string();
    assert!(error.contains("earlier checking rules"), "{error}");
    let review = ClaimReviewRepository::new(db.clone())
        .load(&lesson_id)
        .await?
        .review
        .unwrap();
    assert_eq!(review.state, LessonReviewState::Stopped);
    let stop = review.stop_reason.clone().unwrap();
    assert_eq!(
        stop.code,
        lattice_core::domain::learning::claim_review::StopCode::PolicyChanged
    );
    // The worker completes the job as held, not failed.
    let held = held_for_review(&db, &lesson_id, &job).await?.unwrap();
    let (result_ref, message) = review_outcome(&[held]);
    assert_eq!(message, format!("Stopped: {}", stop.message));
    assert!(
        JobStore::new(db.clone())
            .complete(&job, &result_ref, &message)
            .await?
    );
    let stopped = repo.job(&job).await?;
    assert_eq!(
        stopped.status,
        crate::curriculum::LearningGenerationJobStatus::Completed
    );

    // Preparing the lesson again starts a new job, which takes the lesson
    // from its outline and publishes it once its sources back it.
    let program = crate::repository::LearningRepository::new(db.clone())
        .get(&stopped.program_id)
        .await?;
    let again = repo
        .start_lesson_job(&crate::plan_dto::StartLearningGenerationJobRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.summary.id,
            expected_revision: program.summary.revision,
            kind: crate::curriculum::LearningGenerationJobKind::LessonPreparation,
            request_json: json!({"lessonIds":[lesson_id]}).to_string(),
            progress_total: 1,
        })
        .await?;
    assert_ne!(again.id, job);
    assert_eq!(
        again.status,
        crate::curriculum::LearningGenerationJobStatus::Pending
    );
    assert!(JobStore::new(db.clone()).claim(&again.id).await?.is_some());
    let (raw, report) = prepare(
        &db,
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
    assert!(held_for_review(&db, &lesson_id, &again.id).await?.is_none());
    invariants::assert_prepared(&db, &lesson_id, &raw, &report).await?;
    Ok(())
}

#[tokio::test]
async fn the_calls_ceiling_does_not_rise_while_a_rewrite_fails_its_recheck_and_is_cut() -> Result<()>
{
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let sources = [source()];
    let model = Figures::new();

    let (prepared, trace) = crate::lesson_progress::traced(prepare(
        &db,
        &repo,
        &job,
        &lesson_id,
        &model,
        Some(lesson(&[GOOD, BAD]).to_string()),
        &sources,
    ))
    .await;
    let (raw, report) = prepared?;

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
    invariants::assert_prepared(&db, &lesson_id, &raw, &report).await?;
    Ok(())
}

#[tokio::test]
async fn a_statement_whose_judge_never_answers_readably_keeps_its_text_and_attempts_and_the_lesson_publishes_it_unverified(
) -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let sources = [source()];
    let model = Figures {
        unusable_asks: usize::MAX,
        ..Figures::new()
    };
    let draft = lesson(&[GOOD]).to_string();
    let (raw, report) = prepare(
        &db,
        &repo,
        &job,
        &lesson_id,
        &model,
        Some(draft.clone()),
        &sources,
    )
    .await
    .expect("the lesson publishes, its statement not verified");
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert_eq!(model.repairs.load(Ordering::Relaxed), 0, "never rewritten");
    assert_eq!(
        serde_json::from_str::<Value>(&raw)?,
        serde_json::from_str::<Value>(&draft)?,
        "its text in the draft untouched"
    );

    let ledger = ClaimReviewRepository::new(db.clone())
        .load(&lesson_id)
        .await?;
    assert!(held_for_review(&db, &lesson_id, &job).await?.is_none());
    // The statement is as it was: asked twice, no attempt spent, and never
    // taken for verified.
    let [claim] = ledger.claims.as_slice() else {
        panic!("one claim: {:?}", ledger.claims);
    };
    assert!(
        !matches!(
            claim.state(),
            ClaimState::ResolvedByEvidence { .. }
                | ClaimState::Consistent { .. }
                | ClaimState::GeneralKnowledge { .. }
        ),
        "{:?}",
        claim.state()
    );
    assert_eq!(claim.unjudged(), 2);
    assert!(repairs(claim).is_empty());
    invariants::assert_prepared(&db, &lesson_id, &raw, &report).await?;
    Ok(())
}

#[tokio::test]
async fn a_statement_unreadable_once_and_readable_on_the_recheck_is_judged_normally() -> Result<()>
{
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let sources = [source()];
    let model = Figures {
        unusable_asks: 1,
        ..Figures::new()
    };
    let (raw, report) = prepare(
        &db,
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
    let ledger = ClaimReviewRepository::new(db.clone())
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
    // The statement cites nothing, so its first reading is of its page, asked
    // twice: no answer, then the judgment.
    let unanswered: Vec<bool> = claim
        .events()
        .iter()
        .filter_map(|record| match &record.event {
            ClaimEvent::Checked(check) => Some(check.result.is_unjudged()),
            _ => None,
        })
        .collect();
    assert_eq!(unanswered, [true, false]);
    assert!(held_for_review(&db, &lesson_id, &job).await?.is_none());
    invariants::assert_prepared(&db, &lesson_id, &raw, &report).await?;
    Ok(())
}

#[tokio::test]
async fn an_edit_is_applied_on_resume_and_only_its_section_is_extracted_again() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let sources = [source()];
    let model = Figures {
        fail_repair_at: Some(0),
        ..Figures::new()
    };
    let raw = lesson(&[GOOD, BAD]).to_string();
    let error = prepare(&db, &repo, &job, &lesson_id, &model, Some(raw), &sources)
        .await
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("fixture server failure"), "{error}");
    let store = ClaimReviewRepository::new(db.clone());
    let ledger = store.load(&lesson_id).await?;
    let edited = ledger
        .claims
        .iter()
        .find(|entry| entry.location().unit_index == 1)
        .unwrap();
    let edited = edited.claim_id().clone();
    ClaimReviewService::new(db.clone())
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
    let (raw, report) = prepare(&db, &repo, &job, &lesson_id, &model, None, &sources).await?;
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
    let examples = CheckerExamples::new(db.clone()).list(None).await?;
    assert_eq!(examples.len(), 1);
    assert_eq!(examples[0].label_source, LabelSource::UserEdit);
    assert_eq!(examples[0].label, Label::False);
    invariants::assert_prepared(&db, &lesson_id, &raw, &report).await?;
    Ok(())
}

#[tokio::test]
async fn a_claim_confirmed_while_its_repair_was_interrupted_is_not_sent_on_resume() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let sources = [source()];
    let model = Figures {
        repair_to_good: true,
        fail_repair_at: Some(1),
        ..Figures::new()
    };
    let raw = lesson(&[BAD, BAD]).to_string();
    let error = prepare(&db, &repo, &job, &lesson_id, &model, Some(raw), &sources)
        .await
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("fixture server failure"), "{error}");
    let store = ClaimReviewRepository::new(db.clone());
    let ledger = store.load(&lesson_id).await?;
    let second = ledger
        .claims
        .iter()
        .find(|entry| entry.location().unit_index == 1)
        .unwrap();
    // A contradicted claim is confirmed only with a note (D3).
    let service = ClaimReviewService::new(db.clone());
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

    let (raw, report) = prepare(&db, &repo, &job, &lesson_id, &model, None, &sources).await?;
    assert_eq!(
        model.repairs.load(Ordering::Relaxed),
        2,
        "the confirmed claim's repair is dropped"
    );
    let lesson: Value = serde_json::from_str(&raw)?;
    assert_eq!(lesson["blocks"][0]["body"], section(FIXED));
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
    invariants::assert_prepared(&db, &lesson_id, &raw, &report).await?;
    Ok(())
}

#[tokio::test]
async fn human_verdicts_on_flagged_claims_are_labelled_examples_that_export_as_claim_pairs(
) -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
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
    lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
        lesson_drafts::resume(INPUT.into()).await?;
        lesson_drafts::save(&draft).await
    })
    .await?;
    let store = ClaimReviewRepository::new(db.clone());
    let mut revision = store
        .set_lesson_review(
            &lesson_id,
            LessonReview {
                program_id: program_id.clone(),
                job_id: job.clone(),
                state: LessonReviewState::NeedsReview,
                draft_sha256: sha256_hex(&draft),
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
            fingerprint: EvidenceFingerprint::of(
                Judged::Passages.policy(),
                "figures-fixture",
                &seen,
            ),
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
    let service = ClaimReviewService::new(db.clone());
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
    let examples = CheckerExamples::new(db.clone());
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

#[tokio::test]
async fn the_tally_counts_a_claim_as_searching_only_where_the_policy_sends_it_to_research(
) -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let sentence = |index: usize| figure(index);
    let held = |index: usize| (sentence(index), sentence(index));
    let (unchecked_quote, unchecked_statement) = held(0);
    claim(
        &mut ledger,
        "unchecked",
        (&unchecked_quote, &unchecked_statement),
        &section(&unchecked_quote),
        SOURCED,
    )
    .await?;
    let (research_quote, research_statement) = held(1);
    let research = claim(
        &mut ledger,
        "research",
        (&research_quote, &research_statement),
        &section(&research_quote),
        SOURCED,
    )
    .await?;
    walk(
        &mut ledger,
        &research,
        Class::Sourced,
        &[(Step::Read(Reading::Parts), Outcome::Unproven)],
    )
    .await?;
    assert_eq!(
        ledger.waits_at(&policy, &research).map(|(_, step)| step),
        Some(Step::Research)
    );
    let (rewrite_quote, rewrite_statement) = held(2);
    let rewrite = claim(
        &mut ledger,
        "rewrite",
        (&rewrite_quote, &rewrite_statement),
        &section(&rewrite_quote),
        SOURCED,
    )
    .await?;
    walk(
        &mut ledger,
        &rewrite,
        Class::Sourced,
        &[
            (Step::Read(Reading::Parts), Outcome::Unproven),
            (Step::Research, Outcome::FoundNothing),
            (Step::Read(Reading::Page), Outcome::Refuted),
        ],
    )
    .await?;
    assert!(matches!(
        ledger.waits_at(&policy, &rewrite).map(|(_, step)| step),
        Some(Step::Rewrite(_))
    ));
    let outside_text = sentence(3);
    let outside = ClaimId("outside".into());
    let extracted = ClaimEvent::Extracted {
        location: ClaimLocation::locate(
            0,
            UnitKind::Block,
            "body",
            &outside_text,
            &section(&outside_text),
        ),
        statement: outside_text.clone(),
    };
    ledger.append(&outside, extracted).await?;
    let failed = CheckResult::Failed {
        kind: VerdictKind::Unsupported,
    };
    let check = local_check(failed, "The reference does not say so.", vec![]);
    ledger.append(&outside, ClaimEvent::Checked(check)).await?;

    let tally = ledger.tally(&policy);

    assert_eq!(
        (
            tally.unchecked,
            tally.searching,
            tally.repairing,
            tally.waiting
        ),
        (1, 1, 2, 0)
    );
    Ok(())
}

#[tokio::test]
async fn a_statement_no_reading_judged_is_published_not_verified_as_unchecked() -> Result<()> {
    let policy = StandardPolicy::new();
    let mut ledger = open().await?;
    let text = figure(0);
    let id = claim(
        &mut ledger,
        "unjudged",
        (&text, &text),
        &section(&text),
        SOURCED,
    )
    .await?;
    let to_cut = [
        (Step::Read(Reading::Parts), Outcome::Unproven),
        (Step::Research, Outcome::FoundNothing),
        (Step::Read(Reading::Page), Outcome::Refuted),
        (
            Step::Rewrite(Goal::WhatThePassageStates),
            Outcome::Rewritten,
        ),
        (Step::Read(Reading::Parts), Outcome::Refuted),
    ];
    walk(&mut ledger, &id, Class::Sourced, &to_cut).await?;
    let could_not_judge = CheckResult::Failed {
        kind: VerdictKind::CouldNotJudge,
    };
    let check = local_check(could_not_judge, "The checker gave no answer.", vec![]);
    ledger.append(&id, ClaimEvent::Checked(check)).await?;

    ledger
        .cut(&policy, std::slice::from_ref(&id), Outcome::CannotCut)
        .await?;

    let state = ledger.entry(&id).unwrap().state();
    assert!(
        matches!(
            state,
            ClaimState::Unverified {
                reason: UnverifiedReason::Unchecked,
                ..
            }
        ),
        "{state:?}"
    );
    Ok(())
}

#[test]
fn a_quote_in_no_field_of_its_unit_is_kept_whole() {
    let candidate = json!({
        "blocks": [{"kind":"explanation","title":"Rust","body":"Rust checks borrows at compile time. It is fast.","rubric":[]}],
        "questions": [],
    });
    let quote = "Rust checks borrows at compile time. It has no collector.";

    let location = claim_ledger::locate(
        &candidate,
        0,
        quote,
        Some("Rust checks borrows at compile time."),
    );

    assert_eq!(location.quote, quote);
    assert_eq!(location.span, None);
}

#[tokio::test]
async fn a_statement_supported_beside_an_example_run_is_not_reopened_as_a_changed_source(
) -> Result<()> {
    let mut ledger = open().await?;
    let lesson = cell_lesson(CELL);
    let run: execution::Observation = serde_json::from_value(json!({
        "id": "block-0-fence-0", "unit": 0, "runtime": "python", "code_sha256": "none",
        "code": "print(1)",
        "result": {
            "status": "passed", "exitCode": 0, "stdout": "1", "stderr": "",
            "outputTruncated": false, "durationMs": 1
        }
    }))?;
    let mut finding = passage_finding(CELL, ATP, true, CELL_SOURCE);
    finding.class = Some(Classed {
        class: Class::Sourced,
        by: ClassifiedBy::Writer,
    });
    finding.evidence.splice(0..0, example_runs(&[run], 0));
    let report = pass_report(vec![finding.clone()]);
    let ids = identify_findings(&mut ledger, &lesson, &report).await?;
    stood(
        &mut ledger,
        &ids[0],
        Class::Sourced,
        (Reading::Parts, finding),
    )
    .await?;
    let id = ids[0].clone();

    let ids = identify_findings(&mut ledger, &lesson, &report).await?;
    ledger
        .reopen_changed(&lesson, &HashSet::from(["source-1"]), &ids)
        .await?;

    let entry = ledger.entry(&id).unwrap();
    assert!(
        !entry
            .events()
            .iter()
            .any(|record| matches!(record.event, ClaimEvent::Reopened { .. })),
        "{:?}",
        entry.state()
    );
    Ok(())
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
        claim_id: None,
        two_pass: Default::default(),
        label: None,
        class: None,
    }
}

/// `findings` as the classify step leaves them for statements the writer
/// cited: sorted Sourced, so identifying them records the class.
pub(super) fn sourced(mut findings: Vec<Finding>) -> Vec<Finding> {
    for finding in &mut findings {
        finding.class = Some(Classed {
            class: Class::Sourced,
            by: ClassifiedBy::Writer,
        });
    }
    findings
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
        replaced_findings: vec![],
    }
}

/// The events a pass added to a claim's log.
fn added(entry: &ClaimLedgerEntry, from: usize) -> Vec<&ClaimEvent> {
    entry.events()[from..]
        .iter()
        .map(|record| &record.event)
        .collect()
}

/// The statements of `report`'s findings, identified with the ledger as a pass
/// identifies them before anything is read: where each stands and the class
/// its finding was checked as. Records no check. The claims, in the report's
/// order.
pub(super) async fn identify_findings(
    ledger: &mut claim_ledger::ClaimLedger,
    lesson: &Value,
    report: &LessonVerificationReport,
) -> Result<Vec<ClaimId>> {
    let claims: Vec<(usize, Claim)> = report
        .findings
        .iter()
        .map(|finding| {
            (
                finding.unit,
                Claim {
                    quote: finding.quote.clone(),
                    statement: finding.statement.clone(),
                },
            )
        })
        .collect();
    let mut extracted = ledger.extracted(lesson, &claims, &report.executions);
    for (found, finding) in extracted.iter_mut().zip(&report.findings) {
        found.class = finding.class;
    }
    let matched = ledger.match_pass(lesson, &extracted);
    ledger
        .identify(&StandardPolicy::new(), lesson, extracted, matched)
        .await
}

/// Records for each of `ids` the check of the finding at its place: supported
/// on the passage it quotes, or failed as unsupported. For a claim with no
/// class, which no reading decides.
pub(super) async fn check_findings(
    ledger: &mut claim_ledger::ClaimLedger,
    ids: &[ClaimId],
    findings: &[Finding],
) -> Result<()> {
    for (finding, id) in findings.iter().zip(ids) {
        let seen: Vec<EvidenceSeen> = finding
            .evidence
            .iter()
            .map(|passage| EvidenceSeen {
                source_version_id: passage.source_id.clone(),
                start_byte: passage.start_byte,
                end_byte: passage.end_byte,
                text_sha256: text_sha256(&passage.text),
            })
            .collect();
        let result = match (&finding.supporting_quote, finding.verdict) {
            (Some(quote), ClaimVerdict::Supported) => CheckResult::Supported {
                source_version_id: finding
                    .evidence
                    .first()
                    .map(|passage| passage.source_id.clone())
                    .unwrap_or_default(),
                supporting_quote: quote.clone(),
            },
            _ => CheckResult::Failed {
                kind: VerdictKind::Unsupported,
            },
        };
        let check = local_check(result, &finding.reason, seen);
        ledger.append(id, ClaimEvent::Checked(check)).await?;
    }
    Ok(())
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
                fingerprint: EvidenceFingerprint::of(
                    Judged::Passages.policy(),
                    model.model_name(),
                    &seen,
                ),
                evidence: seen,
                window_tokens: model.max_context_tokens(),
            }),
        )
        .await?;
    let before = ledger.entry(&id).unwrap().clone();

    let lesson = cell_lesson(CELL);
    let report = pass_report(vec![passage_finding(CELL, ATP, true, CELL_SOURCE)]);
    let ids = identify_findings(&mut ledger, &lesson, &report).await?;
    ledger
        .reopen_changed(&lesson, &HashSet::from(["source-1"]), &ids)
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
async fn a_failed_claim_sharing_its_sentence_with_a_verified_one_tells_its_rewrite_what_else_the_sentence_says_and_the_verified_one_rides_on_it(
) -> Result<()> {
    let model = Model::new();
    let mut ledger = claim_ledger::ClaimLedger::open(&model, "{}").await?;
    let overstated = "Mitochondria produce all of a cell's ATP.";
    let before = cell_lesson(CELL);
    let report = pass_report(sourced(vec![
        passage_finding(CELL, ATP, true, CELL_SOURCE),
        passage_finding(CELL, overstated, false, CELL_SOURCE),
    ]));
    let ids = identify_findings(&mut ledger, &before, &report).await?;
    check_findings(&mut ledger, &ids, &report.findings).await?;
    let (verified, shared) = (ids[0].clone(), ids[1].clone());
    walk(&mut ledger, &shared, Class::Sourced, &REWRITING).await?;
    assert_eq!(ledger.cotenants(&shared), std::slice::from_ref(&verified));
    assert_eq!(ledger.cotenants(&verified), std::slice::from_ref(&shared));

    // The rewrite is told what else the sentence says.
    let references = ReferenceCollection::lexical(&[])?;
    let context = repair_context(
        "{}",
        &before,
        &report,
        (&ledger, &StandardPolicy::new()),
        std::slice::from_ref(&shared),
        &references,
    )?;
    assert_eq!(context["failedClaims"][0]["alsoStates"], json!([ATP]));
    assert!(context["failedClaims"][0].get("lastAttempt").is_none());

    // The repair is the failed claim's; the claim that passed rides on it.
    ledger
        .begin_repairs(
            &StandardPolicy::new(),
            &before,
            std::slice::from_ref(&shared),
        )
        .await?;
    assert_eq!(ledger.pending.repairs.len(), 2);
    let rider = ledger
        .pending
        .repairs
        .iter()
        .find(|record| record.claim_id == verified)
        .unwrap();
    assert_eq!(rider.rides_on.as_ref(), Some(&shared));
    Ok(())
}

#[tokio::test]
async fn remove_and_edit_act_on_the_located_sentence() -> Result<()> {
    use lattice_core::domain::learning::claim_review::UserAction;
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let program_id = repo.job(&job).await?.program_id;
    // The lesson writes the ATP sentence with markdown and a citation.
    let raw_atp =
        "Mitochondria produce most of a cell's **ATP** through oxidative phosphorylation [1].";
    let body = CELL.replace(ATP_SENTENCE, raw_atp);
    let draft = cell_lesson(&body).to_string();
    lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
        lesson_drafts::resume(INPUT.into()).await?;
        lesson_drafts::save(&draft).await
    })
    .await?;
    let store = ClaimReviewRepository::new(db.clone());
    let mut revision = store
        .set_lesson_review(
            &lesson_id,
            LessonReview {
                program_id,
                job_id: job.clone(),
                state: LessonReviewState::NeedsReview,
                draft_sha256: sha256_hex(&draft),
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
            fingerprint: EvidenceFingerprint::of(
                Judged::Passages.policy(),
                "figures-fixture",
                &seen,
            ),
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
    let service = ClaimReviewService::new(db.clone());
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
    let candidate = lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
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

const CHANGED_AFTER: &str = "This section changed after that, so it can't be undone on its own. Edit the section to change it back.";

#[tokio::test]
async fn the_undo_of_an_edit_a_pass_already_carried_into_the_draft_says_to_edit_the_section(
) -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let program_id = repo.job(&job).await?.program_id;
    let draft = cell_lesson(CELL).to_string();
    lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
        lesson_drafts::resume(INPUT.into()).await?;
        lesson_drafts::save(&draft).await
    })
    .await?;
    let store = ClaimReviewRepository::new(db.clone());
    let revision = store
        .set_lesson_review(
            &lesson_id,
            LessonReview {
                program_id,
                job_id: job.clone(),
                state: LessonReviewState::NeedsReview,
                draft_sha256: sha256_hex(&draft),
                checker_model: "figures-fixture".into(),
                stop_reason: None,
            },
        )
        .await?;
    let provenance = |actor| EventProvenance {
        actor,
        at: 1,
        job_id: Some(job.clone()),
        app_version: "fixture".into(),
        checker_model: Some("figures-fixture".into()),
    };
    let id = ClaimId("atp".into());
    let revision = store
        .append(
            &lesson_id,
            &id,
            provenance(Actor::Machine),
            ClaimEvent::Extracted {
                location: ClaimLocation::locate(0, UnitKind::Block, "body", ATP_SENTENCE, CELL),
                statement: ATP.into(),
            },
            revision,
        )
        .await?;
    let service = ClaimReviewService::new(db.clone());
    let revision = service
        .resolve(
            &lesson_id,
            "atp",
            ClaimActionRequestDto::Edit {
                before: ATP_SENTENCE.into(),
                after: "Mitochondria make most of a cell's ATP.".into(),
            },
            revision.0,
        )
        .await?;
    let revision = store
        .append(
            &lesson_id,
            &id,
            provenance(Actor::Machine),
            ClaimEvent::Superseded {
                successor: None,
                by: Actor::User,
            },
            Revision(revision),
        )
        .await?;

    let result = service
        .resolve(&lesson_id, "atp", ClaimActionRequestDto::Undo, revision.0)
        .await;

    match result {
        Err(error) => assert!(error.to_string().contains(CHANGED_AFTER), "{error}"),
        Ok(_) => panic!("the undo was carried out"),
    }
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
) -> Result<(Counting, crate::lesson_progress::Trace)> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let model = Counting::new();
    let (prepared, trace) = crate::lesson_progress::traced(prepare(
        &db,
        &repo,
        &job,
        &lesson_id,
        &model,
        Some(cited_lesson(statements, uncited).to_string()),
        sources,
    ))
    .await;
    let (raw, report) = prepared?;
    invariants::assert_prepared(&db, &lesson_id, &raw, &report).await?;
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
fn within_the_plan(model: &Counting, trace: &crate::lesson_progress::Trace) {
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
async fn a_cited_statement_its_passages_do_not_state_reads_a_page_once_and_the_run_stays_within_the_plan(
) -> Result<()> {
    let unstated = figure(0);
    // A page on how DictReader reads rows, which gives no buffer size.
    let on_buffering = LearningSourceDto {
        id: uuid::Uuid::new_v4().to_string(),
        title: "DictReader row reading".into(),
        url: Some("https://docs.python.org/3.14/library/csv.html#reader-objects".into()),
        excerpt: "DictReader buffers rows from the file object it is given before parsing each into a dictionary; how many rows it buffers is up to that file object.".into(),
        acquired_at: 0,
    };
    let (model, trace) = prepared_counting(
        &[GOOD, GOOD, &unstated],
        &[],
        &[source(), beside_the_citation(), on_buffering],
    )
    .await?;

    within_the_plan(&model, &trace);
    // The unstated statement's page is its re-check against what research
    // found: once, and not for the two its citations support. A page with no
    // passage on the statement is decided without a call.
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
async fn a_cited_statement_its_citation_contradicts_is_rewritten_without_a_page_and_the_run_stays_within_the_plan(
) -> Result<()> {
    let (model, trace) =
        prepared_counting(&[GOOD, GOOD, BAD], &[], &[source(), beside_the_citation()]).await?;

    within_the_plan(&model, &trace);
    // Read part by part: its two neighbours, itself and its rewrite's
    // successor; no page is read.
    assert_eq!(
        (
            model.part_wise.load(Ordering::Relaxed),
            model.strict.load(Ordering::Relaxed)
        ),
        (4, 0),
        "{} requests the plan counts, ceilings {:?}",
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

#[tokio::test]
async fn a_sentence_a_repair_rewrote_is_read_afresh_so_its_re_check_judges_the_words_it_now_has(
) -> Result<()> {
    // §1 #6 of the acceptance policy design
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let sources = [source()];
    // A repair rewrites the sentence into another that asserts something else.
    let model = Figures::new();

    let (raw, report) = prepare(
        &db,
        &repo,
        &job,
        &lesson_id,
        &model,
        Some(lesson(&[GOOD, BAD]).to_string()),
        &sources,
    )
    .await?;

    let judged = model.judged.lock().unwrap().clone();
    assert!(
        judged.contains(&figure(0)),
        "the rewritten sentence was never judged as it now reads; judged: {judged:?}"
    );
    invariants::assert_prepared(&db, &lesson_id, &raw, &report).await?;
    Ok(())
}

#[tokio::test]
async fn a_claim_whose_sentence_still_stands_is_not_ended_removed_because_no_extraction_continued_it(
) -> Result<()> {
    // §1 #16 (a) of the acceptance policy design
    let policy = StandardPolicy::new();
    // The extractor names the sentence about twelve mitochondria, and not the
    // one the ledger holds a claim on.
    let model = Classes::new(&[TWELVE_SENTENCE]);
    let mut ledger = claim_ledger::ClaimLedger::open(&model, "{}").await?;
    let lesson = cell_lesson(CELL);
    let id = claim(&mut ledger, "atp", (ATP_SENTENCE, ATP), CELL, SOURCED).await?;
    let read = passage_finding(CELL, ATP, true, CELL_SOURCE);
    stood(&mut ledger, &id, SOURCED.0, (Reading::Parts, read)).await?;
    assert!(matches!(
        ledger.entry(&id).unwrap().state(),
        ClaimState::ResolvedByEvidence { .. }
    ));

    // The next pass: its extraction names no claim for a sentence the draft
    // still holds.
    let checks = ClaimChecks {
        rewrites: ledger.rewrites(),
        ..ClaimChecks::default()
    };
    let extraction =
        passes::extract(&model, &BuiltinRuntimes::for_tests(), &lesson, &checks).await?;
    let course = [LearningSourceDto {
        id: "source-1".into(),
        title: "Mitochondria".into(),
        url: None,
        excerpt: CELL_SOURCE.into(),
        acquired_at: 0,
    }];
    let references = ReferenceCollection::lexical(&course)?;
    passes::identify(
        &model,
        &policy,
        &mut ledger,
        (&lesson, &references),
        extraction,
        &checks,
    )
    .await?;

    let entry = ledger.entry(&id).unwrap();
    assert!(
        !matches!(
            entry.state(),
            ClaimState::Removed { .. } | ClaimState::Edited { .. }
        ),
        "the draft still holds {ATP_SENTENCE:?}, yet its claim is {:?}",
        entry.state()
    );
    Ok(())
}

#[tokio::test]
async fn a_lesson_is_not_published_with_a_sentence_that_no_live_claim_stands_on() -> Result<()> {
    // §1 #16 (b) of the acceptance policy design
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let sources = [source()];
    // A repair rewrites the sentence into another the sources do not state.
    let model = Figures::new();

    let published = prepare(
        &db,
        &repo,
        &job,
        &lesson_id,
        &model,
        Some(lesson(&[GOOD, BAD]).to_string()),
        &sources,
    )
    .await;

    assert_eq!(model.repairs.load(Ordering::Relaxed), 1);
    match published {
        Ok((raw, report)) => {
            invariants::assert_prepared(&db, &lesson_id, &raw, &report).await?;
            let sentence = figure(0);
            let draft: Value = serde_json::from_str(&raw)?;
            let stands = draft["blocks"][1]["body"]
                .as_str()
                .is_some_and(|body| body.contains(&sentence));
            let ledger = ClaimReviewRepository::new(db.clone())
                .load(&lesson_id)
                .await?;
            let claimed = ledger.claims.iter().any(|claim| {
                !matches!(
                    claim.state(),
                    ClaimState::Removed { .. } | ClaimState::Edited { .. }
                ) && claim.location().quote.contains(&sentence)
            });
            let held: Vec<_> = ledger
                .claims
                .iter()
                .map(|claim| (claim.statement(), claim.state()))
                .collect();
            assert!(
                !stands || claimed,
                "the lesson was published with {sentence:?} in it and no live claim on it: {held:?}"
            );
        }
        Err(error) => assert!(
            held_for_review(&db, &lesson_id, &job).await?.is_some(),
            "the lesson was not published, and is not held for review either: {error}"
        ),
    }
    Ok(())
}

#[tokio::test]
async fn a_full_extraction_that_misses_a_claim_whose_sentence_stands_carries_it_and_identification_continues_it(
) -> Result<()> {
    // §1 #16 (a) of the acceptance policy design
    let policy = StandardPolicy::new();
    let lesson = cell_lesson(CELL);
    // The extractor names the sentence about twelve mitochondria, and not the
    // one the ledger holds a claim on.
    let model = Classes::new(&[TWELVE_SENTENCE]);
    let mut ledger = claim_ledger::ClaimLedger::open(&model, "{}").await?;
    let id = claim(&mut ledger, "atp", (ATP_SENTENCE, ATP), CELL, SOURCED).await?;
    let read = passage_finding(CELL, ATP, true, CELL_SOURCE);
    stood(&mut ledger, &id, SOURCED.0, (Reading::Parts, read)).await?;
    let checks = ClaimChecks {
        rewrites: ledger.rewrites(),
        ..ClaimChecks::default()
    };

    let extraction =
        passes::extract(&model, &BuiltinRuntimes::for_tests(), &lesson, &checks).await?;

    let claims = &extraction.inventory.units[0].claims;
    assert!(
        claims
            .iter()
            .any(|claim| claim.quote == ATP_SENTENCE && claim.statement == ATP),
        "the inventory leaves out a claim whose sentence the lesson still holds: {claims:?}"
    );
    let references = ReferenceCollection::lexical(&[])?;
    let pass = passes::identify(
        &model,
        &policy,
        &mut ledger,
        (&lesson, &references),
        extraction,
        &checks,
    )
    .await?;
    assert!(
        pass.statements.iter().any(|known| known.id == id),
        "no statement of the pass continues the claim"
    );
    Ok(())
}

#[tokio::test]
async fn a_claim_whose_sentence_is_gone_is_not_carried_by_the_extraction() -> Result<()> {
    let model = Classes::new(&[TWELVE_SENTENCE]);
    let mut ledger = claim_ledger::ClaimLedger::open(&model, "{}").await?;
    claim(&mut ledger, "atp", (ATP_SENTENCE, ATP), CELL, SOURCED).await?;
    let cut = cell_lesson(&CELL.replace(&format!("{ATP_SENTENCE} "), ""));
    let checks = ClaimChecks {
        rewrites: ledger.rewrites(),
        ..ClaimChecks::default()
    };

    let extraction = passes::extract(&model, &BuiltinRuntimes::for_tests(), &cut, &checks).await?;

    let claims = &extraction.inventory.units[0].claims;
    assert!(
        claims.iter().all(|claim| claim.statement != ATP),
        "{claims:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_claim_the_extraction_words_differently_on_its_sentence_is_continued_not_carried_twice(
) -> Result<()> {
    // The extractor states the sentence itself, where the ledger holds a
    // restatement of it.
    let model = Classes::new(&[ATP_SENTENCE]);
    let mut ledger = claim_ledger::ClaimLedger::open(&model, "{}").await?;
    claim(&mut ledger, "atp", (ATP_SENTENCE, ATP), CELL, SOURCED).await?;
    let checks = ClaimChecks {
        rewrites: ledger.rewrites(),
        ..ClaimChecks::default()
    };

    let extraction = passes::extract(
        &model,
        &BuiltinRuntimes::for_tests(),
        &cell_lesson(CELL),
        &checks,
    )
    .await?;

    let claims = &extraction.inventory.units[0].claims;
    let on_the_sentence = claims
        .iter()
        .filter(|claim| claim.quote == ATP_SENTENCE)
        .count();
    assert_eq!(on_the_sentence, 1, "{claims:?}");
    Ok(())
}

#[tokio::test]
async fn two_claims_on_one_sentence_are_both_kept_when_the_extraction_names_one() -> Result<()> {
    let policy = StandardPolicy::new();
    let overstated = "Mitochondria produce all of a cell's ATP.";
    let model = Classes::new(&[ATP_SENTENCE]);
    let mut ledger = claim_ledger::ClaimLedger::open(&model, "{}").await?;
    let lesson = cell_lesson(CELL);
    let first = claim(&mut ledger, "first", (ATP_SENTENCE, ATP), CELL, SOURCED).await?;
    let second = claim(
        &mut ledger,
        "second",
        (ATP_SENTENCE, overstated),
        CELL,
        SOURCED,
    )
    .await?;
    let checks = ClaimChecks {
        rewrites: ledger.rewrites(),
        ..ClaimChecks::default()
    };

    let extraction =
        passes::extract(&model, &BuiltinRuntimes::for_tests(), &lesson, &checks).await?;
    let references = ReferenceCollection::lexical(&[])?;
    let pass = passes::identify(
        &model,
        &policy,
        &mut ledger,
        (&lesson, &references),
        extraction,
        &checks,
    )
    .await?;

    assert_eq!(ledger.fault(&policy), None);
    for id in [&first, &second] {
        assert!(
            pass.statements.iter().any(|known| known.id == *id),
            "no statement of the pass continues {id:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_lesson_with_a_sentence_a_claim_the_machine_ended_stood_on_publishes_with_that_sentence_unverified(
) -> Result<()> {
    // §1 #16 (b) of the acceptance policy design
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
    let raw = lesson(&[GOOD]).to_string();
    // An earlier run ended a claim on this sentence, which the sentence
    // outlived.
    lesson_drafts::run(&db, &repo, &job, &lesson_id, async {
        lesson_drafts::resume(INPUT.into()).await?;
        lesson_drafts::save(&raw).await?;
        let mut ledger = claim_ledger::ClaimLedger::open(&Model::new(), &raw).await?;
        let ended = claim(&mut ledger, "ended", (GOOD, GOOD), &section(GOOD), SOURCED).await?;
        let gone = ClaimEvent::Superseded {
            successor: None,
            by: Actor::Machine,
        };
        assert!(ledger.append(&ended, gone).await?);
        Ok(())
    })
    .await?;

    let (published, report) = prepare(
        &db,
        &repo,
        &job,
        &lesson_id,
        &Figures::new(),
        Some(raw),
        &[source()],
    )
    .await
    .expect("the lesson publishes, the sentence not verified");

    assert!(report.issues.is_empty(), "{:?}", report.issues);
    let draft: Value = serde_json::from_str(&published)?;
    assert!(
        draft["blocks"][0]["body"].as_str().unwrap().contains(GOOD),
        "the sentence stands in the published lesson"
    );
    assert!(held_for_review(&db, &lesson_id, &job).await?.is_none());
    invariants::assert_prepared(&db, &lesson_id, &published, &report).await?;
    Ok(())
}

#[tokio::test]
async fn a_claim_the_machine_ended_blocks_publication_while_its_sentence_stands_in_a_section_that_stays(
) -> Result<()> {
    let mut ledger = claim_ledger::ClaimLedger::open(&Model::new(), "{}").await?;
    let lesson = cell_lesson(CELL);
    let atp = claim(&mut ledger, "atp", (ATP_SENTENCE, ATP), CELL, SOURCED).await?;
    assert!(
        ledger.ended_but_standing(&lesson).is_none(),
        "a claim that is not ended blocks nothing"
    );
    let ended = ClaimEvent::Superseded {
        successor: None,
        by: Actor::Machine,
    };
    assert!(ledger.append(&atp, ended).await?);

    let (reason, claims) = ledger.ended_but_standing(&lesson).unwrap();
    assert_eq!(claims, 1);
    assert!(
        reason.message.contains(ATP_SENTENCE) && reason.message.contains(ATP),
        "{}",
        reason.message
    );
    // The sentence is gone with a cut.
    let cut = cell_lesson(&CELL.replace(&format!("{ATP_SENTENCE} "), ""));
    assert!(ledger.ended_but_standing(&cut).is_none());
    // Its section is taken out of the lesson before it is published.
    ledger.cuts.go(0, "Mitochondria".into());
    assert!(ledger.ended_but_standing(&lesson).is_none());
    Ok(())
}

#[tokio::test]
async fn a_code_examples_ended_claim_blocks_nothing() -> Result<()> {
    let mut ledger = claim_ledger::ClaimLedger::open(&Model::new(), "{}").await?;
    let example = ClaimId("example".into());
    let extracted = ClaimEvent::Extracted {
        location: ClaimLocation::locate(0, UnitKind::Block, "body", ATP_SENTENCE, CELL),
        statement: "The code example example-0 runs to completion with the output it shows.".into(),
    };
    assert!(ledger.append(&example, extracted).await?);
    let run = ClaimEvent::CodeExample {
        observation: "example-0".into(),
    };
    assert!(ledger.append(&example, run).await?);
    let ended = ClaimEvent::Superseded {
        successor: None,
        by: Actor::Machine,
    };
    assert!(ledger.append(&example, ended).await?);

    assert!(ledger.ended_but_standing(&cell_lesson(CELL)).is_none());
    Ok(())
}

/// A course whose lesson is prepared against a source its library holds, so
/// what a preparation returns can be published and read back as a learner
/// reads it.
pub(super) struct Course {
    pub(super) pool: sqlx::SqlitePool,
    pub(super) db: Database,
    repo: crate::curriculum_repository::LearningCurriculumRepository,
    job: String,
    pub(super) lesson_id: String,
    pub(super) program_id: String,
    sources: Vec<LearningSourceDto>,
}

/// A published lesson as the learner reads it.
pub(super) struct Shown {
    draft: Value,
    view: LearningLessonEvidenceDto,
    claims: Vec<Value>,
}

impl Course {
    /// Records a local checker's failed check on the claim that states
    /// `statement`, as a flag the user's verdict then labels.
    pub(super) async fn flag_locally(&self, statement: &str) -> Result<()> {
        let store = ClaimReviewRepository::new(self.db.clone());
        let ledger = store.load(&self.lesson_id).await?;
        let claim = ledger
            .claims
            .iter()
            .find(|entry| entry.statement() == statement)
            .map(|entry| entry.claim_id().clone())
            .expect("a claim states it");
        let check = local_check(
            CheckResult::Failed {
                kind: VerdictKind::Unsupported,
            },
            "No passage says it.",
            Vec::new(),
        );
        let provenance = EventProvenance {
            actor: Actor::Machine,
            at: chrono::Utc::now().timestamp_millis(),
            job_id: None,
            app_version: "test".into(),
            checker_model: Some("fixture".into()),
        };
        store
            .append(
                &self.lesson_id,
                &claim,
                provenance,
                ClaimEvent::Checked(check),
                ledger.revision,
            )
            .await?;
        Ok(())
    }

    /// Each claim's statement and the events it went through, so a failed
    /// assertion shows where each walk ended.
    pub(super) async fn trail(&self) -> Result<String> {
        let ledger = ClaimReviewRepository::new(self.db.clone())
            .load(&self.lesson_id)
            .await?;
        Ok(ledger
            .claims
            .iter()
            .map(|entry| {
                let events: Vec<_> = entry.events().iter().map(|record| &record.event).collect();
                format!("{}: {events:?}", entry.statement())
            })
            .collect::<Vec<_>>()
            .join("\n"))
    }

    pub(super) async fn new() -> Result<Self> {
        let pool = crate::tests::pool().await?;
        let db = Database::new(pool.clone());
        let (repo, job, lesson_id) = batch_scheduler_tests::setup(&db).await?;
        let program_id = repo.job(&job).await?.program_id;
        install_source(&db, &program_id, SOURCE).await?;
        let sources = LearningRepository::new(db.clone())
            .verification_sources(&program_id)
            .await?;
        Ok(Self {
            pool,
            db,
            repo,
            job,
            lesson_id,
            program_id,
            sources,
        })
    }

    pub(super) async fn prepare(
        &self,
        model: &dyn LLMPort,
        draft: &Value,
    ) -> Result<(String, LessonVerificationReport)> {
        prepare(
            &self.db,
            &self.repo,
            &self.job,
            &self.lesson_id,
            model,
            Some(draft.to_string()),
            &self.sources,
        )
        .await
    }

    pub(super) async fn assert_invariants(
        &self,
        raw: &str,
        report: &LessonVerificationReport,
    ) -> Result<()> {
        invariants::assert_prepared(&self.db, &self.lesson_id, raw, report).await
    }

    /// Publishes what a preparation returned as the worker does, and reads
    /// the lesson back.
    pub(super) async fn publish(
        &self,
        prepared: (String, LessonVerificationReport),
    ) -> Result<Shown> {
        let (raw, report) = prepared;
        assert!(report.issues.is_empty(), "{:?}", report.issues);
        let draft: Value = serde_json::from_str(&raw)?;
        let mut lesson = PreparedLearningLesson {
            blocks: draft["blocks"]
                .as_array()
                .unwrap()
                .iter()
                .map(|block| {
                    LearningBlockDto::new(
                        LearningBlockKind::Explanation,
                        block["title"].as_str().unwrap().to_owned(),
                        block["body"].as_str().unwrap().to_owned(),
                        Vec::new(),
                        Vec::new(),
                    )
                })
                .collect(),
            questions: Vec::new(),
            keys: Vec::new(),
            verification: None,
        };
        report.cite_user_evidence(&mut lesson);
        lesson.verification = Some(report.bind(&self.lesson_id, &lesson)?);
        self.repo.seed_accepted(&self.program_id).await?;
        let programs = LearningRepository::new(self.db.clone());
        let revision = programs.get(&self.program_id).await?.summary.revision;
        self.repo
            .publish_prepared(
                &self.job,
                revision,
                &[(self.lesson_id.clone(), lesson)],
                &[],
                None,
            )
            .await?;

        let program = programs.get(&self.program_id).await?;
        let published = program
            .modules
            .iter()
            .flat_map(|module| module.lessons.iter())
            .find(|lesson| lesson.id == self.lesson_id)
            .unwrap();
        assert_eq!(published.preparation, LearningPreparation::Ready);
        assert_eq!(
            self.repo.job(&self.job).await?.status,
            crate::curriculum::LearningGenerationJobStatus::Completed
        );
        let ledger = ClaimReviewRepository::new(self.db.clone())
            .load(&self.lesson_id)
            .await?;
        if let Some(review) = ledger.review {
            assert_eq!(review.state, LessonReviewState::Ready);
        }
        let view = crate::lesson_evidence::get(&self.db, &self.program_id, &self.lesson_id)
            .await?
            .expect("the published lesson has its evidence");
        let claims = view
            .teaching_claims
            .iter()
            .map(serde_json::to_value)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(Shown {
            draft,
            view,
            claims,
        })
    }
}

impl Shown {
    /// The claims on `sentence`.
    fn on(&self, sentence: &str) -> Vec<&Value> {
        self.claims
            .iter()
            .filter(|entry| entry["claim"] == json!(sentence))
            .collect()
    }

    /// The sentences of the claims read as `resolution`.
    fn read_as(&self, resolution: &str) -> Vec<&str> {
        self.claims
            .iter()
            .filter(|entry| entry["resolution"] == json!(resolution))
            .map(|entry| entry["claim"].as_str().unwrap())
            .collect()
    }

    /// The learner reads `entry` as not verified for `reason`, and its
    /// sentence still stands in its section.
    fn assert_not_verified(&self, entry: &Value, reason: &str) {
        assert_eq!(entry["resolution"], json!("not_verified"), "{entry}");
        assert_eq!(entry["unverifiedReason"], json!(reason), "{entry}");
        let section = entry["sectionIndex"].as_u64().unwrap() as usize;
        let body = self.draft["blocks"][section]["body"].as_str().unwrap();
        assert!(body.contains(entry["claim"].as_str().unwrap()), "{entry}");
        assert!(
            body.contains(entry["contentQuote"].as_str().unwrap()),
            "{entry}"
        );
    }

    /// `sentence` is the one statement the learner is told is not verified
    /// for `reason`, and no statement is read as verified.
    fn assert_only_not_verified(&self, sentence: &str, reason: &str) {
        let on = self.on(sentence);
        assert_eq!(on.len(), 1, "{:?}", self.claims);
        self.assert_not_verified(on[0], reason);
        assert_eq!(self.claims.len(), 1, "{:?}", self.claims);
        assert_eq!(self.view.claim_count, 1);
        assert!(self.read_as("verified").is_empty(), "{:?}", self.claims);
    }
}

#[tokio::test]
async fn a_statement_whose_cut_would_take_the_lessons_first_explanation_is_published_not_verified_as_one_its_sources_do_not_state(
) -> Result<()> {
    let course = Course::new().await?;
    let prepared = course
        .prepare(&Figures::new(), &candidate(BAD))
        .await
        .expect("the lesson publishes, its statement not verified");

    course.assert_invariants(&prepared.0, &prepared.1).await?;
    let shown = course.publish(prepared).await?;

    shown.assert_only_not_verified(&figure(0), "unsupported");
    Ok(())
}

#[tokio::test]
async fn a_statement_whose_last_reading_was_a_contradiction_is_published_not_verified_as_one_a_source_disagrees_with(
) -> Result<()> {
    let course = Course::new().await?;
    let model = Figures {
        contradicted: true,
        ..Figures::new()
    };
    let draft = json!({"blocks":[bare(0, &figure(5))],"questions":[]});
    let prepared = course
        .prepare(&model, &draft)
        .await
        .expect("the lesson publishes, its statement not verified");

    course.assert_invariants(&prepared.0, &prepared.1).await?;
    let shown = course.publish(prepared).await?;

    shown.assert_only_not_verified(&figure(0), "contradicted");
    Ok(())
}

#[tokio::test]
async fn a_lesson_whose_every_statement_would_be_cut_is_published_with_each_not_verified(
) -> Result<()> {
    let course = Course::new().await?;
    let prepared = course
        .prepare(&Figures::new(), &lesson(&[BAD]))
        .await
        .expect("the lesson publishes, its statement not verified");

    course.assert_invariants(&prepared.0, &prepared.1).await?;
    let shown = course.publish(prepared).await?;

    shown.assert_only_not_verified(&figure(0), "unsupported");
    Ok(())
}

#[tokio::test]
async fn a_statement_beside_one_the_sources_do_not_state_still_reads_verified_and_the_other_does_not(
) -> Result<()> {
    let course = Course::new().await?;
    let draft = json!({"blocks":[
        {"kind":"explanation","title":"Section 0","body":BAD,"rubric":[]},
        {"kind":"explanation","title":"Section 1","body":section(GOOD),"rubric":[]}
    ],"questions":[]});
    let prepared = course
        .prepare(&Figures::new(), &draft)
        .await
        .expect("the lesson publishes, one statement not verified");

    course.assert_invariants(&prepared.0, &prepared.1).await?;
    let shown = course.publish(prepared).await?;

    // The settled statement reads as it always did, and is the only one that
    // does: the other is counted among the claims, never as verified.
    assert_eq!(shown.view.claim_count, 2, "{:?}", shown.claims);
    assert_eq!(shown.read_as("verified"), [GOOD]);
    assert_eq!(shown.view.confirmed_count, 0);
    let on = shown.on(&figure(0));
    assert_eq!(on.len(), 1, "{:?}", shown.claims);
    shown.assert_not_verified(on[0], "unsupported");
    assert_ne!(on[0]["resolution"], json!("verified"));
    Ok(())
}

#[tokio::test]
async fn a_statement_whose_judge_never_answers_readably_is_published_not_verified_as_one_that_was_not_checked(
) -> Result<()> {
    let course = Course::new().await?;
    let model = Figures {
        unusable_asks: usize::MAX,
        ..Figures::new()
    };
    let prepared = course
        .prepare(&model, &lesson(&[GOOD]))
        .await
        .expect("the lesson publishes, its statement not verified");

    course.assert_invariants(&prepared.0, &prepared.1).await?;
    let shown = course.publish(prepared).await?;

    shown.assert_only_not_verified(GOOD, "unchecked");
    Ok(())
}

#[tokio::test]
async fn a_statement_left_for_the_user_when_no_machine_work_is_left_is_published_not_verified_as_unchecked(
) -> Result<()> {
    let course = Course::new().await?;
    let draft = lesson(&[GOOD]);
    // An earlier run verified the sentence against a source the course has
    // since replaced, so the claim is reopened and waits for the user.
    lesson_drafts::run(
        &course.db,
        &course.repo,
        &course.job,
        &course.lesson_id,
        async {
            lesson_drafts::resume(INPUT.into()).await?;
            lesson_drafts::save(&draft.to_string()).await?;
            let mut ledger =
                claim_ledger::ClaimLedger::open(&Model::new(), &draft.to_string()).await?;
            let id = claim(
                &mut ledger,
                "verified",
                (GOOD, GOOD),
                &section(GOOD),
                SOURCED,
            )
            .await?;
            let read = passage_finding(GOOD, GOOD, true, SOURCE);
            stood(&mut ledger, &id, SOURCED.0, (Reading::Parts, read)).await
        },
    )
    .await?;
    let prepared = course
        .prepare(&Figures::new(), &draft)
        .await
        .expect("the lesson publishes, its statement not verified");

    course.assert_invariants(&prepared.0, &prepared.1).await?;
    let shown = course.publish(prepared).await?;

    shown.assert_only_not_verified(GOOD, "unchecked");
    Ok(())
}

#[tokio::test]
async fn a_sentence_a_claim_the_machine_ended_stood_on_is_published_with_that_claim_not_verified_by_its_last_reading(
) -> Result<()> {
    let course = Course::new().await?;
    let draft = lesson(&[GOOD]);
    // An earlier run read a claim on this sentence as contradicted and ended
    // it; the sentence outlived it.
    lesson_drafts::run(
        &course.db,
        &course.repo,
        &course.job,
        &course.lesson_id,
        async {
            lesson_drafts::resume(INPUT.into()).await?;
            lesson_drafts::save(&draft.to_string()).await?;
            let mut ledger =
                claim_ledger::ClaimLedger::open(&Model::new(), &draft.to_string()).await?;
            let ended = claim(&mut ledger, "ended", (GOOD, GOOD), &section(GOOD), SOURCED).await?;
            let read = local_check(
                CheckResult::Failed {
                    kind: VerdictKind::Contradicted,
                },
                "A source says otherwise.",
                vec![],
            );
            assert!(ledger.append(&ended, ClaimEvent::Checked(read)).await?);
            let gone = ClaimEvent::Superseded {
                successor: None,
                by: Actor::Machine,
            };
            assert!(ledger.append(&ended, gone).await?);
            Ok(())
        },
    )
    .await?;
    let prepared = course
        .prepare(&Figures::new(), &draft)
        .await
        .expect("the lesson publishes, the sentence not verified");

    course.assert_invariants(&prepared.0, &prepared.1).await?;
    let shown = course.publish(prepared).await?;

    let on = shown.on(GOOD);
    let marked = on
        .iter()
        .find(|entry| entry["resolution"] == json!("not_verified"))
        .unwrap_or_else(|| panic!("no claim on {GOOD:?} is marked: {:?}", shown.claims));
    shown.assert_not_verified(marked, "contradicted");
    Ok(())
}

/// A question citing the lesson's source, as the lesson schema requires.
fn quiz(kind: &str, explanation: &str) -> Value {
    json!({"kind":kind,"prompt":"Which?","options":["One","Two","Three","Four"],"correctIndex":0,"explanation":explanation,"sourceIndex":0,"quote":GOOD})
}

/// A section whose whole body is one sentence, so a cut that takes the
/// sentence takes the section.
fn bare(index: usize, sentence: &str) -> Value {
    json!({"kind":"explanation","title":format!("Section {index}"),"body":sentence,"rubric":[]})
}

/// `block` citing the source's own sentence, so the review finds nothing in
/// its citation. The course's library numbers the source it holds second, after
/// the one its setup adds.
fn cited(mut block: Value) -> Value {
    block["sourceIndex"] = json!(1);
    block["quote"] = json!(GOOD);
    block
}

/// A lesson of two cited sections that state what the sources do, and
/// `questions`.
fn with_questions(questions: Vec<Value>) -> Value {
    json!({"blocks":[
        {"kind":"explanation","title":"Section 0","body":section(GOOD),"rubric":[],"sourceIndex":0,"quote":GOOD},
        {"kind":"explanation","title":"Section 1","body":section(GOOD),"rubric":[],"sourceIndex":0,"quote":GOOD}
    ],"questions":questions})
}

#[tokio::test]
async fn statements_alone_in_their_sections_are_published_not_verified_instead_of_taking_the_sections_out(
) -> Result<()> {
    let course = Course::new().await?;
    let (first, second) = (figure(5), figure(6));
    let draft = json!({"blocks":[
        cited(json!({"kind":"explanation","title":"Section 0","body":section(GOOD),"rubric":[]})),
        cited(bare(1, &first)),
        cited(bare(2, &second))
    ],"questions":[]});
    let prepared = course
        .prepare(&Figures::contradicting(), &draft)
        .await
        .expect("the lesson publishes, two statements not verified");

    course.assert_invariants(&prepared.0, &prepared.1).await?;
    let shown = course.publish(prepared).await?;

    // Each section's sentence is the figure its rewrite gave, and stands.
    assert_eq!(shown.view.claim_count, 3, "{:?}", shown.claims);
    assert_eq!(shown.read_as("verified"), [GOOD]);
    for sentence in [figure(0), figure(1)] {
        let on = shown.on(&sentence);
        assert_eq!(on.len(), 1, "{sentence}: {:?}", shown.claims);
        shown.assert_not_verified(on[0], "contradicted");
    }
    Ok(())
}

// A question's claims are the lesson's own and are not among the learner's
// teaching claims, so the next two read the lesson as the preparation returns
// it: it publishes, and keeps a question.

#[tokio::test]
async fn a_lesson_whose_cuts_would_take_all_of_its_questions_keeps_the_last_with_its_statement_standing(
) -> Result<()> {
    let course = Course::new().await?;
    let draft = with_questions(vec![quiz("quiz", &figure(5)), quiz("quiz", &figure(6))]);
    let (raw, report) = course
        .prepare(
            &Figures {
                contradicted: true,
                ..Figures::planning("csv dictreader buffer size")
            },
            &draft,
        )
        .await
        .expect("the lesson publishes, its last question kept");

    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert!(held_for_review(&course.db, &course.lesson_id, &course.job)
        .await?
        .is_none());
    let published: Value = serde_json::from_str(&raw)?;
    let questions = published["questions"].as_array().unwrap();
    let trail = course.trail().await?;
    assert_eq!(questions.len(), 1, "{questions:?}\n{trail}");
    assert_eq!(questions[0]["explanation"], json!(figure(1)), "{trail}");
    course.assert_invariants(&raw, &report).await?;
    Ok(())
}

#[tokio::test]
async fn a_lesson_whose_cuts_would_take_every_question_of_a_kind_it_asks_is_published_with_that_statement_standing(
) -> Result<()> {
    let course = Course::new().await?;
    let draft = with_questions(vec![quiz("practice", &figure(5)), quiz("quiz", GOOD)]);
    let (raw, report) = course
        .prepare(
            &Figures {
                contradicted: true,
                ..Figures::planning("csv dictreader buffer size")
            },
            &draft,
        )
        .await
        .expect("the lesson publishes, one statement not verified");

    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert!(held_for_review(&course.db, &course.lesson_id, &course.job)
        .await?
        .is_none());
    let published: Value = serde_json::from_str(&raw)?;
    let questions = published["questions"].as_array().unwrap();
    let trail = course.trail().await?;
    assert_eq!(questions.len(), 2, "{questions:?}\n{trail}");
    assert_eq!(questions[0]["kind"], json!("practice"));
    assert_eq!(questions[0]["explanation"], json!(figure(0)));
    assert_eq!(questions[1]["explanation"], json!(GOOD));
    course.assert_invariants(&raw, &report).await?;
    Ok(())
}

/// A model whose judge never answers readably, so every statement is
/// published not verified as not checked.
pub(super) fn unreadable() -> Figures {
    Figures {
        unusable_asks: usize::MAX,
        ..Figures::new()
    }
}

#[test]
fn a_choices_reason_is_a_field_of_its_question_and_its_sentence_is_the_answer_keys() {
    const REASON: &str = "B fails because it reverses the order.";
    let lesson = json!({
        "blocks":[{"kind":"explanation","title":"Idea","body":"The idea."}],
        "questions":[{"prompt":"Which?","options":["a","b","c","d"],"explanation":"A holds.",
            "rationales":["",REASON,"C fails.","D fails."]}]
    });

    let location = locate(&lesson, 1, REASON, None);

    assert_eq!(location.unit_index, 1);
    assert_eq!(location.unit_kind, UnitKind::AnswerKey);
    assert_eq!(location.field, "rationales/1");
    assert_eq!(
        field_text(&lesson, 1, "rationales/2").as_deref(),
        Some("C fails.")
    );
    assert_eq!(
        unit_texts(&lesson)[1].1,
        format!("a\n\nb\n\nc\n\nd\n\nA holds.\n\n\n\n{REASON}\n\nC fails.\n\nD fails.")
    );
}
