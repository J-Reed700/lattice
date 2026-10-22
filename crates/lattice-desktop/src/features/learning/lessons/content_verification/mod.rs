//! Fail-closed lesson publication checks. Evidence and execution reduce errors;
//! the claim inventory and its coverage audit are still fallible model judgments.
use crate::features::learning::dto::LearningSourceDto;
use crate::features::learning::dto::PreparedLearningLesson;
use crate::features::learning::embedded_runtime::BuiltinRuntimes;
use crate::features::learning::reference_collection::{ReferenceCollection, ReferencePassage};
use futures::{FutureExt, StreamExt, TryStreamExt};
use lattice_core::application::ports::{llm_port::SamplingOverride, LLMPort};
#[cfg(test)]
use lattice_core::application::services::claim_verification::ClaimEvidence;
use lattice_core::application::services::claim_verification::{
    CheckPolicy, ClaimChecker, ClaimJudgment, ClaimVerdict, JudgeOutcome,
};
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

mod citations;
mod claim_ledger;
use citations::{overlaps, section_citations, source_mentions, take_cited, Citations, CITED};
pub(in crate::features::learning) use claim_ledger::{
    field_text, held_for_review, held_lessons, review_outcome, unit_texts, HeldForReview,
};
mod coverage;
#[cfg(test)]
pub(in crate::features::learning) use coverage::live_mapping_fixture;
mod claim_reuse;
mod evidence_checks;
pub(in crate::features::learning) use evidence_checks::Reach;
#[cfg(test)]
pub(in crate::features::learning) use evidence_checks::ORDERS;
#[cfg(test)]
pub(in crate::features::learning) use evidence_checks::{
    page_budget, page_seconds, walk_pages, walk_standing_pages, Depth, WalkedPage,
};
use evidence_checks::{
    same_passage, Paging, SupportedElsewhere, SupportingPassage, TwoPass, CHECKER_OUTPUT_TOKENS,
};
#[cfg(test)]
use evidence_checks::{ContraryPassage, Pass, REACHES};
mod evidence_selection;
mod execution;
mod general_knowledge;
mod judge;
pub(in crate::features::learning) use judge::{
    judge_for_publication, judge_in_order, ranking, shown_of,
};
mod inventory;
mod inventory_checkpoint;
mod learner_view;
mod portability;
mod quote_claims;
#[cfg(test)]
pub(in crate::features::learning) use inventory::revisions::live_revision_fixture;
#[cfg(test)]
pub(in crate::features::learning) async fn live_inventory_fixture(
    llm: &dyn LLMPort,
    content: &[Value],
) -> Result<Value> {
    let pool = crate::features::learning::tests::pool().await?;
    let (_, job, _) = tests::batch_scheduler_tests::setup(&pool).await?;
    crate::features::learning::lesson_progress::run(
        &lattice_core::shared::runtime::jobs::JobContext::detached(
            &lattice_core::shared::runtime::jobs::JobStore::new(pool.clone()),
            &job,
        )
        .await?,
        async {
            Ok(serde_json::to_value(
                inventory::extract(llm, content).await?,
            )?)
        },
    )
    .await
}
mod passes;
use passes::{check_evidence, verify_with_references};
mod provenance_gate;
#[cfg(test)]
pub(in crate::features::learning) use provenance_gate::Judgements;
pub(in crate::features::learning) use provenance_gate::{
    gate_reason, outline_trust, relevel, weigh_claim, with_outline_trust, with_trust,
};
mod repair;
pub(in crate::features::learning) use repair::{confine, sentences, Protected};
mod repair_loop;
mod repair_work;
mod report;
pub(in crate::features::learning) mod research;
mod review_pass;
mod scrutiny;
mod section_checkpoints;
mod support_gate;
#[cfg(test)]
pub(crate) mod tests;
use inventory_checkpoint::{complete_inventory, extract_audited_inventory, valid_saved_coverage};
#[cfg(test)]
use repair_loop::repair_context;
#[cfg(test)]
pub(in crate::features::learning) use repair_loop::verify_and_repair;
pub(in crate::features::learning) use repair_loop::{
    has_pending_repair, verify_and_repair_with_references,
};
use repair_loop::{repair_fingerprint, PendingRepair};
#[cfg(test)]
use report::content_hash;
pub(in crate::features::learning) use report::{persist_report, sources_changed};
/// v21: the evidence challenge is gone; a supported verdict is weighed by
/// the provenance gate (source provenance §11) and passages carry their
/// source's title, domain and date.
const POLICY: &str = "lesson-evidence-v21";

