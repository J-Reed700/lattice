//! Fail-closed lesson publication checks. Evidence and execution reduce errors;
//! the claim inventory and its coverage audit are still fallible model judgments.
use crate::dto::LearningSourceDto;
use crate::dto::PreparedLearningLesson;
use crate::embedded_runtime::BuiltinRuntimes;
use crate::reference_collection::{PassageKind, ReferenceCollection, ReferencePassage};
use futures::{FutureExt, StreamExt};
use lattice_core::application::ports::{llm_port::SamplingOverride, LLMPort};
#[cfg(test)]
use lattice_core::application::services::claim_verification::ClaimEvidence;
use lattice_core::application::services::claim_verification::{
    CheckPolicy, ClaimChecker, ClaimJudgment, ClaimVerdict, JudgeOutcome, CONSISTENCY_POLICY,
    KNOWLEDGE_POLICY,
};
use lattice_core::domain::learning::acceptance::Reading;
use lattice_core::domain::learning::claim_review::{ClaimId, Class, ClassifiedBy};
use lattice_core::domain::provenance::Label;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::hash::sha256_hex;
use lattice_core::shared::protocol::Protocol;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

mod apparatus;
#[cfg(test)]
use apparatus::strip_citation_apparatus;
pub(crate) use apparatus::strip_draft_apparatus;
mod citations;
mod claim_ledger;
mod claim_sentences;
pub(crate) use citations::names_a_source;
use citations::{section_citations, source_mentions, take_cited, Citations};
#[cfg(test)]
pub(crate) use claim_ledger::locate;
pub(crate) use claim_ledger::{
    field_pointer, field_text, held_for_review, held_lessons, located, located_quote, narrow,
    review_outcome, stands_in_part, unit_fields, unit_texts, HeldForReview,
};
mod class_checks;
mod classify;
pub(crate) use classify::quoted_wording;
mod coverage;
#[cfg(any(test, feature = "test-support"))]
pub use coverage::live_mapping_fixture;
mod evidence_checks;
#[cfg(any(test, feature = "test-support"))]
pub use evidence_checks::page_budget;
pub(crate) use evidence_checks::supporting_passage;
use evidence_checks::{CheckedPart, Reach, SupportedElsewhere, TwoPass};
#[cfg(test)]
use evidence_checks::{ContraryPassage, Pass};
mod evidence_selection;
mod execution;
mod judge;
pub use judge::judge_for_publication;
pub(crate) use judge::{judge_in_order, ranking, shown_of};
mod inventory;
mod inventory_checkpoint;
mod learner_view;
pub(crate) use learner_view::ClaimStates;
mod portability;
mod put_text;
pub(crate) use put_text::{
    cuts_for, insert_sentence, put_at_junction, put_place, put_sentence, remove_sentence, Placing,
};
mod quote_claims;
mod readings;
mod resolution;
#[cfg(any(test, feature = "test-support"))]
pub use inventory::revisions::live_revision_fixture;
#[cfg(any(test, feature = "test-support"))]
pub async fn live_inventory_fixture(llm: &dyn LLMPort, content: &[Value]) -> Result<Value> {
    let db = lattice_core::shared::persistence::Database::new(crate::test_support::pool().await?);
    let (_, job, _) = test_support::setup(&db).await?;
    crate::lesson_progress::run(
        &lattice_core::shared::runtime::jobs::JobContext::detached(
            &lattice_core::shared::runtime::jobs::JobStore::new(db.clone()),
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
mod provenance_gate;
pub use provenance_gate::with_outline_trust;
pub(crate) use provenance_gate::{gate_reason, outline_trust, relevel, weigh_claim, with_trust};
mod repair;
pub(crate) use repair::{confine, Protected};
mod repair_loop;
mod repair_request;
mod repair_work;
mod report;
pub(crate) use report::{Rewritten, TakenOut, WrittenBack, WrittenOver};
pub mod research;
mod review_pass;
pub(crate) use review_pass::with_pending_put_backs;
mod round;
mod rounds;
pub(crate) use rounds::Rounds;
mod section_checkpoints;
mod sections;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
#[cfg(test)]
pub(crate) mod tests;
use inventory_checkpoint::{complete_inventory, extract_audited_inventory, valid_saved_coverage};
#[cfg(test)]
pub(crate) use repair_loop::settle_after_cuts;
#[cfg(test)]
pub(crate) use repair_loop::verify_and_repair;
pub(crate) use repair_loop::verify_and_repair_with_references;
#[cfg(test)]
use repair_request::repair_context;
pub(crate) use repair_request::restart;
use repair_request::{repair_fingerprint, PendingRepair};
#[cfg(test)]
use report::content_hash;
pub(crate) use report::{not_verified_line, persist_report, sources_changed};
/// v21: the evidence challenge is gone; a supported verdict is weighed by
/// the provenance gate (source provenance §11) and passages carry their
/// source's title, domain and date.
/// v22: the part-wise judge has a kind for the lesson's own figures.
const POLICY: Protocol = lattice_core::protocol!("lesson-evidence", 22);

/// What a check judged, which names the policy its fingerprint is under.
#[derive(Clone, Copy)]
pub(crate) enum Judged {
    Passages,
    Consistency,
    Knowledge,
}

impl Judged {
    pub(crate) fn of(reading: Reading) -> Self {
        match reading {
            Reading::Parts | Reading::Ground | Reading::Page => Self::Passages,
            Reading::Consistency => Self::Consistency,
            Reading::Knowledge | Reading::Veto => Self::Knowledge,
        }
    }

    pub(crate) fn policy(self) -> &'static str {
        match self {
            Self::Passages => POLICY.as_str(),
            Self::Consistency => CONSISTENCY_POLICY.as_str(),
            Self::Knowledge => KNOWLEDGE_POLICY.as_str(),
        }
    }
}
// Extraction is versioned independently of coverage and factual judgment.
// Its exact-content checkpoint contains no factual approvals to inherit.
const INVENTORY_POLICY: Protocol = lattice_core::protocol!("lesson-evidence", 9);
const CLAIM_KEYS: Protocol = lattice_core::protocol!("claim", 1);

pub(crate) async fn normalize_example_fences(llm: &dyn LLMPort, raw: String) -> Result<String> {
    execution::label_unlabeled(llm, raw).await
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
/// unrepresented, located by its app-owned passage ID and, once the judge
/// named it, the ID of the sentence that states it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MissingAssertion {
    passage_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sentence_id: Option<String>,
    statement: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CoverageUnit {
    index: usize,
    complete: bool,
    reason: String,
    /// Empty exactly when complete. What remains is what the rule layer makes
    /// Internal, or what the guard refuses: recorded in the report only.
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
    resolution: Option<resolution::Resolution>,
    /// The claim this finding reports, which the learner view follows after
    /// publication; none in a report from before findings named theirs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    claim_id: Option<String>,
    /// What the provenance gate weighed for a supported finding: the
    /// supports' levels for the claim's subject and the decision. Boxed: it
    /// is most of a finding's size and most findings have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    trust: Option<Box<provenance_gate::FindingTrust>>,
    /// How the learner is told what the claim's support is (claim classes
    /// design §3.2): the provenance gate's label for a weighed claim,
    /// `Attributed` for a quotation found in the source it names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label: Option<Label>,
    /// The class the claim was checked as, and who decided it (claim classes
    /// design §3.1): the check the learner is told of, and the ledger's
    /// record of the claim's class.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    class: Option<Classed>,
    /// How the claim was read (course evidence index §5.1): whether its own
    /// citation or the course's page decided it, which page and where it was
    /// read, the passage found when the section's citations gave no support,
    /// the parts its citation was read as, and a passage found saying
    /// otherwise about a supported claim.
    #[serde(flatten)]
    two_pass: TwoPass,
}
/// A statement's class and the authority that decided it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Classed {
    class: Class,
    by: ClassifiedBy,
}

impl Classed {
    /// Whether the statement is a claim to check. Only what the rule layer
    /// makes Internal is left out; the model's Internal meets the veto.
    fn checked(&self) -> bool {
        !(self.class == Class::Internal && self.by == ClassifiedBy::Rule)
    }
}

/// Completed comparisons and evidence selections. Durable checkpoints retain
/// both across restarts; a selection is never itself a factual approval.
#[derive(Default)]
struct ClaimChecks {
    /// Claims the ledger holds as resolved by evidence the user brought, by
    /// unit and statement: that check is their receipt (`claim_ledger`).
    settled: HashMap<(usize, String), Finding>,
    /// What the ledger keeps for the next pass: the lesson before the repairs
    /// (`claim_ledger`).
    rewrites: claim_ledger::Rewrites,
    /// The finding of each statement's latest reading, by claim: what the
    /// report carries for a statement a pass does not read. Each is also saved
    /// as a checkpoint, so a restarted job carries it still.
    latest: HashMap<ClaimId, Finding>,
    selections: std::sync::Mutex<HashMap<String, evidence_selection::Selection>>,
    research_scopes: std::sync::Mutex<HashMap<String, evidence_selection::ResearchTargets>>,
    /// What the judge's answers have cost this job, per claim, across its
    /// readings: the answer size a page of the course is budgeted by
    /// (`evidence_checks::page_budget`).
    answers: std::sync::Mutex<evidence_checks::AnswerSize>,
    /// The sections that go out of the lesson (claim classes design §3.10):
    /// their statements are not sorted or judged and their examples not
    /// reported.
    removed: sections::Going,
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
        sha256_hex(json!({"unit":unit,"statement":claim.statement,"passages":passages}).to_string())
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
    /// The positions of the page's passages the judge was shown when the
    /// whole page did not fit its window; `None` when it saw them all. The
    /// finding names only those: a passage cut for size was never compared
    /// with the claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shown: Option<Vec<usize>>,
    /// The parts the part-wise judge read the claim as.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    parts: Vec<CheckedPart>,
}

fn claim_receipt_key(llm: &dyn LLMPort, comparison: &str) -> String {
    CLAIM_KEYS.key(sha256_hex(
        json!({
            "policy": POLICY, "model": llm.model_name(),
            "context": llm.max_context_tokens(), "comparison": comparison,
            "trust": lattice_core::domain::provenance::TRUST_POLICY_VERSION,
        })
        .to_string(),
    ))
}

impl ClaimReceipt {
    /// The finding of this saved receipt against `page`, narrowed to the
    /// passages its judge was shown when it records them.
    fn restored(self, unit: usize, claim: &Claim, page: &[EvidencePassage]) -> Option<Finding> {
        let shown = match &self.shown {
            Some(shown) => shown_of(page, shown),
            None => page.to_vec(),
        };
        self.finding(unit, claim, shown)
    }

    /// The finding this receipt makes of `claim` of section `unit` against
    /// `evidence`, the passages its judge was shown; `None` when it makes none:
    /// an unverified answer, or support whose quotation no passage holds. A
    /// support keeps the passage's own words.
    fn finding(
        self,
        unit: usize,
        claim: &Claim,
        evidence: Vec<EvidencePassage>,
    ) -> Option<Finding> {
        let supporting_quote = match self.verdict {
            ClaimVerdict::Unverified => return None,
            ClaimVerdict::Supported => Some(
                supporting_passage(self.supporting_quote.as_deref()?, &evidence, None)?
                    .1
                    .to_owned(),
            ),
            ClaimVerdict::Contradicted | ClaimVerdict::Unsupported => self.supporting_quote,
        };
        Some(Finding {
            unit,
            quote: claim.quote.clone(),
            statement: claim.statement.clone(),
            verdict: self.verdict,
            reason: self.reason,
            evidence,
            supporting_quote,
            resolution: None,
            claim_id: None,
            two_pass: TwoPass {
                parts: self.parts,
                ..TwoPass::default()
            },
            label: None,
            class: None,
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
    /// The tokens a page of the course held at the end of the pass, or why it
    /// had no budget (course evidence index §5.3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    page_budget: Option<String>,
    unexecuted_languages: Vec<String>,
    /// Findings a published edit or restore replaced, by the claim that took
    /// their place, so an Undo brings them back.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    replaced_findings: Vec<report::ReplacedFindings>,
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
    if blocks.is_empty() {
        return Err(invalid("Verification needs lesson blocks."));
    }
    Ok(blocks.iter().map(|b| json!({"kind":"teaching", "role":b["kind"], "title":b["title"], "body":b["body"], "rubric":b["rubric"]}))
        .chain(questions.iter().map(|q| {
            let mut unit = json!({"kind":"assessment", "prompt":q["prompt"], "options":q["options"], "correctIndex":q["correctIndex"], "explanation":q["explanation"]});
            if let (Some(reasons), Some(fields)) = (q.get("rationales"), unit.as_object_mut()) {
                fields.insert("rationales".into(), reasons.clone());
            }
            unit
        })).collect())
}
fn validate_inventory(inventory: &Inventory, content: &[Value]) -> Result<()> {
    let mut seen = HashSet::new();
    let count: usize = inventory.units.iter().map(|u| u.claims.len()).sum();
    if inventory.units.len() != content.len() || count == 0 {
        return Err(invalid(
            "Statement extraction returned incomplete or excessive coverage. No lesson was published.",
        ));
    }
    for unit in &inventory.units {
        let value = content
            .get(unit.index)
            .ok_or_else(|| invalid("Unknown verification section."))?;
        if !seen.insert(unit.index)
            || (unit.claims.is_empty() && unit.non_factual_reason.trim().is_empty())
            || unit.claims.iter().any(|c| {
                c.quote.trim().is_empty()
                    || !stands_in_part(value, &c.quote)
                    || c.statement.trim().is_empty()
            })
        {
            return Err(invalid(
                "Statement extraction returned invalid section evidence. No lesson was published.",
            ));
        }
    }
    Ok(())
}
/// The tokens one passage costs in a strict batch's passage bank, with its
/// id and the claim's reference to it.
pub(crate) fn bank_tokens(llm: &dyn LLMPort, text: &str) -> usize {
    llm.count_tokens(&json!({"id":"passage-000","text":text}).to_string()) + 5
}

/// A claim the checker could not judge, with why, as a judgment: the cause
/// travels in its reason, and an unverified verdict is never a receipt.
pub(crate) fn unchecked(cause: impl Into<String>) -> ClaimJudgment {
    ClaimJudgment::Judged(JudgeOutcome {
        verdict: ClaimVerdict::Unverified,
        reason: Some(cause.into()),
        quote: None,
        confidence: None,
        uncertain: false,
    })
}

/// Whether `claim` checked alone against `passages` fits the judge's window
/// beside its answer, by the calibrated estimate. The window bounds a page;
/// it never sizes one (course evidence index §5.3).
pub(crate) fn fits_window<'p>(
    llm: &dyn LLMPort,
    claim: &str,
    passages: impl IntoIterator<Item = &'p EvidencePassage>,
) -> bool {
    let texts: Vec<String> = passages
        .into_iter()
        .map(|passage| passage.text.clone())
        .collect();
    ClaimChecker::for_policy(llm, SamplingOverride::deterministic(), CheckPolicy::Strict)
        .located_fits(claim, &texts)
}

/// The passed example runs of section `unit`, as evidence: the section's
/// own, which a statement's citation is read beside.
fn example_runs(observations: &[execution::Observation], unit: usize) -> Vec<EvidencePassage> {
    observations
        .iter()
        .filter(|o| o.passed() && o.unit == Some(unit))
        .map(execution::Observation::passage)
        .collect()
}

/// A claim's evidence when its section cites nothing: its section's passed
/// example runs and then the course's first page for it, as the readers
/// select it at the start of a job (`evidence_checks`). For tests that bind a
/// receipt to exactly what a reading compares.
#[cfg(any(test, feature = "test-support"))]
async fn evidence_for(
    llm: &dyn LLMPort,
    claim: &str,
    unit: usize,
    references: &ReferenceCollection<'_>,
    observations: &[execution::Observation],
) -> Result<Vec<EvidencePassage>> {
    let mut evidence = example_runs(observations, unit);
    let budget = evidence_checks::PageBudget::of(llm, evidence_checks::AnswerSize::default());
    let (mut page, _) =
        evidence_checks::page_one(llm, references, claim, claim, &[], &budget).await?;
    canonicalize_evidence(&mut page);
    evidence.extend(page);
    Ok(evidence)
}

/// The order the judge reads a page of the course in: most needed first, the
/// next last, the rest inward, so the most relevant passages sit at the edges
/// of a long prompt. The page's canonical order still keys the comparison.
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
    let lead = |passage: &EvidencePassage| match passage.kind() {
        PassageKind::Researched => 0,
        PassageKind::Cited => 1,
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

#[cfg(any(test, feature = "test-support"))]
pub async fn live_retrieved_passages(
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

pub async fn judge_batch_for_publication(
    llm: &dyn LLMPort,
    claims: &[lattice_core::application::services::claim_verification::LocatedClaim],
) -> Result<lattice_core::application::services::claim_verification::Answered<Vec<ClaimJudgment>>> {
    let checker =
        ClaimChecker::for_policy(llm, SamplingOverride::deterministic(), CheckPolicy::Strict);
    checker
        .check_batch_without_deadline(
            claims,
            &|text| {
                crate::lesson_progress::received(&text);
                Ok(())
            },
            &|attempt| {
                crate::lesson_progress::model_retry(attempt);
                Ok(())
            },
        )
        .await
}

#[cfg(any(test, feature = "test-support"))]
#[allow(clippy::indexing_slicing)]
pub async fn live_coverage_fixture(
    llm: &dyn LLMPort,
    data: &Value,
    progress: &crate::outline_progress::OutlineProgress,
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
