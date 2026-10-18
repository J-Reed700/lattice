//! The Verifier audit against a scripted Verifier, an in-memory ledger and
//! in-memory audit rows: what reaches the ledger, receipts, batching and the
//! answer handling. The §14.6 failure modes are in `failure_tests.rs`.

#![cfg(test)]

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::{json, Value};

use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use lattice_core::domain::learning::claim_review::{
    text_sha256, Actor, Check, CheckResult, ClaimLedgerEntry, ClaimLocation, EvidenceSeen,
    HumanBinding, LessonReview, LessonReviewState, Revision, UnitKind, UserAction,
};
use lattice_core::domain::models::model_ref::{CloudProvider, ModelRef};

use super::runs::AuditRun;
use super::*;
use lattice_core::domain::learning::claim_review::{
    Checker, ClaimEvent, ClaimState, EventProvenance, EvidenceFingerprint, Ledger, ReopenCause,
    VerdictKind,
};

#[path = "answer_tests.rs"]
mod answers;
#[path = "failure_tests.rs"]
mod failures;

pub(super) const LESSON: &str = "lesson-1";
pub(super) const LOCAL: &str = "qwen3.5-27b";
pub(super) const UNIT_TEXT: &str = "Water is everywhere.\n\nWater boils at 100 °C at sea level. \
                                    Salt raises the boiling point.\n\nThen it cools.";
pub(super) const BIG: usize = 200_000;

pub(super) fn verifier_ref() -> ModelRef {
    ModelRef::Cloud {
        provider: CloudProvider::Anthropic,
        model: "claude-x".into(),
    }
}

pub(super) fn verifier_name() -> String {
    verifier_ref().to_string()
}

fn machine(at: i64) -> EventProvenance {
    EventProvenance {
        actor: Actor::Machine,
        at,
        job_id: Some("job-1".into()),
        app_version: "1.0.0".into(),
        checker_model: Some(LOCAL.into()),
    }
}

fn user(at: i64) -> EventProvenance {
    EventProvenance {
        actor: Actor::User,
        job_id: None,
        ..machine(at)
    }
}

/// A claim whose quote is `statement` itself, in unit 0.
pub(super) fn extracted(id: &str, statement: &str) -> ClaimLedgerEntry {
    ClaimLedgerEntry::extracted(
        ClaimId(id.into()),
        machine(1),
        ClaimLocation::locate(0, UnitKind::Block, "body", statement, UNIT_TEXT),
        statement.into(),
    )
    .unwrap()
}

fn local_check(result: CheckResult, version: &str) -> ClaimEvent {
    let evidence = vec![EvidenceSeen {
        source_version_id: version.into(),
        start_byte: 0,
        end_byte: 10,
        text_sha256: text_sha256(version),
    }];
    ClaimEvent::Checked(Check {
        checker: Checker::Local {
            model: LOCAL.into(),
        },
        result,
        reason: "local".into(),
        fingerprint: EvidenceFingerprint::of("strict-v1", LOCAL, &evidence),
        evidence,
        window_tokens: 32_768,
    })
}

pub(super) fn verified(id: &str, statement: &str) -> ClaimLedgerEntry {
    let mut entry = extracted(id, statement);
    let supported = CheckResult::Supported {
        source_version_id: "v1".into(),
        supporting_quote: statement.into(),
    };
    entry
        .append(machine(2), local_check(supported, "v1"))
        .unwrap();
    entry
}

pub(super) fn confirmed(id: &str, statement: &str) -> ClaimLedgerEntry {
    let mut entry = extracted(id, statement);
    let failed = CheckResult::Failed {
        kind: VerdictKind::CouldNotJudge,
    };
    entry.append(machine(2), local_check(failed, "v1")).unwrap();
    let action = UserAction::Confirm {
        bound: HumanBinding::new(statement, vec!["v1".into()]),
    };
    entry
        .append(
            user(3),
            ClaimEvent::UserAction {
                action,
                note: Some("I checked the handbook".into()),
            },
        )
        .unwrap();
    entry
}