/// The evidence policy checks are judged under: what a labelled checker
/// example records (§15).
pub(in crate::features::learning) fn evidence_policy() -> &'static str {
    POLICY
}
// Extraction is versioned independently of coverage and factual judgment.
// Its exact-content checkpoint contains no factual approvals to inherit.
const INVENTORY_POLICY: &str = "lesson-evidence-v7";

pub(in crate::features::learning) async fn normalize_example_fences(
    llm: &dyn LLMPort,
    raw: String,
) -> Result<String> {
    execution::label_unlabeled(llm, raw).await
}

pub(in crate::features::learning) fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
fn invalid(message: impl Into<String>) -> AppError {
    AppError::InvalidInput(message.into())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Claim {
    quote: String,
    statement: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UnitClaims {
    index: usize,
    claims: Vec<Claim>,
    non_factual_reason: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Inventory {
    units: Vec<UnitClaims>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Coverage {
    units: Vec<CoverageUnit>,
}

#[derive(Deserialize, Serialize)]
struct InventoryCheckpoint {
    policy: String,
    model: String,
    model_context: usize,
    content_sha256: String,
    inventory: Inventory,
    coverage: Coverage,
    /// Preserve extracted data across audit-policy changes, never approval.
    coverage_policy: Option<String>,
}
/// An assertion of one original passage that the fidelity audit found
/// unrepresented, located by its app-owned passage ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MissingAssertion {
    passage_id: String,
    statement: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CoverageUnit {
    index: usize,
    complete: bool,
    reason: String,
    /// Empty exactly when complete. The inventory adopts these as claims before
    /// any evidence check, so an audit finding never blocks the lesson by itself.
    missing_assertions: Vec<MissingAssertion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Finding {
    unit: usize,
    quote: String,
    statement: String,
    verdict: ClaimVerdict,
    reason: String,
    evidence: Vec<EvidencePassage>,
    supporting_quote: Option<String>,
    /// The user's resolution, set from the claim ledger at publication.
    #[serde(skip_serializing_if = "Option::is_none")]
    resolution: Option<general_knowledge::Resolution>,
    /// What the provenance gate weighed for a supported finding: the
    /// supports' levels for the claim's subject and the decision. Boxed: it
    /// is most of a finding's size and most findings have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    trust: Option<Box<provenance_gate::FindingTrust>>,
    /// What the two passes made of the claim (course evidence index §5.1):
    /// the pass and page that decided it, where pass B read, the passage it
    /// found when the section's citations gave no support, and a passage
    /// either pass found saying otherwise about a supported claim.
    #[serde(flatten)]
    two_pass: TwoPass,
}
/// Completed comparisons and evidence selections. Durable checkpoints retain
/// both across restarts; a selection is never itself a factual approval.
#[derive(Default)]
struct ClaimChecks {
    findings: HashMap<String, Finding>,
    /// Claims the ledger holds as resolved by evidence the user brought, by
    /// unit and statement: that check is their receipt (`claim_ledger`).
    settled: HashMap<(usize, String), Finding>,
    selections: std::sync::Mutex<HashMap<String, evidence_selection::Selection>>,
    research_scopes: std::sync::Mutex<HashMap<String, HashSet<(usize, String)>>>,
    prior_claims: std::sync::Mutex<HashMap<String, Vec<(usize, Claim)>>>,
    /// What the judge's answers have cost this job, per claim, across its
    /// passes: the answer size a pass B page is budgeted by
    /// (`evidence_checks::page_budget`).
    answers: std::sync::Mutex<evidence_checks::AnswerSize>,
    /// What researching one claim on the web cost when this job's last
    /// research round measured it: pass B's break-even weighs it before the
    /// ledger's latest (`evidence_checks::walk`, D5).
    research_cost: Option<std::time::Duration>,
}
type CheckedClaim = (usize, String, Finding, bool);

impl ClaimChecks {
    fn key(unit: usize, claim: &Claim, evidence: &[EvidencePassage]) -> String {
        // Ranking scores may change when the corpus grows. They are not judge
        // inputs; preserve the freshly retrieved metadata in the new report.
        let passages: Vec<_> = evidence
            .iter()
            .map(|p| (&p.source_id, &p.text, p.start_byte, p.end_byte))
            .collect();
        // Each decision is restricted to this claim and its assigned evidence,
        // including when requests share a bank of passages. Other targets are
        // never evidence. Fresh coverage and final content binding still run.
        digest(&json!({"unit":unit,"statement":claim.statement,"passages":passages}).to_string())
    }

    fn get(
        &self,
        key: &str,
        unit: usize,
        claim: &Claim,
        evidence: &[EvidencePassage],
    ) -> Option<Finding> {
        self.findings.get(key).cloned().map(|mut finding| {
            finding.unit = unit;
            finding.quote = claim.quote.clone();
            finding.statement = claim.statement.clone();
            // A decision on this evidence takes its fresh retrieval metadata;
            // one reached on a deeper page keeps that page's passages.
            let identity = |p: &EvidencePassage| {
                (
                    p.source_id.clone(),
                    p.start_byte,
                    p.end_byte,
                    p.text.clone(),
                )
            };
            if finding
                .evidence
                .iter()
                .map(identity)
                .eq(evidence.iter().map(identity))
            {
                finding.evidence = evidence.to_vec();
            }
            finding
        })
    }

    fn record(&mut self, key: String, finding: &Finding) {
        // A failed/incomplete call is never a reusable decision.
        if finding.verdict != ClaimVerdict::Unverified {
            self.findings.insert(key, finding.clone());
        }
    }
}

/// A complete strict judgment, saved only after parsing and checking source
/// quotes. Evidence is retrieved anew on resume,
/// rather than duplicating full reference bodies in every checkpoint.
#[derive(Serialize, Deserialize)]
struct ClaimReceipt {
    verdict: ClaimVerdict,
    reason: String,
    supporting_quote: Option<String>,
    #[serde(default)]
    interpretation_policy: Option<String>,
    /// The positions of the page's passages the judge was shown when the
    /// whole page did not fit its window; `None` when it saw them all. The
    /// finding names only those: a passage cut for size was never compared
    /// with the claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shown: Option<Vec<usize>>,
}

fn claim_receipt_key(llm: &dyn LLMPort, comparison: &str) -> String {
    format!(
        "claim-v1:{}",
        digest(
            &json!({
                "policy": POLICY, "model": llm.model_name(),
                "context": llm.max_context_tokens(), "comparison": comparison,
                "trust": lattice_core::domain::provenance::TRUST_POLICY_VERSION,
            })
            .to_string()
        )
    )
}

impl ClaimReceipt {
    fn finding(self, unit: usize, claim: &Claim, evidence: &[EvidencePassage]) -> Option<Finding> {
        let evidence = match &self.shown {
            Some(shown) => shown_of(evidence, shown),
            None => evidence.to_vec(),
        };
        let evidence = evidence.as_slice();
        if self.verdict == ClaimVerdict::Unverified
            || (self.verdict != ClaimVerdict::Supported
                && self.interpretation_policy.as_deref() != Some(
                    lattice_core::application::services::claim_verification::STRICT_INTERPRETATION_POLICY,
                ))
            || (self.verdict == ClaimVerdict::Supported
                && self.supporting_quote.as_ref().is_none_or(|q| {
                    q.trim().is_empty() || !evidence.iter().any(|p| p.text.contains(q))
                }))
        {
            return None;
        }
        Some(Finding {
            unit,
            quote: claim.quote.clone(),
            statement: claim.statement.clone(),
            verdict: self.verdict,
            reason: self.reason,
            evidence: evidence.to_vec(),
            supporting_quote: self.supporting_quote,
            resolution: None,
            two_pass: Default::default(),
            trust: None,
        })
    }
}
type EvidencePassage = ReferencePassage;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SourceBinding {
    id: String,
    sha256: String,
}

/// Internal only: answer-key claims and execution details must not leak through
/// learner DTOs before submission. Fields cannot be authored by the generator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LessonVerificationReport {
    policy: String,
    lesson_id: String,
    content_sha256: String,
    checker_model: String,
    checked_at: i64,
    sources: Vec<SourceBinding>,
    coverage: Vec<UnitClaims>,
    coverage_audit: Vec<CoverageUnit>,
    findings: Vec<Finding>,
    executions: Vec<execution::Observation>,
    /// Failed examples the user confirmed, set from the claim ledger.
    #[serde(skip_serializing_if = "Option::is_none")]
    confirmed_examples: Option<Vec<String>>,
    issues: Vec<String>,
    retrieval_mode: String,
    embedding_model: Option<String>,
    /// The tokens a pass B page held at the end of the pass, or why it had
    /// no budget (course evidence index §5.3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    page_budget: Option<String>,
    unexecuted_languages: Vec<String>,
}

fn units(candidate: &Value) -> Result<Vec<Value>> {
    let blocks = candidate
        .get("blocks")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("Verification needs lesson blocks."))?;
    let questions = candidate
        .get("questions")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("Verification needs lesson questions."))?;
    if blocks.is_empty() || blocks.len() > 32 || questions.len() > 24 {
        return Err(invalid("Invalid verification unit count."));
    }
    Ok(blocks.iter().map(|b| json!({"kind":"teaching", "title":b["title"], "body":b["body"], "rubric":b["rubric"]}))
        .chain(questions.iter().map(|q| json!({"kind":"assessment", "prompt":q["prompt"], "options":q["options"], "correctIndex":q["correctIndex"], "explanation":q["explanation"]}))).collect())
}
fn contains_text(value: &Value, quote: &str) -> bool {
    match value {
        Value::String(text) => text.contains(quote),
        Value::Array(items) => items.iter().any(|v| contains_text(v, quote)),
        Value::Object(items) => items.values().any(|v| contains_text(v, quote)),
        _ => false,
    }
}
fn validate_inventory(inventory: &Inventory, content: &[Value]) -> Result<()> {
    let mut seen = HashSet::new();
    let count: usize = inventory.units.iter().map(|u| u.claims.len()).sum();
    if inventory.units.len() != content.len() || count == 0 {
        return Err(invalid(
            "Claim extraction returned incomplete or excessive coverage. No lesson was published.",
        ));
    }
    for unit in &inventory.units {
        let value = content
            .get(unit.index)
            .ok_or_else(|| invalid("Unknown verification section."))?;
        if !seen.insert(unit.index)
            || unit.non_factual_reason.chars().count() > 500
            || (unit.claims.is_empty() && unit.non_factual_reason.trim().is_empty())
            || unit.claims.iter().any(|c| {
                c.quote.trim().is_empty()
                    || c.quote.chars().count() > 1600
                    || !contains_text(value, &c.quote)
                    || c.statement.trim().is_empty()
                    || c.statement.chars().count() > 2000
            })
        {
            return Err(invalid(
                "Claim extraction returned invalid section evidence. No lesson was published.",
            ));
        }
    }
    Ok(())
}
/// The tokens one passage costs in a strict batch's passage bank, with its
/// id and the claim's reference to it.
pub(in crate::features::learning) fn bank_tokens(llm: &dyn LLMPort, text: &str) -> usize {
    llm.count_tokens(&json!({"id":"passage-000","text":text}).to_string()) + 5
}

/// The evidence tokens one claim may carry when `sharing` claims share a
/// strict batch call: an equal share of the room the call leaves after its
/// instructions, the claims and their answers. A claim checked alone is
/// checked by the single-claim judge, so it gets that judge's whole room.
pub(in crate::features::learning) fn claim_share(
    llm: &dyn LLMPort,
    claim: &str,
    sharing: usize,
) -> usize {
    let checker = ClaimChecker::new(
        llm,
        SamplingOverride::deterministic(),
        CHECKER_OUTPUT_TOKENS,
        CheckPolicy::Strict,
    );
    if sharing <= 1 {
        return checker.located_evidence_room(claim);
    }
    checker.batch_evidence_room(&vec![claim.to_owned(); sharing]) / sharing
}

/// A claim the checker could not judge, with why, as a judgment: the cause
/// travels in its reason, and an unverified verdict is never a receipt.
pub(in crate::features::learning) fn unchecked(cause: impl Into<String>) -> ClaimJudgment {
    ClaimJudgment::Judged(JudgeOutcome {
        verdict: ClaimVerdict::Unverified,
        reason: Some(cause.into()),
        quote: None,
        confidence: None,
    })
}

/// Whether `passages` fit within `share` tokens of a passage bank.
pub(in crate::features::learning) fn within_share<'p>(
    llm: &dyn LLMPort,
    share: usize,
    passages: impl Iterator<Item = &'p EvidencePassage>,
) -> bool {
    passages
        .map(|passage| bank_tokens(llm, &passage.text))
        .sum::<usize>()
        <= share
}

/// Whether `claim` checked alone against `passages` fits the judge's window
/// beside its answer, by the calibrated estimate. The window bounds a page;
/// it never sizes one (course evidence index §5.3).
pub(in crate::features::learning) fn fits_window<'p>(
    llm: &dyn LLMPort,
    claim: &str,
    passages: impl IntoIterator<Item = &'p EvidencePassage>,
) -> bool {
    let texts: Vec<String> = passages
        .into_iter()
        .map(|passage| passage.text.clone())
        .collect();
    ClaimChecker::new(
        llm,
        SamplingOverride::deterministic(),
        CHECKER_OUTPUT_TOKENS,
        CheckPolicy::Strict,
    )
    .located_fits(claim, &texts)
}

