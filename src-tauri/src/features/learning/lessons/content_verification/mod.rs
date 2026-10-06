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
mod evidence_checks;
mod execution;
mod inventory;
mod repair;
pub(in crate::features::learning) mod research;
mod section_checkpoints;
#[cfg(test)]
pub(crate) mod tests;
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
/// Memory cache for the current pass. Durable receipts below survive restarts;
/// retrieval always runs again before either cache can reuse a comparison.
#[derive(Default)]
struct ClaimChecks(HashMap<String, Finding>);
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
        digest(&json!({"unit":unit,"claim":claim,"passages":passages}).to_string())
    }

    fn get(&self, key: &str, evidence: &[EvidencePassage]) -> Option<Finding> {
        self.0.get(key).cloned().map(|mut finding| {
            finding.evidence = evidence.to_vec();
            finding
        })
    }

    fn record(&mut self, key: String, finding: &Finding) {
        // A failed/incomplete call is never a reusable decision.
        if finding.verdict != ClaimVerdict::Unverified {
            self.0.insert(key, finding.clone());
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
            || unit.claims.len() > 24
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
    let mut used: usize = evidence.iter().map(|p| p.text.chars().count()).sum();
    if used > 24_000 {
        return Ok(Vec::new());
    }
    // Each claim searches the entire pinned collection independently of the
    // author's selected passages. Contradictory passages are not filtered out.
    for passage in references.retrieve_for_verification(claim, 8).await? {
        let length = passage.text.chars().count();
        if used + length <= 24_000 {
            used += length;
            evidence.push(passage);
        }
    }
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

async fn audit_coverage(
    llm: &dyn LLMPort,
    content: &[Value],
    inventory: &Inventory,
) -> Result<Coverage> {
    coverage::audit(llm, content, inventory, None).await
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

pub(in crate::features::learning) async fn judge_batch_for_publication(
    llm: &dyn LLMPort,
    claims: &[crate::application::services::claim_verification::LocatedClaim],
) -> Result<Vec<ClaimJudgment>> {
    let checker = ClaimChecker::new(
        llm,
        SamplingOverride::deterministic(),
        512,
        CheckPolicy::Strict,
    );
    let provisional = checker
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
        .await?;
    challenge::guard_batch(llm, claims, provisional).await
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
            if saved.assessment_policy.as_deref() != Some(coverage::ASSESSMENT_POLICY) {
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

async fn save_inventory(
    llm: &dyn LLMPort,
    candidate: &str,
    inventory: &Inventory,
    coverage: Option<&Coverage>,
) -> Result<()> {
    let checkpoint = InventoryCheckpoint {
        policy: INVENTORY_POLICY.into(),
        model: llm.model_name().into(),
        model_context: llm.max_context_tokens(),
        content_sha256: digest(candidate),
        inventory: inventory.clone(),
        coverage: coverage.cloned().unwrap_or(Coverage { units: Vec::new() }),
        coverage_policy: coverage.map(|_| coverage::POLICY.into()),
        assessment_policy: coverage.map(|_| coverage::ASSESSMENT_POLICY.into()),
        teaching_context_policy: coverage.map(|_| coverage::TEACHING_CONTEXT_POLICY.into()),
    };
    crate::features::learning::lesson_drafts::record_claim_inventory(
        candidate,
        serde_json::to_string(&checkpoint)?,
    )
    .await
}

async fn extract_audited_inventory(
    llm: &dyn LLMPort,
    content: &[Value],
    candidate: &str,
) -> Result<(Inventory, Coverage)> {
    crate::features::learning::lesson_progress::phase(
        crate::features::learning::lesson_progress::Phase::Inventory,
        "Extracting factual claims from the lesson and assessments",
    );
    let (retained, coverage) = section_checkpoints::load(llm, content).await?;
    if !retained.is_empty() {
        crate::features::learning::lesson_progress::stage(format!(
            "Reusing completed claim inventories for {} unchanged sections",
            retained.len()
        ));
    }
    let inventory = inventory::extract_remaining(llm, content, retained).await?;
    complete_inventory(llm, content, inventory, candidate, Some(coverage)).await
}

fn valid_saved_coverage(coverage: &Coverage, content: &[Value]) -> bool {
    let inputs = inventory::inputs(content);
    let mut seen = HashSet::new();
    coverage.units.iter().all(|unit| {
        let Some(input) = inputs.get(unit.index) else {
            return false;
        };
        let ids: HashSet<_> = input["passages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|passage| passage["id"].as_str())
            .collect();
        seen.insert(unit.index)
            && (!unit.complete || unit.unresolved_passages.is_empty())
            && unit
                .unresolved_passages
                .iter()
                .all(|id| ids.contains(id.as_str()))
            && unit
                .unresolved_passages
                .iter()
                .collect::<HashSet<_>>()
                .len()
                == unit.unresolved_passages.len()
    })
}

fn retain_current_assessment_checks(
    coverage: &mut Coverage,
    content: &[Value],
    policy: Option<&str>,
) {
    if policy != Some(coverage::ASSESSMENT_POLICY) {
        // Teaching inputs and their judge instructions are unchanged. Only
        // assessments receive scenario context, so only those audits expire.
        coverage.units.retain(|unit| {
            content
                .get(unit.index)
                .is_some_and(|unit| unit["kind"] != "assessment")
        });
    }
}

fn retain_current_teaching_checks(
    coverage: &mut Coverage,
    content: &[Value],
    policy: Option<&str>,
) {
    if policy != Some(coverage::TEACHING_CONTEXT_POLICY) {
        coverage.units.retain(|unit| {
            content.get(unit.index).is_some_and(|section| {
                section["kind"] != "teaching"
                    // Earlier checkpoints had no contextual fallback. Their
                    // positive base comparisons are unchanged and still valid;
                    // negative ones must be reconsidered with section context.
                    || (policy.is_none() && unit.complete)
            })
        });
    }
}

// This strictly decreasing, bounded tuple prevents correction cycles without
// imposing a time or attempt limit. More complete sections come first, then
// fewer unresolved passages, then more captured claims within those passages.
fn coverage_work(
    coverage: &Coverage,
    inventory: &Inventory,
    content: &[Value],
) -> (usize, usize, std::cmp::Reverse<usize>) {
    let inputs = inventory::inputs(content);
    let incomplete: Vec<_> = coverage
        .units
        .iter()
        .filter(|unit| !unit.complete)
        .collect();
    let passages = incomplete
        .iter()
        .map(|unit| {
            if unit.unresolved_passages.is_empty() {
                // Older checkpoints retain their findings as data. An absent
                // location list is never interpreted as zero unresolved work.
                inputs
                    .get(unit.index)
                    .and_then(|input| input["passages"].as_array())
                    .map_or(0, Vec::len)
            } else {
                unit.unresolved_passages.len()
            }
        })
        .sum();
    (
        incomplete.len(),
        passages,
        std::cmp::Reverse(inventory.units.iter().map(|unit| unit.claims.len()).sum()),
    )
}

async fn complete_inventory(
    llm: &dyn LLMPort,
    content: &[Value],
    mut inventory: Inventory,
    candidate: &str,
    saved_coverage: Option<Coverage>,
) -> Result<(Inventory, Coverage)> {
    let mut coverage = saved_coverage.unwrap_or(Coverage { units: Vec::new() });
    // A later batch can fail after individual sections were saved but before
    // the whole-candidate checkpoint was updated. Prefer those completed,
    // current-policy sections when resuming the older aggregate checkpoint.
    let (retained, audits) = section_checkpoints::load(llm, content).await?;
    for unit in retained {
        inventory
            .units
            .retain(|current| current.index != unit.index);
        inventory.units.push(unit);
    }
    for audit in audits.units {
        coverage
            .units
            .retain(|current| current.index != audit.index);
        coverage.units.push(audit);
    }
    inventory.units.sort_by_key(|unit| unit.index);
    coverage.units.sort_by_key(|unit| unit.index);
    validate_inventory(&inventory, content)?;
    section_checkpoints::save(llm, content, &inventory.units, &coverage.units).await?;
    save_inventory(llm, candidate, &inventory, Some(&coverage)).await?;
    crate::features::learning::lesson_progress::phase(
        crate::features::learning::lesson_progress::Phase::Coverage,
        "Checking that every factual claim was captured",
    );
    let pending = Inventory {
        units: inventory
            .units
            .iter()
            .filter(|unit| !coverage.units.iter().any(|check| check.index == unit.index))
            .cloned()
            .collect(),
    };
    if !pending.units.is_empty() {
        coverage
            .units
            .extend(audit_coverage(llm, content, &pending).await?.units);
        coverage.units.sort_by_key(|unit| unit.index);
    }
    save_inventory(llm, candidate, &inventory, Some(&coverage)).await?;
    while coverage.units.iter().any(|unit| !unit.complete) {
        let previous_work = coverage_work(&coverage, &inventory, content);
        crate::features::learning::lesson_progress::stage(
            "Completing missing claims identified by the coverage audit",
        );
        inventory = inventory::correct_coverage(llm, content, &inventory, &coverage).await?;
        crate::features::learning::lesson_progress::stage(
            "Rechecking the completed claim inventory",
        );
        // correct_coverage retains complete section inventories verbatim, and
        // lesson content is immutable here. Recheck only corrected sections;
        // unrelated approved inventories have not changed their audit inputs.
        let unchanged: HashSet<_> = coverage
            .units
            .iter()
            .filter(|unit| unit.complete)
            .map(|unit| unit.index)
            .collect();
        let corrected = Inventory {
            units: inventory
                .units
                .iter()
                .filter(|unit| !unchanged.contains(&unit.index))
                .cloned()
                .collect(),
        };
        // Save completed, unchanged units even if the next model call fails.
        coverage.units.retain(|unit| unit.complete);
        save_inventory(llm, candidate, &inventory, Some(&coverage)).await?;
        section_checkpoints::save(llm, content, &inventory.units, &coverage.units).await?;
        let checked = audit_coverage(llm, content, &corrected).await?;
        coverage.units.extend(checked.units);
        coverage.units.sort_by_key(|unit| unit.index);
        save_inventory(llm, candidate, &inventory, Some(&coverage)).await?;
        if coverage.units.iter().any(|unit| !unit.complete)
            && coverage_work(&coverage, &inventory, content) >= previous_work
        {
            return Err(AppError::Other(format!(
                "Claim coverage stopped making progress: {}. The draft and completed checks are saved; no lesson was published.",
                coverage
                    .units
                    .iter()
                    .filter(|unit| !unit.complete)
                    .map(|unit| format!("Section {}: {}", unit.index + 1, unit.reason))
                    .collect::<Vec<_>>()
                    .join("; ")
            )));
        }
    }
    Ok((inventory, coverage))
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

/// Evidence-driven repairs, with full re-extraction and rechecking afterward.
/// Continue while defects decrease; preserve a draft when repairs stop helping.
/// Operational extraction/audit errors propagate rather than asking the author
/// to rewrite valid content because a checker is unavailable.
#[cfg(test)]
pub(in crate::features::learning) async fn verify_and_repair(
    llm: &dyn LLMPort,
    prompt: &str,
    schema: &Value,
    raw: String,
    sources: &[LearningSourceDto],
    output_tokens: usize,
) -> Result<(String, LessonVerificationReport)> {
    let references = ReferenceCollection::lexical(sources)?;
    verify_and_repair_with_references(llm, prompt, schema, raw, &references, output_tokens).await
}

pub(in crate::features::learning) async fn verify_and_repair_with_references(
    llm: &dyn LLMPort,
    prompt: &str,
    schema: &Value,
    mut raw: String,
    references: &ReferenceCollection<'_>,
    output_tokens: usize,
) -> Result<(String, LessonVerificationReport)> {
    let mut expanded = None;
    let mut previous_defects = None;
    let mut resume_repair = true;
    let mut checks = ClaimChecks::default();
    loop {
        raw = normalize_example_fences(llm, raw).await?;
        let candidate: Value = crate::features::learning::generation::parse_json(&raw)?;
        let active = expanded.as_ref().unwrap_or(references);
        // A failed rewrite can resume from its saved defects. This only skips
        // repeating the diagnosis; it never skips checking the rewritten text.
        if std::mem::take(&mut resume_repair) {
            if let Some(saved) =
                crate::features::learning::lesson_drafts::pending_repair(&candidate.to_string())
                    .await?
                    .and_then(|raw| serde_json::from_str::<PendingRepair>(&raw).ok())
                    .filter(|saved| saved.matches(llm, prompt, schema, &candidate, active))
            {
                previous_defects = Some(saved.defects);
                crate::features::learning::lesson_progress::phase(
                    crate::features::learning::lesson_progress::Phase::Repair,
                    "Resuming the saved evidence-based repair",
                );
                raw = repair::candidate(
                    llm,
                    prompt,
                    schema,
                    saved.context,
                    output_tokens,
                    active,
                    saved.defects,
                )
                .await?;
                continue;
            }
        }
        let mut report = verify_with_references(llm, &candidate, active, &mut checks).await?;
        loop {
            let gaps = report
                .findings
                .iter()
                .filter(|f| {
                    matches!(
                        f.verdict,
                        ClaimVerdict::Unsupported | ClaimVerdict::Contradicted
                    )
                })
                .count();
            let active = expanded.as_ref().unwrap_or(references);
            let Some(additional) = research::expand(llm, active, &report.findings)
                .boxed()
                .await?
            else {
                break;
            };
            expanded = Some(additional);
            let active = expanded
                .as_ref()
                .ok_or_else(|| AppError::InternalError("Missing expanded references".into()))?;
            report = check_evidence(
                llm,
                &candidate,
                active,
                Inventory {
                    units: report.coverage,
                },
                Coverage {
                    units: report.coverage_audit,
                },
                report.executions,
                &mut checks,
            )
            .await?;
            // More research can close missing-evidence gaps. Contradictions
            // after this expansion need rewriting now, even if another claim
            // improved; otherwise known falsehoods trigger repeated full passes.
            if report
                .findings
                .iter()
                .filter(|f| {
                    matches!(
                        f.verdict,
                        ClaimVerdict::Unsupported | ClaimVerdict::Contradicted
                    )
                })
                .count()
                >= gaps
                || report
                    .findings
                    .iter()
                    .any(|finding| finding.verdict == ClaimVerdict::Contradicted)
            {
                break;
            }
        }
        if report.issues.is_empty() {
            return Ok((raw, report));
        }
        if report
            .findings
            .iter()
            .any(|f| f.verdict == ClaimVerdict::Unverified)
        {
            let unfinished = report
                .findings
                .iter()
                .filter(|f| f.verdict == ClaimVerdict::Unverified)
                .count();
            return Err(AppError::Other(format!("The evidence checker could not complete {unfinished} claim checks. No lesson was published. Retry when the checker is available; durable jobs retain the lesson draft.")));
        }
        if previous_defects.is_some_and(|previous| report.issues.len() >= previous) {
            return Err(invalid(format!("Lesson repair stopped making progress with {} unresolved checks: {} The draft is saved; no lesson was published.", report.issues.len(), report.issues.first().map(String::as_str).unwrap_or("Incomplete checks"))));
        }
        previous_defects = Some(report.issues.len());
        crate::features::learning::lesson_progress::phase(
            crate::features::learning::lesson_progress::Phase::Repair,
            "Correcting defects found by evidence checks",
        );
        let context = repair_context(prompt, &candidate, &report)?;
        let pending = PendingRepair {
            fingerprint: repair_fingerprint(
                llm,
                prompt,
                schema,
                &candidate,
                expanded.as_ref().unwrap_or(references),
            ),
            defects: report.issues.len(),
            context: context.clone(),
        };
        crate::features::learning::lesson_drafts::record_pending_repair(
            &candidate.to_string(),
            serde_json::to_string(&pending)?,
        )
        .await?;
        raw = repair::candidate(
            llm,
            prompt,
            schema,
            context,
            output_tokens,
            expanded.as_ref().unwrap_or(references),
            report.issues.len(),
        )
        .await?;
    }
}

#[derive(Serialize, Deserialize)]
struct PendingRepair {
    fingerprint: String,
    defects: usize,
    context: Value,
}

pub(in crate::features::learning) async fn has_pending_repair(
    llm: &dyn LLMPort,
    prompt: &str,
    schema: &Value,
    raw: &str,
    references: &ReferenceCollection<'_>,
) -> Result<bool> {
    let candidate: Value = crate::features::learning::generation::parse_json(raw)?;
    Ok(
        crate::features::learning::lesson_drafts::pending_repair(raw)
            .await?
            .and_then(|raw| serde_json::from_str::<PendingRepair>(&raw).ok())
            .is_some_and(|saved| saved.matches(llm, prompt, schema, &candidate, references)),
    )
}

fn repair_fingerprint(
    llm: &dyn LLMPort,
    prompt: &str,
    schema: &Value,
    candidate: &Value,
    references: &ReferenceCollection<'_>,
) -> String {
    let mut sources: Vec<_> = references
        .sources
        .iter()
        .map(|source| (&source.id, digest(&source.excerpt)))
        .collect();
    sources.sort();
    digest(&json!({"policy":POLICY,"repairPolicy":"pending-repair-v1","model":llm.model_name(),"prompt":prompt,"schema":schema,"candidate":candidate,"sources":sources}).to_string())
}

impl PendingRepair {
    fn matches(
        &self,
        llm: &dyn LLMPort,
        prompt: &str,
        schema: &Value,
        candidate: &Value,
        references: &ReferenceCollection<'_>,
    ) -> bool {
        self.defects > 0
            && self.context.get("candidate") == Some(candidate)
            && self.fingerprint == repair_fingerprint(llm, prompt, schema, candidate, references)
    }
}

/// References overlap heavily across related claims. Send each exact passage
/// once instead of multiplying an entire retrieval result by every finding.
/// This changes representation only: no conflicting or unsupported evidence is
/// discarded, and the full candidate still undergoes independent verification.
fn repair_context(
    prompt: &str,
    candidate: &Value,
    report: &LessonVerificationReport,
) -> Result<Value> {
    let mut locations = HashMap::new();
    let mut passages = Vec::new();
    let failed: Vec<_> = report.findings.iter()
        .filter(|finding| finding.verdict != ClaimVerdict::Supported)
        .map(|finding| {
            let evidence: Vec<_> = finding.evidence.iter().map(|passage| {
                let key = (&passage.source_id, passage.start_byte, passage.end_byte, &passage.text);
                let index = *locations.entry(key).or_insert_with(|| {
                    let index = passages.len();
                    passages.push(json!({"id":format!("evidence-{index}"),"sourceId":passage.source_id,"startByte":passage.start_byte,"endByte":passage.end_byte,"text":passage.text}));
                    index
                });
                format!("evidence-{index}")
            }).collect();
            json!({"unit":finding.unit,"statement":finding.statement,"verdict":finding.verdict,"reason":finding.reason,"evidence":evidence})
        }).collect();
    Ok(json!({
        "requirements":crate::features::learning::generation::parse_json::<Value>(prompt)?,
        "candidate":candidate,"failedClaims":failed,"evidence":passages,
        "executions":report.executions
    }))
}

fn content_hash(lesson: &PreparedLearningLesson) -> Result<String> {
    Ok(digest(&serde_json::to_string(&(
        &lesson.blocks,
        &lesson.questions,
        &lesson.keys,
    ))?))
}
impl LessonVerificationReport {
    pub(in crate::features::learning) fn reusable(
        &self,
        lesson_id: &str,
        lesson: &PreparedLearningLesson,
        sources: &[crate::features::learning::dto::LearningSourceDto],
        llm: &dyn LLMPort,
    ) -> bool {
        self.checker_model == llm.model_name()
            && self.validate(lesson_id, lesson).is_ok()
            && self.sources.len() == sources.len()
            && self.sources.iter().all(|binding| {
                sources.iter().any(|source| {
                    source.id == binding.id && digest(&source.excerpt) == binding.sha256
                })
            })
    }

    pub(in crate::features::learning) fn bind(
        mut self,
        lesson_id: &str,
        lesson: &PreparedLearningLesson,
    ) -> Result<Self> {
        self.lesson_id = lesson_id.into();
        self.content_sha256 = content_hash(lesson)?;
        Ok(self)
    }
    fn validate(&self, lesson_id: &str, lesson: &PreparedLearningLesson) -> Result<()> {
        let unit_count = lesson.blocks.len() + lesson.questions.len();
        let coverage_ids = self
            .coverage
            .iter()
            .map(|unit| unit.index)
            .collect::<HashSet<_>>();
        let audit_ids = self
            .coverage_audit
            .iter()
            .map(|unit| unit.index)
            .collect::<HashSet<_>>();
        // A deserialized final checkpoint must account for every extracted
        // assertion exactly once. A nonempty subset of supported findings is
        // not a complete report, even when the content hash still matches.
        let mut outstanding = HashMap::new();
        for unit in &self.coverage {
            for claim in &unit.claims {
                *outstanding
                    .entry((unit.index, claim.quote.as_str(), claim.statement.as_str()))
                    .or_insert(0usize) += 1;
            }
        }
        let findings_complete = self.findings.iter().all(|finding| {
            let Some(count) = outstanding.get_mut(&(
                finding.unit,
                finding.quote.as_str(),
                finding.statement.as_str(),
            )) else {
                return false;
            };
            if *count == 0 {
                return false;
            }
            *count -= 1;
            true
        }) && outstanding.values().all(|count| *count == 0);
        if unit_count == 0
            || self.coverage.len() != unit_count
            || coverage_ids.len() != unit_count
            || self.coverage_audit.len() != unit_count
            || audit_ids != coverage_ids
            || coverage_ids.iter().any(|index| *index >= unit_count)
            || self.coverage_audit.iter().any(|unit| !unit.complete)
            || self.policy != POLICY
            || self.lesson_id != lesson_id
            || self.content_sha256 != content_hash(lesson)?
            || !self.issues.is_empty()
            || self.findings.is_empty()
            || !findings_complete
            || self
                .findings
                .iter()
                .any(|f| f.verdict != ClaimVerdict::Supported)
            || self.executions.iter().any(|e| !e.passed())
        {
            return Err(invalid("A complete verification report for this exact lesson revision is required before publication."));
        }
        Ok(())
    }
}

/// Called inside both publication transactions, before marking anything ready.
pub(in crate::features::learning) async fn persist_report(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    program_id: &str,
    lesson_id: &str,
    lesson: &PreparedLearningLesson,
) -> Result<()> {
    use sqlx::Row;
    let report = lesson.verification.as_ref().ok_or_else(|| {
        invalid("This lesson has no verification report. Prepare it again before publication.")
    })?;
    report.validate(lesson_id, lesson)?;
    if !report.sources.is_empty() {
        let active: Vec<String> = sqlx::query_scalar("SELECT active_version_id FROM learning_source_library WHERE program_id=? AND deleted_at IS NULL AND active_version_id IS NOT NULL").bind(program_id).fetch_all(&mut **tx).await.map_err(|e| AppError::Database(e.to_string()))?;
        let expected: HashSet<_> = report.sources.iter().map(|s| s.id.as_str()).collect();
        if active.iter().map(String::as_str).collect::<HashSet<_>>() != expected {
            return Err(invalid(
                "The reference collection changed during verification. Prepare the lesson again.",
            ));
        }
    }
    for source in &report.sources {
        let row = sqlx::query("SELECT v.full_text FROM learning_source_versions v JOIN learning_source_library s ON s.active_version_id=v.id AND s.program_id=v.program_id WHERE v.id=? AND v.program_id=? AND s.deleted_at IS NULL")
            .bind(&source.id).bind(program_id).fetch_optional(&mut **tx).await.map_err(|e| AppError::Database(e.to_string()))?.ok_or_else(|| invalid("A verified source is no longer active; verify this lesson again."))?;
        if digest(&row.get::<String, _>("full_text")) != source.sha256 {
            return Err(invalid(
                "Verification source content changed; verify this lesson again.",
            ));
        }
    }
    sqlx::query("INSERT INTO learning_lesson_verifications(lesson_id,program_id,policy,content_sha256,report_json,checked_at) VALUES(?,?,?,?,?,?)")
        .bind(lesson_id).bind(program_id).bind(&report.policy).bind(&report.content_sha256).bind(serde_json::to_string(report)?).bind(report.checked_at).execute(&mut **tx).await.map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}
