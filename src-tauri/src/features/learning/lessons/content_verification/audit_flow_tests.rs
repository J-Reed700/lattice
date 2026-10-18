//! The Verifier audit inside a preparation (§14.3), over the claim review's
//! end-to-end harness: the audit runs only once every claim is resolved and
//! before the lesson publishes; a dispute holds the lesson for review with
//! both checkers' verdicts on the card; without consent nothing is sent and
//! the lesson waits with the reason, and Resume needs no claim change.

use super::claim_ledger_tests::{prepare, waiting};
use super::claim_review_flow_tests::{Supporting, Waiting};
use super::*;
use crate::features::learning::curriculum_repository::ProgramVerifierAudit;
use crate::features::learning::lessons::claim_reviews::dto::{
    CheckResultDto, CheckerRoleDto, ClaimActionRequestDto,
};
use crate::features::learning::lessons::claim_reviews::repository::ClaimReviewRepository;
use crate::features::learning::lessons::verifier_audit::lesson::{self, VerifierAccess};
use crate::features::learning::lessons::verifier_audit::tools::VerifierWeb;
use async_trait::async_trait;
use lattice_core::application::ports::llm_port::{ModelOrigin, PinnedModel, ResolvedModel};
use lattice_core::application::ports::ClaimReviewStore;
use lattice_core::domain::learning::claim_review::{
    Checker, ClaimEvent, ClaimState, LessonReviewState, ReopenCause, VerdictKind,
};
use lattice_core::domain::models::model_ref::{CloudProvider, ModelRef};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

const VERIFIER_WINDOW: usize = 200_000;
const NO_PASSAGE: &str = "No passage states this.";

/// A Verifier that gives every claim `verdict`: `supported` quotes the first
/// sentence of the claim's first passage (could not verify without one),
/// anything else quotes nothing. Counts its requests.
struct AuditingModel {
    verdict: &'static str,
    calls: AtomicUsize,
}

impl AuditingModel {
    fn new(verdict: &'static str) -> Arc<Self> {
        Arc::new(Self {
            verdict,
            calls: AtomicUsize::new(0),
        })
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }

    fn answer(&self, task: &str) -> String {
        let lines: Vec<&str> = task.lines().collect();
        let passage = |label: &str| {
            lines
                .iter()
                .position(|line| line.starts_with(&format!("[{label}] ")))
                .and_then(|index| lines.get(index + 1))
                .map(|text| {
                    text.split_inclusive('.')
                        .next()
                        .unwrap_or_default()
                        .trim()
                        .to_owned()
                })
        };
        let mut verdicts = Vec::new();
        for (index, line) in lines.iter().enumerate() {
            let Some(label) = line
                .strip_prefix("[C")
                .and_then(|rest| rest.split_once("] "))
                .map(|(number, _)| format!("C{number}"))
            else {
                continue;
            };
            let cited = lines[index..]
                .iter()
                .find_map(|line| line.strip_prefix("Passages: "))
                .and_then(|cited| cited.split(", ").next())
                .filter(|first| first.starts_with('P') || first.starts_with('W'))
                .and_then(|first| passage(first).map(|quote| (first.to_owned(), quote)));
            verdicts.push(match (self.verdict, cited) {
                ("supported", Some((passage, quote))) => json!({
                    "claim": label,
                    "passagesUsed": [{"passage": passage, "quote": quote}],
                    "verdict": "supported",
                    "reason": "The passage states this.",
                }),
                ("supported", None) => json!({
                    "claim": label,
                    "passagesUsed": [],
                    "verdict": "could_not_verify",
                    "reason": "No passage was supplied.",
                }),
                (verdict, _) => json!({
                    "claim": label,
                    "passagesUsed": [],
                    "verdict": verdict,
                    "reason": NO_PASSAGE,
                }),
            });
        }
        json!({ "verdicts": verdicts }).to_string()
    }
}