/// The passed example runs of section `unit`, as evidence: the section's
/// own, so pass A reads them beside its citations.
fn example_runs(observations: &[execution::Observation], unit: usize) -> Vec<EvidencePassage> {
    observations
        .iter()
        .filter(|o| o.passed() && o.unit == Some(unit))
        .map(|observation| EvidencePassage {
            source_id: observation.id.clone(),
            text: observation.evidence(),
            start_byte: 0,
            end_byte: 0,
            retrieval_kind: "execution".into(),
            score: 0.0,
        })
        .collect()
}

/// A claim's evidence when its section cites nothing: its section's passed
/// example runs (pass A) and then pass B's first page, as a fresh check
/// would select it (`evidence_checks`). For tests that bind a receipt to
/// exactly what a check compares.
#[cfg(test)]
async fn evidence_for(
    llm: &dyn LLMPort,
    claim: &str,
    unit: usize,
    references: &ReferenceCollection<'_>,
    observations: &[execution::Observation],
) -> Result<Vec<EvidencePassage>> {
    let mut evidence = example_runs(observations, unit);
    evidence.extend(
        evidence_checks::first_page(llm, references, claim, &Citations::default(), &[]).await?,
    );
    Ok(evidence)
}

/// The order the judge reads a pass B `page` in, as positions in the page:
/// by need ([`ranking`]), the most needed first, the next last and the rest
/// inward alternately, so the most relevant passages sit at the edges and
/// the least relevant at the centre, where a long prompt's middle is read
/// least closely. The page's own order stays canonical: it keys the
/// comparison and its receipt, whatever order the judge read. The most
/// needed passages of a page cut for the window keep this order among
/// themselves, so a shortened page reads the same way.
fn reading_order(page: &[EvidencePassage]) -> Vec<usize> {
    let (mut front, mut back) = (Vec::new(), Vec::new());
    for (need, position) in ranking(page).into_iter().enumerate() {
        if need % 2 == 0 {
            front.push(position);
        } else {
            back.push(position);
        }
    }
    front.extend(back.into_iter().rev());
    front
}

