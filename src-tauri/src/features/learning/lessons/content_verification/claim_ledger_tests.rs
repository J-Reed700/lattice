//! The claim ledger drives the verify-and-repair loop (§4, §12): a claim gets
//! two machine attempts per piece of evidence and then waits for the user,
//! with everything compared kept; the user's verdicts are never re-litigated
//! and are labelled examples of the checker's accuracy (§15).
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

/// The claim a fixture section states.
fn statement_in(text: &str) -> String {
    if let Some(start) = text.find("DictReader buffers ") {
        let rest = &text[start..];
        return rest[..=rest.find('.').unwrap()].to_owned();
    }
    if text.contains(BAD) {
        BAD.into()
    } else if text.contains(GOOD) {
        GOOD.into()
    } else {
        text.into()
    }
}

/// A lesson of one-sentence sections.
fn lesson(statements: &[&str]) -> Value {
    let blocks: Vec<_> = statements
        .iter()
        .enumerate()
        .map(|(index, statement)| {
            json!({"kind":"explanation","title":format!("Section {index}"),"body":statement,"rubric":[]})
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
    unusable_judge: bool,
    repairs: AtomicUsize,
    escalations: AtomicUsize,
    judged: Mutex<Vec<String>>,
    extracted: Mutex<Vec<Vec<usize>>>,
}

impl Figures {
    pub(super) fn new() -> Self {
        Self {
            inner: Model::new(),
            repair_to_good: false,
            fail_repair_at: None,
            unusable_judge: false,
            repairs: AtomicUsize::new(0),
            escalations: AtomicUsize::new(0),
            judged: Mutex::new(Vec::new()),
            extracted: Mutex::new(Vec::new()),
        }
    }

    fn judged_figures(&self) -> usize {
        self.judged
            .lock()
            .unwrap()
            .iter()
            .filter(|claim| claim.starts_with("DictReader buffers "))
            .count()
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
                        json!({"index":unit["index"],"claims":[{"passageId":passage["id"],"statement":statement}],"nonFactualReason":""})
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
            let changes: Vec<_> = data["existingClaims"]
                .as_array()
                .unwrap()
                .iter()
                .map(|claim| json!({"claimId":claim["claimId"],"passageId":passage["id"],"statement":if claim["statement"] == json!(statement) { Value::Null } else { json!(statement) }}))
                .collect();
            return Ok(json!({"changes":changes,"nonFactualReason":""}).to_string());
        }
        if prompt.starts_with("Repair verified lesson defects.") {
            let data = self::context(prompt);
            if data["repeatedDefects"].as_bool() == Some(true) {
                self.escalations.fetch_add(1, Ordering::Relaxed);
            }
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
            let after = body.replace(&statement_in(body), &replacement);
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
        if self.unusable_judge
            && prompts
                .iter()
                .any(|prompt| prompt.starts_with("Source passages:"))
        {
            return Ok(CompletionResponse::from_text("no verdict here"));
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
async fn a_claim_that_fails_twice_on_the_same_evidence_waits_for_the_user_with_everything_kept(
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
    assert!(error.starts_with("Needs your review: 1 claim"), "{error}");
    // A correction, then the escalated rewrite, then the user: no third try.
    assert_eq!(model.repairs.load(Ordering::Relaxed), 2);
    assert_eq!(model.escalations.load(Ordering::Relaxed), 1);

    let store = ClaimReviewRepository::new(pool.clone());
    let ledger = store.load(&lesson_id).await?;
    let review = ledger.review.clone().unwrap();
    assert_eq!(review.state, LessonReviewState::NeedsReview);
    assert_eq!(review.job_id, job);
    // The rewrites kept the claim: one card, with both attempts on it.
    assert_eq!(ledger.claims.len(), 1, "{:?}", ledger.claims);
    let claim = waiting(&ledger.claims)[0];
    assert_eq!(claim.statement(), figure(1));
    assert_eq!(claim.location().unit_index, 0);
    assert_eq!(claim.location().unit_kind, UnitKind::Block);
    assert_eq!(claim.location().field, "body");
    let ClaimState::Queued { kind, reason } = claim.state() else {
        panic!("{:?}", claim.state());
    };
    assert_eq!(*kind, VerdictKind::Unsupported);
    assert!(reason.contains("buffer size"), "{reason}");
    assert_eq!(
        repairs(claim),
        vec![
            (1, false, RepairOutcome::Rewritten),
            (2, true, RepairOutcome::Rewritten)
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
        .all(|passage| SOURCE.contains(&passage.text)));
    // The job reads the review row: held for review, not failed.
    let held = held_for_review(&pool, &lesson_id, &job).await?.unwrap();
    assert_eq!(held.claims, 1);
    assert_eq!(
        review_outcome(&[held]),
        (
            format!("needs_review:{lesson_id}"),
            "Needs your review: 1 claim".to_owned()
        )
    );
    Ok(())
}

#[tokio::test]
async fn a_claim_the_checker_cannot_judge_waits_for_the_user_instead_of_failing_the_job(
) -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let sources = [source()];
    let model = Figures {
        unusable_judge: true,
        ..Figures::new()
    };
    let error = prepare(
        &repo,
        &job,
        &lesson_id,
        &model,
        Some(candidate(GOOD).to_string()),
        &sources,
    )
    .await
    .err()
    .unwrap();
    assert!(
        error.to_string().starts_with("Needs your review:"),
        "{error}"
    );
    assert_eq!(model.repairs.load(Ordering::Relaxed), 0, "never repaired");
    let ledger = ClaimReviewRepository::new(pool.clone())
        .load(&lesson_id)
        .await?;
    let queued = waiting(&ledger.claims);
    assert!(!queued.is_empty());
    for claim in queued {
        let ClaimState::Queued { kind, reason } = claim.state() else {
            panic!("{:?}", claim.state());
        };
        assert_eq!(*kind, VerdictKind::CouldNotJudge);
        assert!(!reason.trim().is_empty());
    }
    assert!(held_for_review(&pool, &lesson_id, &job).await?.is_some());
    Ok(())
}

#[tokio::test]
async fn a_confirmed_claim_publishes_on_resume_without_being_checked_again() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let sources = [source()];
    let model = Figures::new();
    let raw = candidate(BAD).to_string();
    assert!(
        prepare(&repo, &job, &lesson_id, &model, Some(raw), &sources)
            .await
            .is_err()
    );
    let service = ClaimReviewService::new(pool.clone());
    // Resume with nothing changed reruns nothing (§6.2).
    assert!(!service.changed_since_stop(&lesson_id).await?);
    let store = ClaimReviewRepository::new(pool.clone());
    let ledger = store.load(&lesson_id).await?;
    let claim = waiting(&ledger.claims)[0].claim_id().clone();
    assert!(service
        .resolve(
            &lesson_id,
            &claim.0,
            ClaimActionRequestDto::Confirm {
                note: None,
                source_version_ids: vec![],
            },
            ledger.revision.0 - 1,
        )
        .await
        .is_err());
    service
        .resolve(
            &lesson_id,
            &claim.0,
            ClaimActionRequestDto::Confirm {
                note: None,
                source_version_ids: vec![],
            },
            ledger.revision.0,
        )
        .await?;
    assert!(service.changed_since_stop(&lesson_id).await?);

    let judged = model.judged_figures();
    let (_, report) = prepare(&repo, &job, &lesson_id, &model, None, &sources).await?;
    assert_eq!(
        model.judged_figures(),
        judged,
        "the confirmed claim is not rechecked"
    );
    assert_eq!(model.repairs.load(Ordering::Relaxed), 2, "nor repaired");
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    let finding = &report.findings[0];
    assert_eq!(finding.statement, figure(1));
    assert!(matches!(
        finding.resolution,
        Some(general_knowledge::Resolution::Confirmed { note: None, .. })
    ));
    let ledger = store.load(&lesson_id).await?;
    assert!(matches!(
        ledger.claims[0].state(),
        ClaimState::Confirmed { .. }
    ));
    // The confirmation over the checker's flag is a labelled example (§15).
    let examples = CheckerExamples::new(pool.clone()).list(None).await?;
    assert_eq!(examples.len(), 1);
    assert_eq!(examples[0].label_source, LabelSource::UserConfirm);
    assert_eq!(examples[0].label, Label::True);
    assert_eq!(examples[0].local_verdict, "unsupported");
    assert_eq!(examples[0].claim, figure(1));
    assert!(!examples[0].passages.is_empty());
    Ok(())
}

#[tokio::test]
async fn an_edit_is_applied_on_resume_and_only_its_section_is_extracted_again() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson_id) = batch_scheduler_tests::setup(&pool).await?;
    let sources = [source()];
    let model = Figures::new();
    let raw = lesson(&[GOOD, BAD]).to_string();
    assert!(
        prepare(&repo, &job, &lesson_id, &model, Some(raw), &sources)
            .await
            .is_err()
    );
    let store = ClaimReviewRepository::new(pool.clone());
    let ledger = store.load(&lesson_id).await?;
    let edited = waiting(&ledger.claims)[0];
    assert_eq!(edited.location().unit_index, 1);
    let edited = edited.claim_id().clone();
    ClaimReviewService::new(pool.clone())
        .resolve(
            &lesson_id,
            &edited.0,
            ClaimActionRequestDto::Edit {
                before: figure(1),
                after: GOOD.into(),
            },
            ledger.revision.0,
        )
        .await?;

    let extractions = model.extracted.lock().unwrap().len();
    let (raw, report) = prepare(&repo, &job, &lesson_id, &model, None, &sources).await?;
    let lesson: Value = serde_json::from_str(&raw)?;
    assert_eq!(lesson["blocks"][1]["body"], GOOD);
    assert_eq!(lesson["blocks"][0]["body"], GOOD);
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
    assert_eq!(lesson["blocks"][0]["body"], GOOD);
    assert_eq!(lesson["blocks"][1]["body"], BAD);
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    let confirmed = report.findings.iter().find(|f| f.unit == 1).unwrap();
    assert!(matches!(
        &confirmed.resolution,
        Some(general_knowledge::Resolution::Confirmed { note: Some(_), .. })
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
        "DictReader is the usual way to read CSV.",
    ];
    let kinds = [
        VerdictKind::Unsupported,
        VerdictKind::Unsupported,
        VerdictKind::Contradicted,
        VerdictKind::Unsupported,
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
        ClaimActionRequestDto::GeneralKnowledge,
    ];
    for (id, action) in ids.iter().zip(actions) {
        revision = service.resolve(&lesson_id, &id.0, action, revision).await?;
    }
    let examples = CheckerExamples::new(pool.clone());
    let labelled = examples.list(Some(&program_id)).await?;
    // General knowledge is not scored; each other verdict is one example.
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