pub(super) fn general_knowledge(id: &str, statement: &str) -> ClaimLedgerEntry {
    let mut entry = extracted(id, statement);
    let proposed = CheckResult::Failed {
        kind: VerdictKind::GeneralKnowledgeProposed,
    };
    entry
        .append(machine(2), local_check(proposed, "v1"))
        .unwrap();
    entry
        .append(
            user(3),
            ClaimEvent::UserAction {
                action: UserAction::GeneralKnowledge,
                note: None,
            },
        )
        .unwrap();
    entry
}

pub(super) fn passage(version: &str, text: &str) -> AuditPassage {
    AuditPassage {
        source_version_id: version.into(),
        title: format!("Handbook {version}"),
        start_byte: 0,
        end_byte: text.len(),
        text: text.into(),
    }
}

pub(super) fn units() -> Vec<AuditUnit> {
    vec![AuditUnit {
        index: 0,
        title: "Boiling".into(),
        text: UNIT_TEXT.into(),
    }]
}

/// The ledger the job left: every claim resolved.
pub(super) struct MemoryLedger {
    pub ledger: Mutex<Ledger>,
    /// The next append finds the review moved on: this user event landed
    /// on this claim first.
    pub interleave: Mutex<Option<(ClaimId, ClaimEvent)>>,
}

impl MemoryLedger {
    pub(super) fn new(claims: Vec<ClaimLedgerEntry>) -> Self {
        Self {
            ledger: Mutex::new(Ledger {
                lesson_id: LESSON.into(),
                review: Some(LessonReview {
                    program_id: "program-1".into(),
                    job_id: "job-1".into(),
                    state: LessonReviewState::Verifying,
                    draft_sha256: "draft-1".into(),
                    checker_model: LOCAL.into(),
                    stop_reason: None,
                }),
                revision: Revision(1),
                claims,
            }),
            interleave: Mutex::new(None),
        }
    }

    pub(super) fn state(&self, id: &str) -> ClaimState {
        self.entry(id).state().clone()
    }

    pub(super) fn entry(&self, id: &str) -> ClaimLedgerEntry {
        let ledger = self.ledger.lock().unwrap();
        ledger
            .claims
            .iter()
            .find(|entry| entry.claim_id().0 == id)
            .cloned()
            .unwrap()
    }

    /// The claim's Verifier checks, oldest first.
    pub(super) fn verifier_checks(&self, id: &str) -> Vec<Check> {
        self.entry(id)
            .events()
            .iter()
            .filter_map(|recorded| match &recorded.event {
                ClaimEvent::Checked(check) if matches!(check.checker, Checker::Verifier { .. }) => {
                    Some(check.clone())
                }
                _ => None,
            })
            .collect()
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
        if let Some((target, first)) = self.interleave.lock().unwrap().take() {
            let entry = ledger
                .claims
                .iter_mut()
                .find(|entry| *entry.claim_id() == target)
                .unwrap();
            entry.append(user(50), first).map_err(AppError::from)?;
            ledger.revision.0 += 1;
        }
        if expected_revision != ledger.revision {
            return Err(AppError::InvalidState("the review moved on".into()));
        }
        let entry = ledger
            .claims
            .iter_mut()
            .find(|entry| entry.claim_id() == claim_id)
            .ok_or_else(|| AppError::NotFound("no such claim".into()))?;
        entry.append(provenance, event).map_err(AppError::from)?;
        ledger.revision.0 += 1;
        Ok(ledger.revision)
    }

    async fn set_lesson_review(&self, _lesson_id: &str, review: LessonReview) -> Result<Revision> {
        let mut ledger = self.ledger.lock().unwrap();
        ledger.review = Some(review);
        ledger.revision.0 += 1;
        Ok(ledger.revision)
    }
}

#[derive(Default)]
pub(super) struct MemoryRuns {
    pub runs: Mutex<HashMap<(String, String, String), AuditRun>>,
}

impl MemoryRuns {
    pub(super) fn only(&self) -> AuditRun {
        let runs = self.runs.lock().unwrap();
        assert_eq!(runs.len(), 1, "one audit row");
        runs.values().next().cloned().unwrap()
    }
}

fn run_key(key: &RunKey) -> (String, String, String) {
    (
        key.lesson_id.clone(),
        key.draft_sha256.clone(),
        key.verifier_model.clone(),
    )
}