fn canonicalize_evidence(evidence: &mut [EvidencePassage]) {
    // Retrieval still chooses the current evidence, including conflicts. Once
    // selected, use a stable order for BOTH the actual judge input and its key.
    // Corpus-dependent ranking changes must not repeat an identical comparison.
    // What research found for the claim comes first, then the section's own
    // cited passages.
    let lead = |passage: &EvidencePassage| match passage.retrieval_kind.as_str() {
        evidence_selection::RESEARCHED => 0,
        CITED => 1,
        _ => 2,
    };
    evidence.sort_by(|a, b| {
        (lead(a), &a.source_id, a.start_byte, a.end_byte, &a.text).cmp(&(
            lead(b),
            &b.source_id,
            b.start_byte,
            b.end_byte,
            &b.text,
        ))
    });
}

#[cfg(test)]
pub(in crate::features::learning) async fn live_retrieved_passages(
    llm: &dyn LLMPort,
    claim: &str,
    references: &ReferenceCollection<'_>,
) -> Result<Vec<String>> {
    Ok(evidence_for(llm, claim, 0, references, &[])
        .await?
        .into_iter()
        .map(|passage| passage.text)
        .collect())
}

#[cfg(test)]
async fn verify(
    llm: &dyn LLMPort,
    candidate: &Value,
    sources: &[LearningSourceDto],
) -> Result<LessonVerificationReport> {
    let references = ReferenceCollection::lexical(sources)?;
    verify_with_references(
        llm,
        &BuiltinRuntimes::for_tests(),
        candidate,
        &references,
        &mut ClaimChecks::default(),
    )
    .await
}