#[async_trait]
impl LLMPort for AuditingModel {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let mut response = CompletionResponse::from_text(self.answer(request.user_text()));
        response.input_tokens = 1_000;
        response.output_tokens = 50;
        Ok(response)
    }
    fn model_name(&self) -> &str {
        "verifier-model"
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len().div_ceil(4)
    }
    fn max_context_tokens(&self) -> usize {
        VERIFIER_WINDOW
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// The Verifier as Settings would hold it, without web tools.
struct FakeVerifier {
    model: ModelRef,
    llm: Arc<AuditingModel>,
}

impl FakeVerifier {
    fn local(llm: &Arc<AuditingModel>) -> Arc<dyn VerifierAccess> {
        Arc::new(Self {
            model: ModelRef::Local {
                model_id: "verifier-9b".into(),
            },
            llm: Arc::clone(llm),
        })
    }

    fn cloud(llm: &Arc<AuditingModel>) -> Arc<dyn VerifierAccess> {
        Arc::new(Self {
            model: ModelRef::Cloud {
                provider: CloudProvider::Anthropic,
                model: "claude-x".into(),
            },
            llm: Arc::clone(llm),
        })
    }
}

#[async_trait]
impl VerifierAccess for FakeVerifier {
    async fn model(&self) -> Result<Option<ModelRef>> {
        Ok(Some(self.model.clone()))
    }
    async fn pin(&self) -> Result<PinnedModel> {
        Ok(PinnedModel {
            resolved: ResolvedModel {
                model: self.model.clone(),
                origin: ModelOrigin::VerifierRole,
            },
            llm: self.llm.clone(),
            instance_id: 0,
            degraded: None,
        })
    }
    fn web(&self, _program_id: &str) -> Option<Arc<dyn VerifierWeb>> {
        None
    }
}

async fn opt_in(course: &Waiting, on: bool) -> Result<()> {
    course
        .jobs
        .set_verifier_audit(
            &course.program_id,
            &ProgramVerifierAudit {
                on,
                ..ProgramVerifierAudit::default()
            },
        )
        .await
}

/// Confirms the claim waiting for the user and resumes: the continued job.
async fn confirm_and_resume(course: &Waiting) -> Result<String> {
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
    Ok(course.resume().await?.id)
}

/// Claims the attempt as the runtime would and runs its pass under
/// `verifier` up to publication. True when the lesson is held for review.
async fn held_by_pass(
    course: &Waiting,
    model: &Supporting,
    job_id: &str,
    verifier: Arc<dyn VerifierAccess>,
) -> Result<bool> {
    assert!(JobStore::new(course.pool.clone())
        .claim(job_id)
        .await?
        .is_some());
    let references = course.references().await?;
    let prepared = lesson::scope(
        Some(verifier),
        prepare(
            &course.jobs,
            job_id,
            &course.lesson_id,
            model,
            None,
            &references,
        ),
    )
    .await;
    let held = held_for_review(&course.pool, &course.lesson_id, job_id)
        .await?
        .is_some();
    assert_eq!(prepared.is_err(), held, "{:?}", prepared.as_ref().err());
    Ok(held)
}

/// Completes a held attempt as the worker does (§6.1).
async fn complete_held(course: &Waiting, job_id: &str) -> Result<()> {
    let held = held_for_review(&course.pool, &course.lesson_id, job_id)
        .await?
        .expect("the lesson is held for review");
    let (result_ref, message) = review_outcome(&[held]);
    assert!(
        JobStore::new(course.pool.clone())
            .complete(job_id, &result_ref, &message)
            .await?
    );
    Ok(())
}

fn verifier_checks(ledger: &lattice_core::domain::learning::claim_review::Ledger) -> usize {
    ledger
        .claims
        .iter()
        .flat_map(|entry| entry.events())
        .filter(|record| {
            matches!(
                &record.event,
                ClaimEvent::Checked(check) if matches!(check.checker, Checker::Verifier { .. })
            )
        })
        .count()
}

#[tokio::test]
async fn the_audit_runs_once_every_claim_is_resolved_then_the_lesson_publishes() -> Result<()> {
    let model = Supporting::new();
    let verifier = AuditingModel::new("supported");
    // Opted in from the start: the pass that leaves a claim for the user
    // never reaches the audit.
    let course = Waiting::course().await?;
    opt_in(&course, true).await?;
    lesson::scope(Some(FakeVerifier::local(&verifier)), course.hold(&model)).await?;
    assert_eq!(verifier.calls(), 0, "a claim still waits for the user");
    let job_id = confirm_and_resume(&course).await?;
    lesson::scope(
        Some(FakeVerifier::local(&verifier)),
        course.run_and_publish(&model, &job_id),
    )
    .await?;
    assert!(verifier.calls() > 0, "the resolved lesson is audited");
    let ledger = ClaimReviewRepository::new(course.pool.clone())
        .load(&course.lesson_id)
        .await?;
    assert!(verifier_checks(&ledger) > 0);
    course.assert_published(&job_id).await
}

#[tokio::test]
async fn a_dispute_holds_the_lesson_with_both_checkers_on_the_card() -> Result<()> {
    let model = Supporting::new();
    let course = Waiting::new(&model).await?;
    opt_in(&course, true).await?;
    let job_id = confirm_and_resume(&course).await?;
    let verifier = AuditingModel::new("unsupported");

    let held = held_by_pass(&course, &model, &job_id, FakeVerifier::local(&verifier)).await?;
    assert!(held, "a disputed lesson does not publish");
    assert!(verifier.calls() > 0);
    let ledger = ClaimReviewRepository::new(course.pool.clone())
        .load(&course.lesson_id)
        .await?;
    let review = ledger.review.clone().unwrap();
    assert_eq!(review.state, LessonReviewState::NeedsReview);
    assert_eq!(review.stop_reason, None, "a dispute is not a stop");
    assert!(ledger.claims.iter().any(|entry| matches!(
        entry.state(),
        ClaimState::Queued {
            kind: VerdictKind::Disputed { .. },
            ..
        } | ClaimState::Reopened {
            cause: ReopenCause::VerifierDispute,
            ..
        }
    )));

    let card = course
        .service()
        .review(&course.program_id, &course.lesson_id)
        .await?;
    let disputed = card
        .claims
        .iter()
        .find(|item| item.verifier_check.is_some())
        .expect("the Verifier's verdict is on the card");
    let said = disputed.verifier_check.as_ref().unwrap();
    assert!(matches!(said.checker.role, CheckerRoleDto::Verifier));
    assert!(matches!(said.result, CheckResultDto::Failed { .. }));
    assert_eq!(said.reason, NO_PASSAGE);
    let local = disputed
        .local_check
        .as_ref()
        .expect("the local checker's verdict beside it");
    assert!(matches!(local.checker.role, CheckerRoleDto::Local));
    Ok(())
}

#[tokio::test]
async fn without_consent_nothing_is_sent_and_resume_needs_no_claim_change() -> Result<()> {
    let model = Supporting::new();
    let course = Waiting::new(&model).await?;
    // On, but sending lesson text to the cloud provider was never confirmed.
    opt_in(&course, true).await?;
    let job_id = confirm_and_resume(&course).await?;
    let verifier = AuditingModel::new("supported");

    let held = held_by_pass(&course, &model, &job_id, FakeVerifier::cloud(&verifier)).await?;
    assert!(held, "the audit is on and cannot run");
    assert_eq!(verifier.calls(), 0, "no consent, no call");
    let review = ClaimReviewRepository::new(course.pool.clone())
        .load(&course.lesson_id)
        .await?
        .review
        .unwrap();
    assert_eq!(review.state, LessonReviewState::NeedsReview);
    assert_eq!(
        review.stop_reason.map(|stop| stop.code).as_deref(),
        Some("verifier_consent_missing")
    );

    // The fix is the course's switch, not a claim: Resume is not refused.
    complete_held(&course, &job_id).await?;
    opt_in(&course, false).await?;
    let next = course.resume().await?;
    lesson::scope(
        Some(FakeVerifier::cloud(&verifier)),
        course.run_and_publish(&model, &next.id),
    )
    .await?;
    assert_eq!(verifier.calls(), 0, "opted out: no audit gate");
    course.assert_published(&next.id).await
}