#[async_trait]
impl AuditRunStore for MemoryRuns {
    async fn begin(&self, key: &RunKey, policy: &str, at: i64) -> Result<AuditRun> {
        let mut runs = self.runs.lock().unwrap();
        let run = runs.entry(run_key(key)).or_insert_with(|| AuditRun {
            key: key.clone(),
            policy: policy.into(),
            state: RunState::Running,
            batches: Vec::new(),
            input_tokens: 0,
            output_tokens: 0,
            started_at: at,
            finished_at: None,
        });
        run.state = RunState::Running;
        run.finished_at = None;
        Ok(run.clone())
    }

    async fn record(&self, key: &RunKey, batch: &BatchRecord) -> Result<()> {
        let mut runs = self.runs.lock().unwrap();
        let run = runs.get_mut(&run_key(key)).unwrap();
        run.input_tokens += batch.input_tokens;
        run.output_tokens += batch.output_tokens;
        run.batches.push(batch.clone());
        Ok(())
    }

    async fn finish(&self, key: &RunKey, state: RunState, at: i64) -> Result<()> {
        let mut runs = self.runs.lock().unwrap();
        let run = runs.get_mut(&run_key(key)).unwrap();
        run.state = state;
        run.finished_at = Some(at);
        Ok(())
    }
}

/// Answers the gate's checks in order; open once the script runs out.
#[derive(Default)]
pub(super) struct ScriptedGate {
    pub stops: Mutex<VecDeque<Option<AuditStop>>>,
}

#[async_trait]
impl AuditGate for ScriptedGate {
    async fn check(&self) -> Result<Option<AuditStop>> {
        Ok(self.stops.lock().unwrap().pop_front().flatten())
    }
}

type Reply = Box<dyn Fn(&CompletionRequest, usize) -> Result<CompletionResponse> + Send + Sync>;

/// A Verifier whose answers a test writes, counting four characters a token.
pub(super) struct ScriptedVerifier {
    pub window: usize,
    pub reply: Reply,
    pub sent: Mutex<Vec<CompletionRequest>>,
}

impl ScriptedVerifier {
    pub(super) fn new(
        window: usize,
        reply: impl Fn(&CompletionRequest, usize) -> Result<CompletionResponse> + Send + Sync + 'static,
    ) -> Self {
        Self {
            window,
            reply: Box::new(reply),
            sent: Mutex::new(Vec::new()),
        }
    }

    /// Answers every claim of every request with `verdict(statement, passages)`.
    pub(super) fn answering(
        window: usize,
        verdict: impl Fn(&str, &[Labelled]) -> Value + Send + Sync + 'static,
    ) -> Self {
        Self::new(window, move |request, _| {
            Ok(usage(CompletionResponse::from_text(answer_each(
                request, &verdict,
            ))))
        })
    }

    pub(super) fn calls(&self) -> usize {
        self.sent.lock().unwrap().len()
    }

    pub(super) fn user_texts(&self) -> Vec<String> {
        self.sent
            .lock()
            .unwrap()
            .iter()
            .map(|request| request.user_text().to_owned())
            .collect()
    }
}

#[async_trait]
impl LLMPort for ScriptedVerifier {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let call = {
            let mut sent = self.sent.lock().unwrap();
            sent.push(request.clone());
            sent.len()
        };
        (self.reply)(request, call)
    }
    fn model_name(&self) -> &str {
        "claude-x"
    }
    fn max_context_tokens(&self) -> usize {
        self.window
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len().div_ceil(4)
    }
    fn supports_tool_calling(&self) -> bool {
        true
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

pub(super) fn usage(mut response: CompletionResponse) -> CompletionResponse {
    response.input_tokens = 1_000;
    response.output_tokens = 50;
    response
}

/// A claim or passage of a rendered request: `(label, text)`.
pub(super) type Labelled = (String, String);

/// The request's claims as `(label, statement)` and passages as
/// `(label, text)`, read back from the rendered task.
pub(super) fn parse_request(user_text: &str) -> (Vec<Labelled>, Vec<Labelled>) {
    let lines: Vec<&str> = user_text.lines().collect();
    let mut claims = Vec::new();
    let mut passages = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let Some(rest) = line.strip_prefix('[') else {
            continue;
        };
        let Some((label, tail)) = rest.split_once("] ") else {
            continue;
        };
        if label.starts_with('C') {
            claims.push((label.to_owned(), tail.to_owned()));
        } else if label.starts_with('P') {
            let text = lines.get(index + 1).copied().unwrap_or_default();
            passages.push((label.to_owned(), text.to_owned()));
        }
    }
    (claims, passages)
}

