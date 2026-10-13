//! Fail-closed lesson publication checks. Evidence and execution reduce errors;
//! the claim inventory and its coverage audit are still fallible model judgments.
use crate::application::ports::{llm_port::SamplingOverride, LLMPort};
#[cfg(test)]
use crate::application::services::claim_verification::ClaimEvidence;
use crate::application::services::claim_verification::{
    CheckPolicy, ClaimChecker, ClaimJudgment, ClaimVerdict, JudgeOutcome,
};
#[cfg(test)]
use crate::features::learning::dto::LearningSourceDto;
use crate::features::learning::dto::PreparedLearningLesson;
use crate::features::learning::reference_collection::{ReferenceCollection, ReferencePassage};
use crate::shared::error::{AppError, Result};
use futures::{FutureExt, StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

mod challenge;
mod coverage;
#[cfg(test)]
pub(in crate::features::learning) use coverage::live_mapping_fixture;
mod claim_reuse;
mod evidence_checks;
mod evidence_selection;
mod execution;
mod inventory;
mod inventory_checkpoint;
mod learner_view;
mod portability;
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
        &crate::shared::runtime::jobs::JobContext::detached(
            &crate::shared::runtime::jobs::JobStore::new(pool.clone()),
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
mod repair;
mod repair_loop;
mod report;
pub(in crate::features::learning) mod research;
mod section_checkpoints;
#[cfg(test)]
pub(crate) mod tests;
use inventory_checkpoint::{
    complete_inventory, extract_audited_inventory, retain_current_assessment_checks,
    retain_current_teaching_checks, valid_saved_coverage,
};
#[cfg(test)]
pub(in crate::features::learning) use repair_loop::verify_and_repair;
pub(in crate::features::learning) use repair_loop::{
    has_pending_repair, verify_and_repair_with_references,
};
#[cfg(test)]
use repair_loop::{repair_context, repair_issue_fingerprint};
use repair_loop::{repair_fingerprint, PendingRepair};
#[cfg(test)]
use report::content_hash;
pub(in crate::features::learning) use report::persist_report;
const POLICY: &str = "lesson-evidence-v17";
// Extraction is versioned independently of coverage and factual judgment.
// Its exact-content checkpoint contains no factual approvals to inherit.
const INVENTORY_POLICY: &str = "lesson-evidence-v5";

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
    #[serde(default)]
    coverage_policy: Option<String>,
    #[serde(default)]
    assessment_policy: Option<String>,
    #[serde(default)]
    teaching_context_policy: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CoverageUnit {
    index: usize,
    complete: bool,
    reason: String,
    /// App-owned locations permit progress checks without interpreting prose.
    #[serde(default)]
    unresolved_passages: Vec<String>,
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
}
/// Completed comparisons and evidence selections. Durable checkpoints retain
/// both across restarts; a selection is never itself a factual approval.
#[derive(Default)]
struct ClaimChecks {
    findings: HashMap<String, Finding>,
    selections: std::sync::Mutex<HashMap<String, evidence_selection::Selection>>,
    research_scopes: std::sync::Mutex<HashMap<String, HashSet<(usize, String)>>>,
    prior_claims: std::sync::Mutex<HashMap<String, Vec<(usize, Claim)>>>,
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
            finding.evidence = evidence.to_vec();
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

/// A complete strict judgment plus its independent challenge, saved only after
/// parsing and checking source quotes. Evidence is retrieved anew on resume,
/// rather than duplicating full reference bodies in every checkpoint.
#[derive(Serialize, Deserialize)]
struct ClaimReceipt {
    verdict: ClaimVerdict,
    reason: String,
    supporting_quote: Option<String>,
    #[serde(default)]
    interpretation_policy: Option<String>,
}

fn claim_receipt_key(llm: &dyn LLMPort, comparison: &str) -> String {
    format!(
        "claim-v1:{}",
        digest(
            &json!({
                "policy": POLICY, "model": llm.model_name(),
                "context": llm.max_context_tokens(), "comparison": comparison,
            })
            .to_string()
        )
    )
}

impl ClaimReceipt {
    fn finding(self, unit: usize, claim: &Claim, evidence: &[EvidencePassage]) -> Option<Finding> {
        if self.verdict == ClaimVerdict::Unverified
            || (self.verdict != ClaimVerdict::Supported
                && self.interpretation_policy.as_deref() != Some(
                    crate::application::services::claim_verification::STRICT_INTERPRETATION_POLICY,
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
    issues: Vec<String>,
    retrieval_mode: String,
    embedding_model: Option<String>,
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
async fn evidence_for(
    claim: &str,
    unit: usize,
    references: &ReferenceCollection<'_>,
    observations: &[execution::Observation],
) -> Result<Vec<EvidencePassage>> {
    let mut evidence = Vec::new();
    for observation in observations
        .iter()
        .filter(|o| o.passed() && o.unit == Some(unit))
    {
        evidence.push(EvidencePassage {
            source_id: observation.id.clone(),
            text: observation.evidence(),
            start_byte: 0,
            end_byte: 0,
            retrieval_kind: "execution".into(),
            score: 0.0,
        });
    }
    // Each claim searches the entire pinned collection independently of the
    // author's selected passages. Contradictory passages are not filtered out.
    // Execution output must not erase retrieved references when it is lengthy.
    // The checker batches by its actual context budget, not a character count.
    evidence.extend(references.retrieve_for_verification(claim, 8).await?);
    canonicalize_evidence(&mut evidence);
    Ok(evidence)
}

fn canonicalize_evidence(evidence: &mut [EvidencePassage]) {
    // Retrieval still chooses the current evidence, including conflicts. Once
    // selected, use a stable order for BOTH the actual judge input and its key.
    // Corpus-dependent ranking changes must not repeat an identical comparison.
    evidence.sort_by(|a, b| {
        (&a.source_id, a.start_byte, a.end_byte, &a.text).cmp(&(
            &b.source_id,
            b.start_byte,
            b.end_byte,
            &b.text,
        ))
    });
}

#[cfg(test)]
pub(in crate::features::learning) async fn live_retrieved_passages(
    claim: &str,
    references: &ReferenceCollection<'_>,
) -> Result<Vec<String>> {
    Ok(evidence_for(claim, 0, references, &[])
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
    verify_with_references(llm, candidate, &references, &mut ClaimChecks::default()).await
}

/// The publication path requires both source entailment and an independent
/// search for unresolved evidence concerns. Neither can bypass the other.
pub(in crate::features::learning) async fn judge_for_publication(
    llm: &dyn LLMPort,
    claim: &str,
    passages: &[String],
) -> ClaimJudgment {
    let checker = ClaimChecker::new(
        llm,
        SamplingOverride::deterministic(),
        512,
        CheckPolicy::Strict,
    );
    let _model_call = crate::features::learning::lesson_progress::model_call();
    let provisional = checker
        .check_passages_without_deadline(claim, passages)
        .await;
    challenge::guard(llm, claim, passages, provisional).await
}

pub(in crate::features::learning) async fn judge_batch_provisionally(
    llm: &dyn LLMPort,
    claims: &[crate::application::services::claim_verification::LocatedClaim],
) -> Result<Vec<ClaimJudgment>> {
    let checker = ClaimChecker::new(
        llm,
        SamplingOverride::deterministic(),
        512,
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

pub(in crate::features::learning) async fn challenge_batch_for_publication(
    llm: &dyn LLMPort,
    claims: &[crate::application::services::claim_verification::LocatedClaim],
    provisional: Vec<ClaimJudgment>,
) -> Result<Vec<ClaimJudgment>> {
    challenge::guard_batch(llm, claims, provisional).await
}

#[cfg(test)]
pub(in crate::features::learning) async fn judge_batch_for_publication(
    llm: &dyn LLMPort,
    claims: &[crate::application::services::claim_verification::LocatedClaim],
) -> Result<Vec<ClaimJudgment>> {
    let provisional = judge_batch_provisionally(llm, claims).await?;
    challenge_batch_for_publication(llm, claims, provisional).await
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

async fn verify_with_references(
    llm: &dyn LLMPort,
    candidate: &Value,
    references: &ReferenceCollection<'_>,
    checks: &mut ClaimChecks,
) -> Result<LessonVerificationReport> {
    let candidate_text = candidate.to_string();
    let content = units(candidate)?;
    let checkpoint = crate::features::learning::lesson_drafts::claim_inventory(&candidate_text)
        .await?
        .and_then(|raw| serde_json::from_str::<InventoryCheckpoint>(&raw).ok())
        .filter(|saved| {
            saved.policy == INVENTORY_POLICY
                && saved.model == llm.model_name()
                && saved.model_context == llm.max_context_tokens()
                && saved.content_sha256 == digest(&candidate_text)
                && validate_inventory(&saved.inventory, &content).is_ok()
        });
    let (inventory, coverage) = match checkpoint {
        Some(mut saved) => {
            retain_current_teaching_checks(
                &mut saved.coverage,
                &content,
                saved.teaching_context_policy.as_deref(),
            );
            retain_current_assessment_checks(
                &mut saved.coverage,
                &content,
                saved.assessment_policy.as_deref(),
            );
            if saved.coverage_policy.as_deref() != Some(coverage::POLICY)
                || !valid_saved_coverage(&saved.coverage, &content)
            {
                saved.coverage.units.clear();
            }
            // v3 adds original choice labels as interpretation context, without
            // changing which assertions belong in the saved inventory.
            if !matches!(
                saved.assessment_policy.as_deref(),
                Some(coverage::ASSESSMENT_POLICY) | Some("assessment-fidelity-v2")
            ) {
                saved.inventory =
                    inventory::refresh_assessments(llm, &content, saved.inventory).await?;
            }
            crate::features::learning::lesson_progress::phase(
                crate::features::learning::lesson_progress::Phase::Coverage,
                "Resuming saved claim coverage with the current policy",
            );
            complete_inventory(
                llm,
                &content,
                saved.inventory,
                &candidate_text,
                Some(saved.coverage),
            )
            .await?
        }
        None => extract_audited_inventory(llm, &content, &candidate_text).await?,
    };
    let checkpoint = InventoryCheckpoint {
        policy: INVENTORY_POLICY.into(),
        model: llm.model_name().into(),
        model_context: llm.max_context_tokens(),
        content_sha256: digest(&candidate_text),
        inventory,
        coverage,
        coverage_policy: Some(coverage::POLICY.into()),
        assessment_policy: Some(coverage::ASSESSMENT_POLICY.into()),
        teaching_context_policy: Some(coverage::TEACHING_CONTEXT_POLICY.into()),
    };
    crate::features::learning::lesson_drafts::record_claim_inventory(
        &candidate_text,
        serde_json::to_string(&checkpoint)?,
    )
    .await?;
    let (inventory, coverage) = (checkpoint.inventory, checkpoint.coverage);
    crate::features::learning::lesson_progress::phase(
        crate::features::learning::lesson_progress::Phase::Examples,
        "Checking executable examples",
    );
    let executions = execution::observe(candidate).await?;
    check_evidence(
        llm, candidate, references, inventory, coverage, executions, checks,
    )
    .await
}

async fn check_evidence(
    llm: &dyn LLMPort,
    candidate: &Value,
    references: &ReferenceCollection<'_>,
    inventory: Inventory,
    coverage: Coverage,
    executions: Vec<execution::Observation>,
    completed: &mut ClaimChecks,
) -> Result<LessonVerificationReport> {
    let mut issues = Vec::new();
    if executions
        .iter()
        .any(|observation| observation.unavailable())
    {
        return Err(AppError::ServiceNotAvailable("An example runtime is unavailable or cancelled. No lesson was published; retry when the runtime is available.".into()));
    }
    for observation in &executions {
        if !observation.passed() {
            issues.push(format!(
                "Example {} did not complete: {}",
                observation.id,
                observation.evidence()
            ));
        }
    }
    let results =
        evidence_checks::check(llm, references, &inventory, &executions, completed).await?;
    let findings: Vec<_> = results
        .into_iter()
        .map(|(_, key, finding, _)| {
            completed.record(key, &finding);
            finding
        })
        .collect();
    for finding in &findings {
        if finding.verdict != ClaimVerdict::Supported {
            issues.push(format!(
                "Section {}: {} — {}",
                finding.unit + 1,
                finding.statement,
                finding.reason
            ));
        }
    }
    Ok(LessonVerificationReport {
        policy: POLICY.into(),
        lesson_id: String::new(),
        content_sha256: String::new(),
        checker_model: llm.model_name().into(),
        checked_at: chrono::Utc::now().timestamp_millis(),
        sources: references
            .sources
            .iter()
            .map(|s| SourceBinding {
                id: s.id.clone(),
                sha256: digest(&s.excerpt),
            })
            .collect(),
        coverage: inventory.units,
        coverage_audit: coverage.units,
        findings,
        executions,
        issues,
        retrieval_mode: references.mode().into(),
        embedding_model: references.embedding_model(),
        unexecuted_languages: execution::unexecuted_languages(candidate),
    })
}