pub(in crate::features::learning) async fn judge_batch_for_publication(
    llm: &dyn LLMPort,
    claims: &[lattice_core::application::services::claim_verification::LocatedClaim],
) -> Result<lattice_core::application::services::claim_verification::Answered<Vec<ClaimJudgment>>> {
    let checker = ClaimChecker::new(
        llm,
        SamplingOverride::deterministic(),
        CHECKER_OUTPUT_TOKENS,
        CheckPolicy::Strict,
    );
    checker
        .check_batch_without_deadline(
            claims,
            &|text| {
                crate::features::learning::lesson_progress::received(&text);
                Ok(())
            },
            &|attempt| {
                crate::features::learning::lesson_progress::model_retry(attempt);
                Ok(())
            },
        )
        .await
}

#[cfg(test)]
pub(in crate::features::learning) async fn live_coverage_fixture(
    llm: &dyn LLMPort,
    data: &Value,
    progress: &crate::features::learning::outline_progress::OutlineProgress,
) -> Result<Value> {
    let entries = data["units"]
        .as_array()
        .ok_or_else(|| invalid("Missing fixture units"))?;
    let content: Vec<_> = entries.iter().map(|unit| unit["content"].clone()).collect();
    let inventory = Inventory {
        units: entries
            .iter()
            .enumerate()
            .map(|(index, unit)| {
                let claims = unit["statements"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(|statement| Claim {
                        quote: String::new(),
                        statement: statement.into(),
                    })
                    .collect();
                UnitClaims {
                    index,
                    claims,
                    non_factual_reason: unit["nonFactualReason"]
                        .as_str()
                        .unwrap_or_default()
                        .into(),
                }
            })
            .collect(),
    };
    Ok(serde_json::to_value(
        coverage::audit(llm, &content, &inventory, Some(progress)).await?,
    )?)
}