/// The statements of the claims a request carried (its section text holds
/// other claims' wording too).
pub(super) fn sent_claims(user_text: &str) -> Vec<String> {
    parse_request(user_text)
        .0
        .into_iter()
        .map(|(_, statement)| statement)
        .collect()
}

/// An answer giving each claim `verdict(statement, passages)`, which
/// returns `{verdict, reason?, passagesUsed?}`.
pub(super) fn answer_each(
    request: &CompletionRequest,
    verdict: &dyn Fn(&str, &[Labelled]) -> Value,
) -> String {
    let (claims, passages) = parse_request(request.user_text());
    let verdicts: Vec<Value> = claims
        .iter()
        .map(|(label, statement)| {
            let mut answer = verdict(statement, &passages);
            answer["claim"] = json!(label);
            if answer.get("reason").is_none() {
                answer["reason"] = json!("judged on the passages");
            }
            if answer.get("passagesUsed").is_none() {
                answer["passagesUsed"] = json!([]);
            }
            answer
        })
        .collect();
    json!({ "verdicts": verdicts }).to_string()
}

/// "supported", quoting the first passage's first sentence.
pub(super) fn supports(_: &str, passages: &[Labelled]) -> Value {
    let (label, text) = passages.first().cloned().unwrap_or_default();
    let quote = text
        .split_inclusive('.')
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned();
    json!({"verdict": "supported", "passagesUsed": [{"passage": label, "quote": quote}]})
}

pub(super) struct Harness {
    pub store: MemoryLedger,
    pub runs: MemoryRuns,
    pub gate: ScriptedGate,
    pub progress: Mutex<Vec<String>>,
    pub passages: HashMap<ClaimId, Vec<AuditPassage>>,
    pub units: Vec<AuditUnit>,
    pub draft: String,
}

impl Harness {
    pub(super) fn new(claims: Vec<ClaimLedgerEntry>) -> Self {
        let passages = claims
            .iter()
            .map(|entry| {
                let text = format!("{} So the handbook says.", entry.statement());
                (entry.claim_id().clone(), vec![passage("v1", &text)])
            })
            .collect();
        Self {
            store: MemoryLedger::new(claims),
            runs: MemoryRuns::default(),
            gate: ScriptedGate::default(),
            progress: Mutex::new(Vec::new()),
            passages,
            units: units(),
            draft: "draft-1".into(),
        }
    }

    pub(super) async fn audit(
        &self,
        llm: &dyn LLMPort,
        web: Option<&dyn VerifierWeb>,
    ) -> Result<AuditOutcome> {
        let model = verifier_ref();
        let request = AuditRequest {
            lesson_id: LESSON,
            job_id: Some("job-1"),
            app_version: "1.0.0",
            draft_sha256: &self.draft,
            verifier_model: &model,
            units: &self.units,
            passages: &self.passages,
        };
        let progress = |step: AuditProgress| self.progress.lock().unwrap().push(step.label());
        let clock = || 1_000;
        let deps = AuditDeps {
            store: &self.store,
            runs: &self.runs,
            gate: &self.gate,
            web,
            progress: &progress,
            clock: &clock,
        };
        run(llm, &request, &deps).await
    }
}

const BOILS: &str = "Water boils at 100 °C at sea level.";
const SALT: &str = "Salt raises the boiling point.";

#[tokio::test]
async fn agreement_records_a_verifier_check_and_the_lesson_may_publish() {
    let harness = Harness::new(vec![verified("a", BOILS)]);
    let llm = ScriptedVerifier::answering(BIG, supports);

    let outcome = harness.audit(&llm, None).await.unwrap();

    assert!(matches!(&outcome, AuditOutcome::Completed(summary) if summary.disputes == 0));
    assert!(outcome.publishable());
    assert!(matches!(
        harness.store.state("a"),
        ClaimState::ResolvedByEvidence { .. }
    ));
    let checks = harness.store.verifier_checks("a");
    assert_eq!(checks.len(), 1);
    assert_eq!(
        checks[0].checker,
        Checker::Verifier {
            model: verifier_name()
        }
    );
    assert!(matches!(checks[0].result, CheckResult::Supported { .. }));
    assert_eq!(checks[0].window_tokens, BIG);
    let run = harness.runs.only();
    assert_eq!(run.state, RunState::Completed);
    assert_eq!(run.policy, AUDIT_POLICY_ID);
    assert_eq!((run.input_tokens, run.output_tokens), (1_000, 50));
    assert_eq!(
        harness.progress.lock().unwrap().as_slice(),
        ["Audit batch 1 of 1"]
    );
}

#[tokio::test]
async fn what_is_sent_is_the_section_the_claims_and_their_passages_only() {
    let harness = Harness::new(vec![verified("a", BOILS), verified("b", SALT)]);
    let llm = ScriptedVerifier::answering(BIG, supports);

    harness.audit(&llm, None).await.unwrap();

    let sent = llm.sent.lock().unwrap();
    assert_eq!(sent.len(), 1, "both claims fit one batch");
    let request = &sent[0];
    let text = request.user_text();
    assert_eq!(text.matches("[S0] Boiling").count(), 1, "the section once");
    assert!(text.contains(BOILS) && text.contains(SALT));
    assert!(text.contains("So the handbook says."));
    assert!(request.json_schema.is_some(), "held to the verdict schema");
    assert!(request.tools.is_empty(), "no web tools unless allowed");
}

#[tokio::test]
async fn a_contradiction_on_a_verified_claim_is_a_dispute_that_blocks_publishing() {
    let harness = Harness::new(vec![verified("a", BOILS)]);
    let llm = ScriptedVerifier::answering(BIG, |_, passages| {
        json!({"verdict": "contradicted", "reason": "the handbook gives another figure",
               "passagesUsed": [{"passage": passages[0].0, "quote": "So the handbook says."}]})
    });

    let outcome = harness.audit(&llm, None).await.unwrap();

    assert_eq!(outcome.summary().disputes, 1);
    assert!(!outcome.publishable());
    match harness.store.state("a") {
        ClaimState::Queued {
            kind: VerdictKind::Disputed { verifier },
            reason,
        } => {
            assert_eq!(*verifier, VerdictKind::Contradicted);
            assert!(reason.contains("\"So the handbook says.\""), "{reason}");
        }
        other => panic!("not disputed: {other:?}"),
    }
}

#[tokio::test]
async fn a_dispute_reopens_a_human_confirmation() {
    let harness = Harness::new(vec![confirmed("a", BOILS)]);
    let llm = ScriptedVerifier::answering(
        BIG,
        |_, _| json!({"verdict": "unsupported", "reason": "nothing says so"}),
    );

    let outcome = harness.audit(&llm, None).await.unwrap();

    assert_eq!(outcome.summary().disputes, 1);
    assert!(matches!(
        harness.store.state("a"),
        ClaimState::Reopened {
            cause: ReopenCause::VerifierDispute,
            ..
        }
    ));
}

#[tokio::test]
async fn general_knowledge_is_audited_for_contradiction_only() {
    let mut harness = Harness::new(vec![general_knowledge("g", SALT)]);
    harness.passages.clear();
    let llm = ScriptedVerifier::answering(
        BIG,
        |_, _| json!({"verdict": "could_not_verify", "reason": "no passage"}),
    );

    let outcome = harness.audit(&llm, None).await.unwrap();

    assert!(outcome.publishable());
    assert!(matches!(
        harness.store.state("g"),
        ClaimState::GeneralKnowledge { .. }
    ));
    let text = llm.user_texts().remove(0);
    assert!(text.contains("accepted as general knowledge: check it for contradiction only"));
}

#[tokio::test]
async fn a_verdict_from_memory_counts_as_could_not_verify() {
    let harness = Harness::new(vec![verified("a", BOILS), general_knowledge("g", SALT)]);
    let llm = ScriptedVerifier::answering(BIG, |statement, passages| {
        if statement == SALT {
            // No passage to quote: a contradiction from memory.
            json!({"verdict": "contradicted", "reason": "everyone knows"})
        } else {
            json!({"verdict": "supported", "reason": "I recall it",
                   "passagesUsed": [{"passage": passages[0].0, "quote": "Water boils at 90 °C."}]})
        }
    });

    harness.audit(&llm, None).await.unwrap();

    match harness.store.state("a") {
        ClaimState::Queued {
            kind: VerdictKind::Disputed { verifier },
            reason,
        } => {
            assert_eq!(*verifier, VerdictKind::CouldNotJudge);
            assert!(
                reason.contains("quoted no passage it was given"),
                "{reason}"
            );
        }
        other => panic!("a memory verdict must not verify: {other:?}"),
    }
    assert!(
        matches!(
            harness.store.state("g"),
            ClaimState::GeneralKnowledge { .. }
        ),
        "an ungrounded contradiction is no dispute"
    );
}

#[tokio::test]
async fn audited_claims_keep_their_receipt_until_their_passages_change() {
    let mut harness = Harness::new(vec![verified("a", BOILS), verified("b", SALT)]);
    let llm = ScriptedVerifier::answering(BIG, supports);
    harness.audit(&llm, None).await.unwrap();
    assert_eq!(llm.calls(), 1);

    let again = harness.audit(&llm, None).await.unwrap();
    assert_eq!(llm.calls(), 1, "nothing is sent again");
    assert!(matches!(&again, AuditOutcome::Completed(summary) if summary.batches_planned == 0));

    harness.passages.insert(
        ClaimId("b".into()),
        vec![passage(
            "v2",
            "Salt raises the boiling point. A new edition.",
        )],
    );
    harness.audit(&llm, None).await.unwrap();
    assert_eq!(llm.calls(), 2);
    let text = llm.user_texts().pop().unwrap();
    assert_eq!(sent_claims(&text), [SALT], "only the changed claim");
    assert_eq!(harness.store.verifier_checks("a").len(), 1);
    assert_eq!(harness.store.verifier_checks("b").len(), 2);
}

#[tokio::test]
async fn a_claim_still_waiting_for_the_user_is_not_sent() {
    let mut queued = extracted("q", SALT);
    let failed = CheckResult::Failed {
        kind: VerdictKind::CouldNotJudge,
    };
    queued
        .append(machine(2), local_check(failed, "v1"))
        .unwrap();
    let harness = Harness::new(vec![verified("a", BOILS), queued]);
    let llm = ScriptedVerifier::answering(BIG, supports);

    harness.audit(&llm, None).await.unwrap();

    assert_eq!(sent_claims(&llm.user_texts()[0]), [BOILS]);
}

#[tokio::test]
async fn a_user_action_meanwhile_is_not_overwritten() {
    let harness = Harness::new(vec![verified("a", BOILS)]);
    *harness.store.interleave.lock().unwrap() = Some((
        ClaimId("a".into()),
        ClaimEvent::UserAction {
            action: UserAction::Remove,
            note: None,
        },
    ));
    let llm = ScriptedVerifier::answering(
        BIG,
        |_, _| json!({"verdict": "unsupported", "reason": "nothing says so"}),
    );

    let outcome = harness.audit(&llm, None).await.unwrap();

    assert!(matches!(
        harness.store.state("a"),
        ClaimState::Removed { .. }
    ));
    assert!(harness.store.verifier_checks("a").is_empty());
    assert_eq!(outcome.summary().disputes, 0);
}

#[tokio::test]
async fn a_lesson_without_a_review_is_refused() {
    let harness = Harness::new(vec![verified("a", BOILS)]);
    harness.store.ledger.lock().unwrap().review = None;
    let llm = ScriptedVerifier::answering(BIG, supports);

    let refused = harness.audit(&llm, None).await;

    assert!(matches!(refused, Err(AppError::InvalidState(_))));
    assert_eq!(llm.calls(), 0);
}
