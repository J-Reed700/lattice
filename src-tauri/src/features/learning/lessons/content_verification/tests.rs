#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
use super::*;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::shared::runtime::jobs::JobStore;
use std::sync::Mutex;
use std::time::Duration;
use tokio::time::Instant;

#[path = "batch_scheduler_tests.rs"]
pub(super) mod batch_scheduler_tests;

#[path = "incremental_tests.rs"]
mod incremental_tests;

const BAD: &str = "DictReader strips field-name whitespace by default.";
const GOOD: &str = "DictReader preserves field-name whitespace by default.";
const SOURCE: &str = "DictReader preserves field-name whitespace by default. Missing values in nonblank rows default to None. The optional skipinitialspace setting ignores spaces immediately after delimiters; it defaults to false.";

/// A deterministic protocol fixture, not evidence of a live model's accuracy.
/// Execution behavior is covered separately with real bundled guests.
struct Model {
    name: &'static str,
    repaired: Mutex<bool>,
    judge_calls: Mutex<usize>,
    failing_judge: bool,
    incomplete_coverage: bool,
    hold_bad_claim: bool,
    /// Fail every batched check as a server that cannot finish a group does,
    /// so the claims are checked one at a time.
    batch_unavailable: bool,
    extracted_sections: Mutex<Vec<Vec<usize>>>,
    audited_sections: Mutex<Vec<Vec<usize>>>,
    fail_coverage_at: Option<usize>,
    coverage_gap_in: Option<usize>,
}
impl Model {
    fn new() -> Self {
        Self {
            name: "scripted-evidence-checker",
            repaired: Mutex::new(false),
            judge_calls: Mutex::new(0),
            failing_judge: false,
            incomplete_coverage: false,
            hold_bad_claim: false,
            batch_unavailable: false,
            extracted_sections: Mutex::new(Vec::new()),
            audited_sections: Mutex::new(Vec::new()),
            fail_coverage_at: None,
            coverage_gap_in: None,
        }
    }
}
fn context(prompt: &str) -> Value {
    serde_json::from_str(prompt.rsplit("\n\n").next().unwrap()).unwrap()
}
/// A typed request as the prompts the scripted fixtures answer: one per claim
/// of a batched check, otherwise one. A generation request reads as its system
/// instructions followed by the user's request; a single claim check reads as
/// its user prompt, which is where the check states the claim.
pub(crate) fn fixture_prompts(request: &CompletionRequest) -> Vec<String> {
    // A two-step call's prose step is answered by `fixture_completion` alone,
    // so scripted replies and call counts belong to the structured answer.
    if crate::features::learning::lessons::structured_call::is_draft_request(request) {
        return Vec::new();
    }
    if let Some(schema) = &request.json_schema {
        assert_schema_request_holds_the_shape(request, schema);
    }
    let (system, user) = fixture_messages(request);
    if let Some(batch) = fixture_batch(user) {
        let challenge = system.starts_with("Plan independent evidence searches");
        return batch
            .into_iter()
            .map(|(claim, passages, _)| {
                let evidence = passages
                    .iter()
                    .enumerate()
                    .map(|(index, text)| format!("[passage-{index}]\n{text}"))
                    .collect::<Vec<_>>()
                    .join("\n\n");
                if challenge {
                    format!("{system}\n\nSource passages:\n{evidence}\n\nClaim: {claim}")
                } else {
                    format!("Source passages:\n{evidence}\n\nClaim: {claim}\n\nCompare every factual part with the passages before deciding.")
                }
            })
            .collect();
    }
    let challenge = system.starts_with("Plan independent evidence searches");
    if (is_claim_check(user) && !challenge) || system.is_empty() {
        return vec![user.to_owned()];
    }
    vec![format!("{system}\n\n{user}")]
}

/// The fixture's replies to [`fixture_prompts`], answered in the typed
/// protocol: one batched reply, a check's verdict as its last line, or the
/// reply text unchanged.
pub(crate) fn fixture_completion(
    request: &CompletionRequest,
    replies: Vec<String>,
) -> CompletionResponse {
    if crate::features::learning::lessons::structured_call::is_draft_request(request) {
        return CompletionResponse::from_text(
            crate::features::learning::lessons::structured_call::FIXTURE_DRAFT,
        );
    }
    let (system, user) = fixture_messages(request);
    let text = if let Some(batch) = fixture_batch(user) {
        let challenge = system.starts_with("Plan independent evidence searches");
        let checks: Vec<Value> = batch
            .iter()
            .enumerate()
            .zip(replies)
            .map(|((index, (_, _, evidence_ids)), reply)| {
                let id = json!(format!("claim-{index}"));
                if challenge {
                    let queries = serde_json::from_str::<Value>(&reply)
                        .map(|value| value["queries"].clone())
                        .unwrap_or_else(|_| json!([]));
                    json!({"id": id, "queries": queries})
                } else {
                    json!({"id": id, "response": verdict_last(&reply, evidence_ids)})
                }
            })
            .collect();
        json!({ "checks": checks }).to_string()
    } else if is_claim_check(user) && system.contains("End with a third line: Verdict") {
        let local: Vec<String> = (0..64).map(|index| format!("passage-{index}")).collect();
        replies
            .into_iter()
            .next()
            .map(|reply| verdict_last(&reply, &local))
            .unwrap_or_default()
    } else {
        replies.concat()
    };
    CompletionResponse::from_text(text)
}

/// Every schema-constrained learning request a fixture answers: its grammar
/// survives llama.cpp's repetition limit, so the server holds the shape, and
/// none of its messages also asks for the format in words.
fn assert_schema_request_holds_the_shape(request: &CompletionRequest, schema: &Value) {
    use crate::features::conversation::memory::prompts::{
        largest_repetition_bound, GRAMMAR_REPETITION_LIMIT,
    };
    let (bound, path) = largest_repetition_bound(schema);
    assert!(
        bound <= GRAMMAR_REPETITION_LIMIT as u64,
        "schema bound {bound} at {path} would make llama.cpp drop the grammar"
    );
    // The batched claim check is the shared claim-verification service's
    // prompt (application/services/claim_verification/batch.rs), not
    // learning's; it still says "Return JSON with checks" and is left to its
    // owner.
    let shared_claim_check = request.input.iter().any(|item| {
        matches!(
            item,
            lattice_core::application::ports::llm_port::CompletionInput::Message { role, content }
                if role == "system" && content.contains("Check the supplied claims independently.")
        )
    });
    if shared_claim_check {
        return;
    }
    for item in &request.input {
        if let lattice_core::application::ports::llm_port::CompletionInput::Message {
            content,
            ..
        } = item
        {
            for demand in [
                "Return only JSON",
                "Return JSON",
                "strict JSON",
                "JSON only",
                "matching the schema",
                "supplied schema",
                "original schema",
                "requested JSON",
            ] {
                assert!(
                    !content.contains(demand),
                    "a schema request still asks for the format in words: {demand}"
                );
            }
        }
    }
}

/// The task's own system and user text, so the answer step of a two-step call
/// reads as the one-step request it replaces.
fn fixture_messages(request: &CompletionRequest) -> (&str, &str) {
    crate::features::learning::lessons::structured_call::task_messages(request)
}

fn is_claim_check(user: &str) -> bool {
    user.starts_with("Source passages:") || user.starts_with("Audit claim fidelity.")
}

/// One claim of a batched check: its text, its permitted passages and their
/// bank identifiers.
type FixtureClaim = (String, Vec<String>, Vec<String>);

fn fixture_batch(user: &str) -> Option<Vec<FixtureClaim>> {
    let batch: Value = serde_json::from_str(user).ok()?;
    let bank = batch["sourcePassages"].as_array()?;
    let text_of = |id: &str| {
        bank.iter()
            .find(|passage| passage["id"] == id)
            .and_then(|passage| passage["text"].as_str())
            .unwrap_or_default()
            .to_owned()
    };
    batch["claims"].as_array().map(|claims| {
        claims
            .iter()
            .map(|claim| {
                let ids: Vec<String> = claim["evidenceIds"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|id| id.as_str().map(str::to_owned))
                    .collect();
                (
                    claim["claim"].as_str().unwrap_or_default().to_owned(),
                    ids.iter().map(|id| text_of(id)).collect(),
                    ids,
                )
            })
            .collect()
    })
}

/// A label-first fixture verdict ("supported\nReason: …\nSource passage:
/// passage-0") rewritten to state the comparison first and the verdict last,
/// with the passage named by `ids` (the claim's own numbering mapped to the
/// identifiers the request used). Anything else is returned as written.
fn verdict_last(reply: &str, ids: &[String]) -> String {
    let mut lines = reply.lines().map(str::trim).filter(|line| !line.is_empty());
    let Some(label) = lines.next().filter(|line| {
        ["supported", "contradicted", "unsupported"].contains(&line.to_ascii_lowercase().as_str())
    }) else {
        return reply.to_owned();
    };
    let mut reason = None;
    let mut passage = "none".to_owned();
    for line in lines {
        if let Some(value) = line.strip_prefix("Reason:") {
            reason = Some(value.trim().to_owned());
        } else if let Some(value) = line.strip_prefix("Source passage:") {
            let value = value.trim();
            passage = value
                .strip_prefix("passage-")
                .and_then(|index| index.parse::<usize>().ok())
                .and_then(|index| ids.get(index).cloned())
                .unwrap_or_else(|| value.to_owned());
        }
    }
    match reason {
        Some(reason) => format!("Reason: {reason}\nSource passage: {passage}\nVerdict: {label}"),
        None => reply.to_owned(),
    }
}

/// Shared with course fixtures so they exercise the additional protocol calls.
pub(crate) fn fixture_response(prompt: &str) -> Option<String> {
    if prompt.starts_with("Update claims after a lesson edit.") {
        let data = context(prompt);
        let passages = data["revisedSection"]["passages"].as_array().unwrap();
        let changes: Vec<_> = data["existingClaims"].as_array().unwrap().iter().map(|claim| {
            let old = claim["statement"].as_str().unwrap();
            let statement = if passages.iter().any(|p| p["text"].as_str().unwrap().contains(old)) { old }
                else if passages.iter().any(|p| p["text"].as_str().unwrap().contains(BAD)) { BAD } else { GOOD };
            let passage = passages.iter().find(|p| p["text"].as_str().unwrap().contains(statement)).unwrap();
            json!({"claimId":claim["claimId"],"passageId":passage["id"],"statement":if old == statement {Value::Null} else {json!(statement)}})
        }).collect();
        return Some(json!({"changes":changes,"nonFactualReason":""}).to_string());
    }
    if prompt.starts_with("Plan independent evidence searches") {
        return Some(json!({"queries":[]}).to_string());
    }
    if prompt.starts_with("Audit inventory fidelity.") {
        return Some(json!({"missingAssertions":[],"reason":"The fixture inventory faithfully represents the original assertions."}).to_string());
    }
    if prompt.starts_with("Extract lesson claims.") {
        let data = context(prompt);
        let units = data["units"].as_array().unwrap();
        return Some(inventory::fixture_response(units.iter().map(|v| {
            let passages = v["passages"].as_array().unwrap();
            let passage = passages.iter().find(|p|p["field"]=="/body" || p["field"]=="/explanation").unwrap();
            let quote = passage["text"].as_str().unwrap();
            let index = v["index"].as_u64().unwrap() as usize;
            let statement = if quote.contains(BAD) { BAD } else if quote.contains(GOOD) { GOOD } else { quote };
            json!({"index":index,"claims":[{"passageId":passage["id"],"statement":statement}],"nonFactualReason":""})
        }).collect::<Vec<_>>()).to_string());
    }
    if prompt.starts_with("Audit claim coverage independently.") {
        let data = context(prompt);
        let mut checks = serde_json::Map::new();
        for unit in data["units"].as_array().unwrap() {
            let ids: Vec<_> = unit["claims"]
                .as_array()
                .unwrap()
                .iter()
                .map(|claim| claim["id"].clone())
                .collect();
            for passage in unit["passages"].as_array().unwrap() {
                checks.insert(passage["id"].as_str().unwrap().into(), json!({"claimIds":ids,"nonFactualReason":if ids.is_empty() {"The fixture contains only a stipulated instruction."} else {""},"missingClaims":[]}));
            }
        }
        return Some(Value::Object(checks).to_string());
    }
    None
}
#[async_trait::async_trait]
impl LLMPort for Model {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let prompts = fixture_prompts(request);
        if self.batch_unavailable && prompts.len() > 1 {
            return Err(AppError::ServiceNotAvailable(
                "fixture server cannot finish a group".into(),
            ));
        }
        let mut replies = Vec::new();
        for prompt in prompts {
            replies.push(self.respond(&prompt).await?);
        }
        Ok(fixture_completion(request, replies))
    }
    fn model_name(&self) -> &str {
        self.name
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
impl Model {
    async fn respond(&self, prompt: &str) -> Result<String> {
        if prompt.starts_with("Extract lesson claims.") {
            self.extracted_sections
                .lock()
                .unwrap()
                .push(serde_json::from_value(
                    context(prompt)["requestedIndices"].clone(),
                )?);
        }
        if prompt.starts_with("Audit claim coverage independently.") {
            let mut audits = self.audited_sections.lock().unwrap();
            if self.fail_coverage_at == Some(audits.len()) {
                return Err(invalid("fixture interrupted during coverage"));
            }
            audits.push(
                context(prompt)["units"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|unit| unit["index"].as_u64().unwrap() as usize)
                    .collect(),
            );
        }
        if prompt.starts_with("Audit claim coverage independently.") && self.incomplete_coverage {
            return Ok(json!({"units":[]}).to_string());
        }
        if let Some(index) = self.coverage_gap_in {
            if prompt.starts_with("Audit inventory fidelity.") {
                let data = context(prompt);
                if data["section"]
                    .to_string()
                    .contains(&format!("Section {index}"))
                    && data["targetPassage"]["text"] == GOOD
                {
                    return Ok(json!({"missingAssertions":["An additional original assertion is missing from this fixture inventory."],"reason":"The fixture names one omission."}).to_string());
                }
            }
            if prompt.starts_with("Audit claim coverage independently.") {
                let mut response: Value = serde_json::from_str(&fixture_response(prompt).unwrap())?;
                for unit in context(prompt)["units"].as_array().unwrap() {
                    if unit["index"].as_u64() == Some(index as u64) {
                        let id = unit["passages"][0]["id"].as_str().unwrap();
                        response[id]["missingClaims"] = json!(["An assertion is missing."]);
                    }
                }
                return Ok(response.to_string());
            }
        }
        if let Some(response) = fixture_response(prompt) {
            return Ok(response);
        }
        if prompt.starts_with("Repair verified lesson defects.") {
            assert!(prompt.contains(GOOD));
            *self.repaired.lock().unwrap() = true;
            return Ok(repair_body(
                prompt,
                candidate(GOOD)["blocks"][0]["body"].clone(),
            ));
        }
        if prompt.starts_with("Review instructional quality.") {
            let data = context(prompt);
            return Ok(json!({"issues":[],"blockChecks":crate::features::learning::teaching_review::fixture_checks(data["candidate"]["blocks"].as_array().unwrap().iter().enumerate().map(|(index,block)|json!({"index":index,"passageId":block["bodyPassages"][0]["id"],"finding":"The corrected field-name statement agrees with the concrete example.","hasDefect":false})).collect::<Vec<_>>())} ).to_string());
        }
        if prompt.starts_with("Source passages:") {
            *self.judge_calls.lock().unwrap() += 1;
            if self.failing_judge {
                return Err(invalid("fixture offline"));
            }
            let bad = prompt.split("\n\nClaim: ").nth(1).unwrap().starts_with(BAD);
            if bad && self.hold_bad_claim {
                std::future::pending::<()>().await;
            }
            return Ok(format!("{}\nReason: The captured reference and runtime preserve the header spaces.\nSource quote: {GOOD}\nSource passage: passage-0", if bad { "contradicted" } else { "supported" }));
        }
        Err(invalid(format!(
            "Unexpected fixture prompt: {}",
            prompt.chars().take(100).collect::<String>()
        )))
    }
}

fn candidate(statement: &str) -> Value {
    json!({"blocks":[{"kind":"explanation","title":"CSV header names","body":format!("{} ", statement).repeat(24),"rubric":[]}],"questions":[]})
}
fn repair_body(prompt: &str, after: Value) -> String {
    json!({"edits":[{"path":"/blocks/0/body","before":context(prompt)["candidate"]["blocks"][0]["body"],"after":after}]}).to_string()
}
fn source() -> LearningSourceDto {
    LearningSourceDto {
        id: uuid::Uuid::new_v4().to_string(),
        title: "CSV reference fixture".into(),
        url: Some("https://docs.python.org/3.14/library/csv.html".into()),
        excerpt: SOURCE.into(),
        acquired_at: 0,
    }
}

#[path = "checkpoint_tests.rs"]
mod checkpoint_tests;

#[path = "coverage_resume_tests.rs"]
mod coverage_resume_tests;

#[path = "coverage_adoption_tests.rs"]
mod coverage_adoption_tests;

#[path = "review_resume_tests.rs"]
mod review_resume_tests;

#[tokio::test]
async fn unchanged_evidence_reuses_judgments_but_new_evidence_and_failed_calls_do_not() -> Result<()>
{
    let mut model = Model::new();
    let runtimes = BuiltinRuntimes::for_tests();
    let lesson = candidate(GOOD);
    let mut sources = vec![source()];
    let mut checks = ClaimChecks::default();
    let first = verify_with_references(
        &model,
        &runtimes,
        &lesson,
        &ReferenceCollection::lexical(&sources)?,
        &mut checks,
    )
    .await?;
    assert!(first.issues.is_empty());
    assert_eq!(*model.judge_calls.lock().unwrap(), 1);

    let mut unrelated = source();
    unrelated.excerpt = "Galaxies orbit stars.".into();
    sources.push(unrelated);
    let expanded = verify_with_references(
        &model,
        &runtimes,
        &lesson,
        &ReferenceCollection::lexical(&sources)?,
        &mut checks,
    )
    .await?;
    assert_eq!(expanded.sources.len(), 2);
    assert!(expanded.issues.is_empty());
    assert_eq!(*model.judge_calls.lock().unwrap(), 1, "Adding an unrelated source should still retrieve current evidence without repeating an identical judgment");

    let mut changed = source();
    changed.excerpt = BAD.into();
    sources.push(changed);
    model.failing_judge = true;
    let failed = verify_with_references(
        &model,
        &runtimes,
        &lesson,
        &ReferenceCollection::lexical(&sources)?,
        &mut checks,
    )
    .await?;
    assert_eq!(failed.findings[0].verdict, ClaimVerdict::Unverified);
    assert!(
        !failed.issues.is_empty(),
        "New evidence cannot inherit an earlier approval"
    );
    assert_eq!(*model.judge_calls.lock().unwrap(), 2);
    model.failing_judge = false;
    verify_with_references(
        &model,
        &runtimes,
        &lesson,
        &ReferenceCollection::lexical(&sources)?,
        &mut checks,
    )
    .await?;
    assert_eq!(
        *model.judge_calls.lock().unwrap(),
        3,
        "Failed checks must be attempted again"
    );

    let edited = verify_with_references(
        &model,
        &runtimes,
        &candidate(BAD),
        &ReferenceCollection::lexical(&sources)?,
        &mut checks,
    )
    .await?;
    assert_eq!(edited.findings[0].verdict, ClaimVerdict::Contradicted);
    assert_eq!(*model.judge_calls.lock().unwrap(), 4);
    Ok(())
}

#[tokio::test]
async fn repairing_one_section_preserves_only_unchanged_claim_comparisons() -> Result<()> {
    let model = Model::new();
    let runtimes = BuiltinRuntimes::for_tests();
    let sources = vec![source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let mut lesson = candidate(GOOD);
    lesson["blocks"]
        .as_array_mut()
        .unwrap()
        .push(candidate(BAD)["blocks"][0].clone());
    let mut checks = ClaimChecks::default();
    let first =
        verify_with_references(&model, &runtimes, &lesson, &references, &mut checks).await?;
    assert_eq!(*model.judge_calls.lock().unwrap(), 2);
    assert_eq!(first.findings[1].verdict, ClaimVerdict::Contradicted);
    lesson["blocks"][1] = candidate(GOOD)["blocks"][0].clone();
    let repaired =
        verify_with_references(&model, &runtimes, &lesson, &references, &mut checks).await?;
    assert!(repaired.issues.is_empty());
    assert_eq!(
        *model.judge_calls.lock().unwrap(),
        3,
        "Repairing one section must not repeat the other section's identical comparison"
    );
    // Changed neighboring wording still needs coverage review, but an identical
    // standalone factual statement keeps its completed evidence comparison.
    lesson["blocks"][0]["body"] = json!(format!(
        "{} Additional wording.",
        lesson["blocks"][0]["body"].as_str().unwrap()
    ));
    verify_with_references(&model, &runtimes, &lesson, &references, &mut checks).await?;
    assert_eq!(*model.judge_calls.lock().unwrap(), 3);
    Ok(())
}

#[tokio::test]
async fn reused_comparisons_keep_current_retrieval_metadata_and_exact_source_identity() -> Result<()>
{
    let model = Model::new();
    let lesson = candidate(GOOD);
    let report = verify(&model, &lesson, &[source()]).await?;
    let finding = &report.findings[0];
    let claim = Claim {
        quote: finding.quote.clone(),
        statement: finding.statement.clone(),
    };
    let key = ClaimChecks::key(finding.unit, &claim, &finding.evidence);
    let mut checks = ClaimChecks::default();
    checks.record(key.clone(), finding);
    let mut evidence = finding.evidence.clone();
    evidence[0].score += 1.0;
    assert_eq!(key, ClaimChecks::key(finding.unit, &claim, &evidence));
    assert_eq!(
        checks
            .get(&key, finding.unit, &claim, &evidence)
            .unwrap()
            .evidence[0]
            .score,
        evidence[0].score
    );
    evidence[0].source_id.push_str("-new-version");
    assert!(checks
        .get(
            &ClaimChecks::key(finding.unit, &claim, &evidence),
            finding.unit,
            &claim,
            &evidence
        )
        .is_none());
    evidence = finding.evidence.clone();
    evidence[0].text.push(' ');
    assert_ne!(key, ClaimChecks::key(finding.unit, &claim, &evidence));
    assert_ne!(
        key,
        ClaimChecks::key(finding.unit + 1, &claim, &finding.evidence)
    );
    assert!(
        ClaimChecks::default()
            .get(&key, finding.unit, &claim, &finding.evidence)
            .is_none(),
        "Judgments are not inherited across preparation operations"
    );
    Ok(())
}

#[test]
fn evidence_order_is_stable_without_discarding_conflicting_or_changed_passages() {
    let passage = |source: &str, text: &str| EvidencePassage {
        source_id: source.into(),
        text: text.into(),
        start_byte: 0,
        end_byte: text.len(),
        retrieval_kind: "lexical".into(),
        score: 1.0,
    };
    let claim = Claim {
        quote: GOOD.into(),
        statement: GOOD.into(),
    };
    let mut evidence = vec![passage("b", GOOD), passage("a", BAD)];
    canonicalize_evidence(&mut evidence);
    let original_key = ClaimChecks::key(0, &claim, &evidence);
    let original_input: Vec<_> = evidence.iter().map(|p| p.text.clone()).collect();
    evidence.reverse();
    evidence[0].score = 42.0;
    canonicalize_evidence(&mut evidence);
    assert_eq!(ClaimChecks::key(0, &claim, &evidence), original_key);
    assert_eq!(
        evidence.iter().map(|p| p.text.clone()).collect::<Vec<_>>(),
        original_input
    );
    assert!(evidence.iter().any(|p| p.text == BAD));
    evidence.push(passage("c", "Additional conditions apply."));
    canonicalize_evidence(&mut evidence);
    assert_ne!(ClaimChecks::key(0, &claim, &evidence), original_key);
    evidence.pop();
    evidence[0].source_id.push_str("-new-version");
    canonicalize_evidence(&mut evidence);
    assert_ne!(ClaimChecks::key(0, &claim, &evidence), original_key);
}

#[test]
fn pending_repairs_require_identical_content_sources_and_instructions() -> Result<()> {
    let model = Model::new();
    let sources = vec![source()];
    let references = ReferenceCollection::lexical(&sources)?;
    let original = candidate(BAD);
    let schema = json!({"type":"object"});
    let pending = PendingRepair {
        fingerprint: repair_fingerprint(&model, "instructions", &schema, &original, &references),
        defects: 1,
        issue_fingerprint: Some(repair_issue_fingerprint(&[BAD.into()])),
        context: json!({"candidate":original,"failedClaims":[{"statement":BAD}]}),
    };
    let restored: PendingRepair = serde_json::from_str(&serde_json::to_string(&pending)?)?;
    assert!(restored.matches(&model, "instructions", &schema, &original, &references));
    assert!(!restored.matches(
        &model,
        "changed instructions",
        &schema,
        &original,
        &references
    ));
    assert!(!restored.matches(
        &model,
        "instructions",
        &schema,
        &candidate(GOOD),
        &references
    ));
    assert!(!restored.matches(
        &model,
        "instructions",
        &json!({"type":"array"}),
        &original,
        &references
    ));
    let mut changed = sources.clone();
    changed[0].excerpt.push_str(" Changed evidence.");
    assert!(!restored.matches(
        &model,
        "instructions",
        &schema,
        &original,
        &ReferenceCollection::lexical(&changed)?
    ));
    let mut expanded = sources.clone();
    expanded.push(source());
    assert!(!restored.matches(
        &model,
        "instructions",
        &schema,
        &original,
        &ReferenceCollection::lexical(&expanded)?
    ));
    let mut approved = restored;
    approved.defects = 0;
    assert!(!approved.matches(&model, "instructions", &schema, &original, &references));
    Ok(())
}

#[test]
fn repair_progress_tracks_defect_identity_instead_of_only_the_count() {
    let first = repair_issue_fingerprint(&[
        "Section 1: unsupported toolchain claim".into(),
        "Section 2: contradicted profile claim".into(),
    ]);
    let reordered = repair_issue_fingerprint(&[
        " Section 2: contradicted   profile claim ".into(),
        "Section 1: unsupported toolchain claim".into(),
    ]);
    let changed = repair_issue_fingerprint(&[
        "Section 1: unsupported toolchain claim".into(),
        "Section 3: a different unsupported claim".into(),
    ]);

    assert_eq!(first, reordered);
    assert_ne!(first, changed);
}

#[tokio::test]
async fn failed_rewrite_resumes_its_checkpoint_and_checks_the_new_candidate() -> Result<()> {
    use crate::features::learning::{
        curriculum::LearningGenerationJobKind,
        curriculum_repository::LearningCurriculumRepository,
        dto::AcceptLearningProgramRequestDto,
        lesson_drafts,
        plan_dto::{LearningGenerationJobActionRequestDto, StartLearningGenerationJobRequestDto},
        repository::LearningRepository,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct InterruptedRepair {
        model: Model,
        repairs: AtomicUsize,
        bad_checks: AtomicUsize,
        good_checks: AtomicUsize,
    }
    #[async_trait::async_trait]
    impl LLMPort for InterruptedRepair {
        async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
            let mut replies = Vec::new();
            for prompt in fixture_prompts(request) {
                replies.push(self.respond(&prompt).await?);
            }
            Ok(fixture_completion(request, replies))
        }
        fn model_name(&self) -> &str {
            self.model.model_name()
        }
        fn count_tokens(&self, text: &str) -> usize {
            self.model.count_tokens(text)
        }
        fn max_context_tokens(&self) -> usize {
            self.model.max_context_tokens()
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }
    impl InterruptedRepair {
        async fn respond(&self, prompt: &str) -> Result<String> {
            if prompt.starts_with("Repair verified lesson defects.") {
                if self.repairs.fetch_add(1, Ordering::Relaxed) == 1 {
                    return Err(AppError::Network(
                        "fixture server failure during the second section repair".into(),
                    ));
                }
                let mut patch = self::context(prompt)["candidate"].clone();
                for block in patch["blocks"].as_array_mut().unwrap() {
                    block["body"] = json!(block["body"].as_str().unwrap().replace(BAD, GOOD));
                }
                return Ok(repair_body(prompt, patch["blocks"][0]["body"].clone()));
            }
            if prompt.starts_with("Source passages:") {
                if prompt.split("\n\nClaim: ").nth(1).unwrap().starts_with(BAD) {
                    self.bad_checks.fetch_add(1, Ordering::Relaxed);
                } else {
                    self.good_checks.fetch_add(1, Ordering::Relaxed);
                }
            }
            self.model.respond(prompt).await
        }
    }

    let pool = crate::features::learning::tests::pool().await?;
    let repository = LearningRepository::new(pool.clone());
    let program = crate::features::learning::tests::fixture();
    repository.create(&program).await?;
    repository
        .accept(&AcceptLearningProgramRequestDto {
            program_id: program.summary.id.clone(),
            expected_revision: 0,
            title: program.summary.title.clone(),
        })
        .await?;
    let program = repository.get(&program.summary.id).await?;
    let jobs = LearningCurriculumRepository::new(pool.clone());
    jobs.plan(&program.summary.id).await?;
    let target = &program.modules[0].lessons[0].id;
    let job = jobs
        .start_job(&StartLearningGenerationJobRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.summary.id.clone(),
            expected_revision: program.summary.revision,
            kind: LearningGenerationJobKind::LessonPreparation,
            request_json: json!({"lessonIds":[target]}).to_string(),
            progress_total: 1,
        })
        .await?;
    JobStore::new(pool.clone()).claim(&job.id).await?;
    let model = InterruptedRepair {
        model: Model::new(),
        repairs: AtomicUsize::new(0),
        bad_checks: AtomicUsize::new(0),
        good_checks: AtomicUsize::new(0),
    };
    let sources = vec![source()];
    let mut original = candidate(BAD);
    let mut second = original["blocks"][0].clone();
    second["title"] = json!("A second section needing repair");
    original["blocks"].as_array_mut().unwrap().push(second);
    let raw = original.to_string();
    let first = lesson_drafts::run(&jobs, &job.id, target, async {
        lesson_drafts::resume("same-inputs".into()).await?;
        lesson_drafts::save(&raw).await?;
        verify_and_repair(&model, "{}", &json!({}), raw.clone(), &sources, 4000).await
    })
    .await;
    let error = first.unwrap_err();
    JobStore::new(pool.clone())
        .fail(&job.id, "generation_failed", &error.to_string())
        .await?;
    let partial = jobs.lesson_draft(&job.id, target).await?.unwrap();
    assert!(partial.pending_repair.is_some());
    let partial: Value = serde_json::from_str(&partial.candidate)?;
    assert!(partial["blocks"][0]["body"]
        .as_str()
        .unwrap()
        .contains(GOOD));
    assert!(partial["blocks"][1]["body"].as_str().unwrap().contains(BAD));
    let retry = jobs
        .retry_job(&LearningGenerationJobActionRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.summary.id.clone(),
            job_id: job.id,
            expected_revision: program.summary.revision,
        })
        .await?;
    JobStore::new(pool.clone()).claim(&retry.id).await?;
    let (_, report) = lesson_drafts::run(&jobs, &retry.id, target, async {
        let saved = lesson_drafts::resume("same-inputs".into()).await?.unwrap();
        assert!(
            has_pending_repair(
                &model,
                "{}",
                &json!({}),
                &saved,
                &ReferenceCollection::lexical(&sources)?
            )
            .await?
        );
        verify_and_repair(&model, "{}", &json!({}), saved, &sources, 4000).await
    })
    .await?;
    assert_eq!(model.repairs.load(Ordering::Relaxed), 3);
    assert_eq!(
        model.bad_checks.load(Ordering::Relaxed),
        2,
        "Retry must resume the rewrite, not repeat its old diagnosis"
    );
    assert_eq!(
        model.good_checks.load(Ordering::Relaxed),
        2,
        "Rewritten content must undergo fresh evidence checking"
    );
    assert!(report.issues.is_empty());
    assert!(report
        .findings
        .iter()
        .all(|f| f.verdict == ClaimVerdict::Supported));
    Ok(())
}

#[tokio::test]
async fn repairs_continue_with_fewer_defects_and_escalate_repeated_defects() -> Result<()> {
    const SECOND: &str = "DictReader removes all whitespace from CSV values by default.";
    struct GradualRepair {
        inner: Model,
        repairs: std::sync::atomic::AtomicUsize,
        repeats_once: bool,
        repeated_escalations: std::sync::atomic::AtomicUsize,
    }
    #[async_trait::async_trait]
    impl LLMPort for GradualRepair {
        async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
            let mut replies = Vec::new();
            for prompt in fixture_prompts(request) {
                replies.push(self.respond(&prompt).await?);
            }
            Ok(fixture_completion(request, replies))
        }
        fn model_name(&self) -> &str {
            "gradual-repair-fixture"
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
    impl GradualRepair {
        async fn respond(&self, prompt: &str) -> Result<String> {
            if prompt.starts_with("Extract lesson claims.") {
                let data = self::context(prompt);
                let unit = &data["units"][0];
                let passage = unit["passages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|p| p["field"] == "/body")
                    .unwrap();
                let text = passage["text"].as_str().unwrap();
                let claims: Vec<_> = [BAD, SECOND, GOOD]
                    .into_iter()
                    .filter(|s| text.contains(s))
                    .map(|statement| json!({"passageId":passage["id"],"statement":statement}))
                    .collect();
                return Ok(inventory::fixture_response(vec![
                    json!({"index":0,"claims":claims,"nonFactualReason":""}),
                ])
                .to_string());
            }
            if prompt.starts_with("Repair verified lesson defects.") {
                let data = self::context(prompt);
                if data["repeatedDefects"].as_bool() == Some(true) {
                    self.repeated_escalations
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                let repair = self
                    .repairs
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                assert!(repair < 3, "The repeated defect should converge on removal");
                let mut body = candidate(if repair == 0 || (self.repeats_once && repair == 1) {
                    SECOND
                } else {
                    GOOD
                })["blocks"][0]["body"]
                    .as_str()
                    .unwrap()
                    .to_owned();
                if self.repeats_once && repair == 1 {
                    body.push(' ');
                }
                return Ok(repair_body(prompt, json!(body)));
            }
            if prompt.starts_with("Source passages:")
                && prompt
                    .split("\n\nClaim: ")
                    .nth(1)
                    .unwrap()
                    .starts_with(SECOND)
            {
                return Ok("contradicted\nReason: The fixture reference preserves whitespace rather than removing it.\nSource passage: passage-0".into());
            }
            self.inner.respond(prompt).await
        }
    }

    for repeats_once in [false, true] {
        let model = GradualRepair {
            inner: Model::new(),
            repairs: Default::default(),
            repeats_once,
            repeated_escalations: Default::default(),
        };
        let (_, report) = verify_and_repair(
            &model,
            "{}",
            &json!({}),
            candidate(&format!("{BAD} {SECOND}")).to_string(),
            &[source()],
            3000,
        )
        .await?;
        assert_eq!(
            model.repairs.load(std::sync::atomic::Ordering::Relaxed),
            if repeats_once { 3 } else { 2 }
        );
        assert_eq!(
            model
                .repeated_escalations
                .load(std::sync::atomic::Ordering::Relaxed),
            usize::from(repeats_once)
        );
        assert!(report.issues.is_empty());
        assert!(report
            .findings
            .iter()
            .all(|finding| finding.verdict == ClaimVerdict::Supported));
    }
    Ok(())
}

#[tokio::test]
async fn contradictory_claim_is_repaired_and_rechecked_against_references() -> Result<()> {
    let model = Model::new();
    let bad = verify(&model, &candidate(BAD), &[source()]).await?;
    assert!(!bad.issues.is_empty());
    assert_eq!(bad.findings[0].verdict, ClaimVerdict::Contradicted);
    assert!(bad.executions.is_empty());
    let (raw, report) = verify_and_repair(
        &model,
        "{}",
        &json!({"type":"object"}),
        candidate(BAD).to_string(),
        &[source()],
        3000,
    )
    .await?;
    assert!(*model.repaired.lock().unwrap());
    assert!(raw.contains(GOOD));
    assert!(report.issues.is_empty());
    assert!(report
        .findings
        .iter()
        .all(|f| f.verdict == ClaimVerdict::Supported));
    Ok(())
}

#[tokio::test]
async fn repair_shares_exact_passages_without_dropping_conflicting_evidence() -> Result<()> {
    let model = Model::new();
    let candidate = candidate(BAD);
    let mut report = verify(&model, &candidate, &[source()]).await?;
    let mut finding = report.findings[0].clone();
    let original = finding.evidence[0].clone();
    let mut conflicting = original.clone();
    conflicting.source_id = "independent-conflicting-source".into();
    conflicting.text = "Keep conflicting evidence too.\r\n  Preserve whitespace.".into();
    finding.evidence = vec![original.clone(), conflicting.clone()];
    report.findings = (0..107)
        .map(|index| {
            let mut copy = finding.clone();
            copy.statement = format!("Claim {index}: {BAD}");
            copy
        })
        .collect();
    let payload = repair_context("{}", &candidate, &report, &[], false)?;
    assert_eq!(payload["failedClaims"].as_array().unwrap().len(), 107);
    assert_eq!(payload["evidence"].as_array().unwrap().len(), 2);
    assert_eq!(payload["evidence"][0]["text"], original.text);
    assert_eq!(payload["evidence"][1]["text"], conflicting.text);
    assert_eq!(payload["evidence"][1]["sourceId"], conflicting.source_id);
    for claim in payload["failedClaims"].as_array().unwrap() {
        assert_eq!(claim["evidence"], json!(["evidence-0", "evidence-1"]));
    }
    assert!(!payload["failedClaims"][0]
        .as_object()
        .unwrap()
        .contains_key("quote"));
    Ok(())
}

#[test]
fn saved_coverage_rejects_foreign_duplicate_or_contradictory_locations() {
    let content = units(&candidate(GOOD)).unwrap();
    let gap = |passage_id: &str| MissingAssertion {
        passage_id: passage_id.into(),
        statement: "Missing condition".into(),
    };
    let valid = CoverageUnit {
        index: 0,
        complete: false,
        reason: "Missing condition".into(),
        missing_assertions: vec![gap("unit-0-passage-0")],
    };
    assert!(valid_saved_coverage(
        &Coverage {
            units: vec![valid.clone()]
        },
        &content
    ));
    assert!(!valid_saved_coverage(
        &Coverage {
            units: vec![valid.clone(), valid.clone()]
        },
        &content
    ));
    for changed in [
        CoverageUnit {
            index: 1,
            ..valid.clone()
        },
        CoverageUnit {
            complete: true,
            ..valid.clone()
        },
        CoverageUnit {
            complete: false,
            missing_assertions: Vec::new(),
            ..valid.clone()
        },
        CoverageUnit {
            missing_assertions: vec![gap("unit-1-passage-0")],
            ..valid.clone()
        },
        CoverageUnit {
            missing_assertions: vec![MissingAssertion {
                statement: "   ".into(),
                ..gap("unit-0-passage-0")
            }],
            ..valid
        },
    ] {
        assert!(!valid_saved_coverage(
            &Coverage {
                units: vec![changed]
            },
            &content
        ));
    }
}

#[tokio::test]
async fn missing_evidence_failed_judge_and_incomplete_coverage_never_approve() -> Result<()> {
    let model = Model::new();
    let no_evidence = verify(&model, &candidate("A short factual definition."), &[]).await?;
    assert_eq!(no_evidence.findings[0].verdict, ClaimVerdict::Unsupported);
    assert!(!no_evidence.issues.is_empty());
    let offline = Model {
        failing_judge: true,
        ..Model::new()
    };
    let report = verify(&offline, &candidate(GOOD), &[source()]).await?;
    assert_eq!(report.findings[0].verdict, ClaimVerdict::Unverified);
    assert!(verify_and_repair(
        &offline,
        "{}",
        &json!({}),
        candidate(GOOD).to_string(),
        &[source()],
        3000
    )
    .await
    .is_err());
    assert!(!*offline.repaired.lock().unwrap());
    let incomplete = Model {
        incomplete_coverage: true,
        ..Model::new()
    };
    assert!(verify(&incomplete, &candidate(GOOD), &[source()])
        .await
        .is_err());
    Ok(())
}

#[test]
fn omitted_duplicate_and_fabricated_inventory_units_are_rejected() {
    let content = units(&candidate(GOOD)).unwrap();
    let valid = UnitClaims {
        index: 0,
        claims: vec![Claim {
            quote: GOOD.into(),
            statement: GOOD.into(),
        }],
        non_factual_reason: String::new(),
    };
    assert!(validate_inventory(
        &Inventory {
            units: vec![valid.clone()]
        },
        &content
    )
    .is_ok());
    assert!(validate_inventory(&Inventory { units: vec![] }, &content).is_err());
    assert!(validate_inventory(
        &Inventory {
            units: vec![valid.clone(), valid.clone()]
        },
        &content
    )
    .is_err());
    let mut fabricated = valid;
    fabricated.claims[0].quote = "invented".into();
    assert!(validate_inventory(
        &Inventory {
            units: vec![fabricated]
        },
        &content
    )
    .is_err());
}

/// Persistence-only tests use an explicit synthetic attestation. This is
/// compiled only for tests and does not bypass production verification.
pub(crate) fn attest(lesson_id: &str, lesson: &PreparedLearningLesson) -> LessonVerificationReport {
    // Persistence fixtures model a complete report shape; live factual checks
    // are exercised separately and this helper is never compiled in production.
    let coverage: Vec<_> = lesson
        .blocks
        .iter()
        .map(|block| &block.body)
        .chain(lesson.questions.iter().map(|question| &question.prompt))
        .enumerate()
        .map(|(index, text)| UnitClaims {
            index,
            claims: vec![Claim {
                quote: text.clone(),
                statement: text.clone(),
            }],
            non_factual_reason: String::new(),
        })
        .collect();
    let findings = coverage
        .iter()
        .flat_map(|unit| {
            unit.claims.iter().map(|claim| Finding {
                unit: unit.index,
                quote: claim.quote.clone(),
                statement: claim.statement.clone(),
                verdict: ClaimVerdict::Supported,
                reason: "Persistence fixture".into(),
                evidence: vec![],
                supporting_quote: None,
            })
        })
        .collect();
    LessonVerificationReport {
        policy: POLICY.into(),
        lesson_id: lesson_id.into(),
        content_sha256: content_hash(lesson).unwrap(),
        checker_model: "persistence-fixture".into(),
        checked_at: 0,
        sources: vec![],
        coverage,
        coverage_audit: (0..lesson.blocks.len() + lesson.questions.len())
            .map(|index| CoverageUnit {
                index,
                complete: true,
                reason: "Persistence fixture".into(),
                missing_assertions: Vec::new(),
            })
            .collect(),
        findings,
        executions: vec![],
        issues: vec![],
        retrieval_mode: "lexical_fallback".into(),
        embedding_model: None,
        unexecuted_languages: vec![],
    }
}

#[tokio::test]
async fn publication_requires_current_report_and_source_and_rolls_back_on_rejection() -> Result<()>
{
    use crate::features::learning::{dto::*, repository::LearningRepository};
    let pool = crate::features::learning::tests::pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let mut program = crate::features::learning::tests::fixture();
    let reference = source();
    program.sources = vec![reference];
    repo.create(&program).await?;
    repo.accept(&AcceptLearningProgramRequestDto {
        program_id: program.summary.id.clone(),
        expected_revision: 0,
        title: program.summary.title.clone(),
    })
    .await?;
    let program = repo.get(&program.summary.id).await?;
    let lesson_id = &program.modules[0].lessons[0].id;
    let sources = repo.verification_sources(&program.summary.id).await?;
    let (raw, report) = verify_and_repair(
        &Model::new(),
        "{}",
        &json!({}),
        candidate(BAD).to_string(),
        &sources,
        3000,
    )
    .await?;
    let value: Value = serde_json::from_str(&raw)?;
    let mut prepared = PreparedLearningLesson {
        blocks: vec![LearningBlockDto {
            kind: LearningBlockKind::Explanation,
            title: "CSV header names".into(),
            body: value["blocks"][0]["body"].as_str().unwrap().into(),
            rubric: vec![],
            source_ids: vec![sources[0].id.clone()],
        }],
        questions: vec![],
        keys: vec![],
        verification: None,
    };
    assert!(repo
        .prepare(
            &program.summary.id,
            lesson_id,
            program.summary.revision,
            &prepared
        )
        .await
        .is_err());
    prepared.verification = Some(report.bind(lesson_id, &prepared)?);
    let mut changed = prepared.clone();
    changed.blocks[0].body = BAD.into();
    assert!(repo
        .prepare(
            &program.summary.id,
            lesson_id,
            program.summary.revision,
            &changed
        )
        .await
        .is_err());
    assert_eq!(
        repo.get(&program.summary.id).await?.summary.revision,
        program.summary.revision
    );
    assert!(prepared
        .verification
        .as_ref()
        .unwrap()
        .validate("different-lesson", &prepared)
        .is_err());
    sqlx::query("UPDATE learning_source_library SET deleted_at=1 WHERE program_id=?")
        .bind(&program.summary.id)
        .execute(&pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    assert!(repo
        .prepare(
            &program.summary.id,
            lesson_id,
            program.summary.revision,
            &prepared
        )
        .await
        .is_err());
    sqlx::query("UPDATE learning_source_library SET deleted_at=NULL WHERE program_id=?")
        .bind(&program.summary.id)
        .execute(&pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    repo.prepare(
        &program.summary.id,
        lesson_id,
        program.summary.revision,
        &prepared,
    )
    .await?;
    let saved = repo.get(&program.summary.id).await?;
    assert_eq!(
        saved.modules[0].lessons[0].preparation,
        LearningPreparation::Ready
    );
    let report_json: String = sqlx::query_scalar(
        "SELECT report_json FROM learning_lesson_verifications WHERE lesson_id=?",
    )
    .bind(lesson_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;
    assert!(report_json.contains("lexical_fallback"));
    assert!(!serde_json::to_string(&saved)?.contains("checker_model"));
    Ok(())
}

#[tokio::test]
async fn exact_worked_code_is_executed_and_bad_output_cannot_hide_a_failed_run() -> Result<()> {
    let runtimes = BuiltinRuntimes::for_tests();
    let good = json!({"blocks":[{"kind":"worked_example","body":"```javascript\nconsole.log(2 + 3);\n```"}]});
    let observed = execution::observe(&runtimes, &good).await?;
    assert!(observed[0].passed());
    assert!(observed[0].evidence().contains('5'));
    let bad = json!({"blocks":[{"kind":"worked_example","body":"```javascript\nthrow new Error('broken example');\n```"}]});
    assert!(!execution::observe(&runtimes, &bad).await?[0].passed());
    assert!(execution::observe(
        &runtimes,
        &json!({"blocks":[{"kind":"worked_example","body":"```python\nprint(1)"}]})
    )
    .await
    .is_err());
    Ok(())
}

#[tokio::test]
async fn unlabeled_data_and_code_are_labeled_without_changing_their_bytes() -> Result<()> {
    let runtimes = BuiltinRuntimes::for_tests();
    use crate::features::learning::lessons::course_generation_tests::ScriptedModel;
    let model=ScriptedModel {outputs:Mutex::new(vec![json!({"fences":[{"id":"block-0-fence-0","language":"text"},{"id":"block-0-fence-1","language":"javascript"}]}).to_string()].into()),prompts:Mutex::new(Vec::new())};
    let tree = "project/\n└── main.rs\n";
    let code = "console.log(' a  b ');\n";
    let body = format!("Directory tree:\n```\n{tree}```\nRunnable example:\n```\n{code}```\n");
    let lesson = json!({"blocks":[{"kind":"worked_example","title":"Tree and program","body":body}],"questions":[]});
    let corrected: Value =
        serde_json::from_str(&normalize_example_fences(&model, lesson.to_string()).await?)?;
    assert_eq!(
        corrected["blocks"][0]["body"],
        format!(
            "Directory tree:\n```text\n{tree}```\nRunnable example:\n```javascript\n{code}```\n"
        )
    );
    let observed = execution::observe(&runtimes, &corrected).await?;
    assert_eq!(observed.len(), 1);
    assert!(observed[0].passed());
    assert!(observed[0].evidence().contains(" a  b "));
    let titled = json!({"blocks":[{"kind":"worked_example","body":"```javascript title=example.js\nconsole.log(42);\n```"}]});
    assert!(execution::observe(&runtimes, &titled).await?[0].passed());
    Ok(())
}

#[tokio::test]
async fn invalid_fence_classification_cannot_hide_an_unexecuted_program() {
    let runtimes = BuiltinRuntimes::for_tests();
    use crate::features::learning::lessons::course_generation_tests::ScriptedModel;
    let lesson = json!({"blocks":[{"kind":"worked_example","body":"```\nconsole.log(42);\n```"}],"questions":[]});
    for response in [
        json!({"fences":[]}),
        json!({"fences":[{"id":"invented","language":"text"}]}),
        json!({"fences":[{"id":"block-0-fence-0","language":"invalid-label"}]}),
    ] {
        let model = ScriptedModel {
            outputs: Mutex::new(vec![response.to_string()].into()),
            prompts: Mutex::new(Vec::new()),
        };
        assert!(normalize_example_fences(&model, lesson.to_string())
            .await
            .is_err());
    }
    assert!(execution::observe(&runtimes, &lesson).await.is_err());
}

struct TypedModel {
    response: CompletionResponse,
}

#[tokio::test]
async fn durable_judge_recovers_truncated_output_without_using_it_as_approval() {
    struct TruncatedJudge(std::sync::atomic::AtomicUsize);
    #[async_trait::async_trait]
    impl LLMPort for TruncatedJudge {
        async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
            assert!(request.no_time_limit);
            assert!(request.max_output_tokens.unwrap() > 512);
            assert_eq!(request.reasoning_effort.as_deref(), Some("low"));
            assert!(!request.want_logprobs);
            let call = self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if call == 0 {
                return Ok(CompletionResponse {
                    text: "Reason: Incomplete and not trustworthy".into(),
                    finish_reason: "length".into(),
                    ..Default::default()
                });
            }
            assert!(serde_json::to_string(&request.input)?.contains("Correct the response format"));
            Ok(CompletionResponse {
                text: "Reason: The reference does not state the claimed date.\nSource passage: none\nVerdict: unsupported".into(),
                finish_reason: "stop".into(),
                ..Default::default()
            })
        }

        fn model_name(&self) -> &str {
            "truncated-judge"
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
    let model = TruncatedJudge(Default::default());
    let result = ClaimChecker::new(
        &model,
        SamplingOverride::deterministic(),
        512,
        CheckPolicy::Strict,
    )
    .check_passages_without_deadline(
        "Rust was released on the claimed date.",
        &["Rust is a language.".into()],
    )
    .await;
    assert!(
        matches!(result,ClaimJudgment::Judged(outcome) if outcome.verdict == ClaimVerdict::Unsupported)
    );
    assert_eq!(model.0.load(std::sync::atomic::Ordering::Relaxed), 2);
}

#[async_trait::async_trait]
impl LLMPort for TypedModel {
    async fn complete(&self, _: &CompletionRequest) -> Result<CompletionResponse> {
        Ok(self.response.clone())
    }
    fn model_name(&self) -> &str {
        "typed-fixture"
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
#[tokio::test]
async fn located_evidence_preserves_source_bytes_and_rejects_invented_locations() {
    let source = "A wrapped factual\nstatement.\n```text\na  b\n```".to_string();
    for (citation, valid) in [
        ("passage-0", true),
        ("passage-9", false),
        ("invented", false),
        ("none", false),
    ] {
        let model=TypedModel {response:CompletionResponse {text:format!("Reason: The source contains the wrapped factual statement.\nSource passage: {citation}\nVerdict: supported"),finish_reason:"stop".into(),..Default::default()}};
        let result = ClaimChecker::new(
            &model,
            SamplingOverride::deterministic(),
            512,
            CheckPolicy::Strict,
        )
        .check_passages_without_deadline(
            "A wrapped factual statement.",
            std::slice::from_ref(&source),
        )
        .await;
        if valid {
            assert!(
                matches!(result,ClaimJudgment::Judged(outcome) if outcome.quote.as_deref()==Some(source.as_str()))
            );
        } else {
            assert!(matches!(result, ClaimJudgment::Unusable));
        }
    }
    let misleading_first_token = TypedModel {
        response: CompletionResponse {
            text: "Reason: The source matches.\nSource passage: passage-0\nVerdict: supported"
                .into(),
            finish_reason: "stop".into(),
            first_token_logprobs: Some(vec![
                ("supported".into(), 0.5_f32.ln()),
                ("unsupported".into(), 0.5_f32.ln()),
            ]),
            ..Default::default()
        },
    };
    assert!(matches!(
        ClaimChecker::new(
            &misleading_first_token,
            SamplingOverride::deterministic(),
            512,
            CheckPolicy::Strict
        )
        .check_passages_without_deadline("A wrapped factual statement.", &[source])
        .await,
        ClaimJudgment::Judged(outcome) if outcome.verdict == ClaimVerdict::Supported && outcome.confidence.is_none()
    ));
}

#[tokio::test]
async fn durable_judgment_requires_one_final_verdict_after_its_comparison() {
    for text in [
        "supported\nReason: The source instead says it is a script, not a binary.\nSource passage: passage-0",
        "Verdict: supported\nReason: The comparison is not finished.\nSource passage: passage-0",
        "Reason: The source agrees.\nSource passage: passage-0\nVerdict: supported\nVerdict: unsupported",
        "Reason: The source agrees. Verdict: unsupported\nSource passage: passage-0\nVerdict: supported",
        "Reason: The source agrees. Verdict: supported Additional text.\nSource passage: passage-0\nVerdict: supported",
        "Reason: The source agrees. Source passage: passage-0 Verdict: supported Verdict: unsupported",
        "Reason: First reason. Reason: Replacement reason. Source passage: passage-0 Verdict: supported",
        "Reason: First reason. Reason: Replacement reason.\nSource passage: passage-0\nVerdict: supported",
        "Reason: The source agrees. Source passage: passage-0 Source passage: passage-1 Verdict: supported",
        "Reason: The source agrees. Verdict: supported Source passage: passage-0",
        "Reason: The source agrees. Source passage: passage-0 Verdict: supported Additional text.",
    ] {
        let model = TypedModel {
            response: CompletionResponse { text: text.into(), finish_reason: "stop".into(), ..Default::default() },
        };
        let result = ClaimChecker::new(&model, SamplingOverride::deterministic(), 512, CheckPolicy::Strict)
            .check_passages_without_deadline("The command downloads a binary.", &["The command downloads a script.".into()]).await;
        assert!(matches!(result, ClaimJudgment::Unusable));
    }
}

#[tokio::test]
async fn durable_judgment_accepts_an_identical_decision_echo_in_the_reason() {
    let source = "The setup process attempts to configure the environment.".to_owned();
    for verdict in ["supported", "unsupported", "contradicted"] {
        let model = TypedModel {
            response: CompletionResponse {
                text: format!("Reason: The source was compared with every assertion. Verdict: {verdict}\nSource passage: passage-0\nVerdict: {verdict}"),
                finish_reason: "stop".into(),
                ..Default::default()
            },
        };
        let result = ClaimChecker::new(
            &model,
            SamplingOverride::deterministic(),
            512,
            CheckPolicy::Strict,
        )
        .check_passages_without_deadline(
            "The setup configures the environment.",
            std::slice::from_ref(&source),
        )
        .await;
        let ClaimJudgment::Judged(outcome) = result else {
            panic!("An identical echo made a canonical result unusable");
        };
        assert_eq!(serde_json::to_value(outcome.verdict).unwrap(), verdict);
        if verdict != "unsupported" {
            assert_eq!(outcome.quote.as_deref(), Some(source.as_str()));
        }
    }
}

#[tokio::test]
async fn durable_judgment_accepts_inline_fields_without_changing_its_evidence() {
    let source = "cargo: the Rust dependency manager and build tool".to_owned();
    for text in [
        "Reason: Passage-0 explicitly describes cargo as the Rust dependency manager and build tool. Source passage: passage-0\nVerdict: supported",
        "Reason: Passage-0 explicitly describes cargo as the Rust dependency manager and build tool. Source passage: passage-0 Verdict: supported",
        "Reason: Passage-0 explicitly describes cargo as the Rust dependency\nmanager and build tool.\nSource passage: passage-0\nVerdict: supported",
    ] {
        let model = TypedModel {
            response: CompletionResponse { text: text.into(), finish_reason: "stop".into(), ..Default::default() },
        };
        let result = ClaimChecker::new(&model, SamplingOverride::deterministic(), 512, CheckPolicy::Strict)
            .check_passages_without_deadline("cargo is the Rust dependency manager and build tool.", std::slice::from_ref(&source)).await;
        assert!(matches!(result, ClaimJudgment::Judged(outcome) if outcome.verdict == ClaimVerdict::Supported && outcome.quote.as_deref() == Some(source.as_str())));
    }
    for citation in ["passage-9", "none", "invented"] {
        let model = TypedModel {
            response: CompletionResponse {
                text: format!(
                    "Reason: The source agrees. Source passage: {citation} Verdict: supported"
                ),
                finish_reason: "stop".into(),
                ..Default::default()
            },
        };
        let result = ClaimChecker::new(
            &model,
            SamplingOverride::deterministic(),
            512,
            CheckPolicy::Strict,
        )
        .check_passages_without_deadline(
            "cargo is the Rust dependency manager and build tool.",
            std::slice::from_ref(&source),
        )
        .await;
        assert!(matches!(result, ClaimJudgment::Unusable));
    }
}

#[tokio::test]
async fn source_passage_prose_does_not_create_a_second_protocol_field() {
    let source = "The measured value is 12.".to_owned();
    for text in [
        "Reason: The factual assertion maps directly to a source passage: the measured value is 12.\nSource passage: passage-0\nVerdict: supported",
        "Reason: One source passage: the measured value is 12. Another source passage: the measurement is recorded.\nSource passage: passage-0\nVerdict: supported",
    ] {
        let model = TypedModel { response: CompletionResponse { text: text.into(), finish_reason: "stop".into(), ..Default::default() } };
        let result = ClaimChecker::new(&model, SamplingOverride::deterministic(), 512, CheckPolicy::Fidelity)
            .check_passages_without_deadline("The measured value is 12.", std::slice::from_ref(&source)).await;
        assert!(matches!(result, ClaimJudgment::Judged(outcome) if outcome.verdict == ClaimVerdict::Supported && outcome.quote.as_deref() == Some(source.as_str())));
    }
    let model = TypedModel { response: CompletionResponse { text: "Reason: The source agrees.\nSource passage: passage-0\nSource passage: passage-1\nVerdict: supported".into(), finish_reason: "stop".into(), ..Default::default() } };
    let result = ClaimChecker::new(
        &model,
        SamplingOverride::deterministic(),
        512,
        CheckPolicy::Fidelity,
    )
    .check_passages_without_deadline("The measured value is 12.", &[source])
    .await;
    assert!(matches!(result, ClaimJudgment::Unusable));
}

#[tokio::test]
async fn malformed_verdict_gets_one_format_correction_without_approving_missing_evidence() {
    use crate::features::learning::lessons::course_generation_tests::ScriptedModel;
    let malformed="The passages describe Rust tools but do not mention Visual Studio.\nReason: Visual Studio is absent.\nSource quote: none";
    for corrected in [
        "unsupported\nReason: Visual Studio is absent.\nSource quote: none",
        malformed,
    ] {
        let model = ScriptedModel {
            outputs: Mutex::new(vec![malformed.into(), corrected.into()].into()),
            prompts: Mutex::new(vec![]),
        };
        let evidence = ClaimEvidence {
            text: SOURCE.into(),
            quote: None,
        };
        let result = ClaimChecker::new(
            &model,
            SamplingOverride::deterministic(),
            512,
            CheckPolicy::Strict,
        )
        .check_without_deadline("Visual Studio includes a package manager.", &evidence)
        .await;
        if corrected.starts_with("unsupported") {
            assert!(
                matches!(result,ClaimJudgment::Judged(outcome) if outcome.verdict==ClaimVerdict::Unsupported)
            );
        } else {
            assert!(matches!(result, ClaimJudgment::Unusable));
        }
        let prompts = model.prompts.lock().unwrap();
        assert_eq!(prompts.len(), 2);
        assert!(prompts[1].contains(SOURCE));
        assert!(prompts[1].contains("Correct the response format"));
    }
    use lattice_core::application::services::claim_verification::parse_verdict_word;
    assert_eq!(
        parse_verdict_word("Label: unsupported\nReason: Missing source."),
        Some(ClaimVerdict::Unsupported)
    );
    assert_eq!(
        parse_verdict_word("Verdict: supported\nReason: Matching source."),
        Some(ClaimVerdict::Supported)
    );
}

#[tokio::test]
async fn strict_checker_preserves_long_exact_quotes_and_rejects_reflowed_code() {
    let quote = format!(
        "{} The field contains two spaces: a  b.",
        "A factual reference sentence. ".repeat(20)
    );
    let evidence = ClaimEvidence {
        text: quote.clone(),
        quote: None,
    };
    for (cited, valid) in [(quote.clone(), true), (quote.replace("a  b", "a b"), false)] {
        let model=TypedModel {response:CompletionResponse {
            text:format!("supported\nReason: The exact reference contains the statement.\nSource quote: {cited}"),
            finish_reason:"stop".into(),..Default::default()
        }};
        let checker = ClaimChecker::new(
            &model,
            SamplingOverride::deterministic(),
            512,
            CheckPolicy::Strict,
        );
        let result = checker
            .check_without_deadline("The field contains two spaces.", &evidence)
            .await;
        if valid {
            assert!(
                matches!(result,ClaimJudgment::Judged(outcome) if outcome.quote.as_deref()==Some(quote.as_str()))
            );
        } else {
            assert!(matches!(result, ClaimJudgment::Unusable));
        }
    }
}

#[tokio::test]
async fn strict_checker_rejects_missing_invented_quotes_and_truncated_replies() {
    let evidence = ClaimEvidence {
        text: SOURCE.into(),
        quote: Some(GOOD.into()),
    };
    for (text, finish) in [("supported","stop"),("supported\nReason: a match\nSource quote: fabricated", "stop"),("supported\nReason: a match\nSource quote: DictReader preserves field-name whitespace by default.","length")] {
        let model=TypedModel { response:CompletionResponse { text:text.into(),finish_reason:finish.into(), ..Default::default() } };
        let strict=ClaimChecker::new(&model,SamplingOverride::deterministic(),512,CheckPolicy::Strict).check(GOOD,&evidence,Instant::now()+Duration::from_secs(5)).await;
        assert!(matches!(strict,ClaimJudgment::Unusable));
    }
    for alternatives in [
        vec![
            ("contradicted".into(), 0.9_f32.ln()),
            ("supported".into(), 0.1_f32.ln()),
        ],
        vec![
            ("supported".into(), 0.5_f32.ln()),
            ("contradicted".into(), 0.5_f32.ln()),
        ],
    ] {
        let model = TypedModel {
            response: CompletionResponse {
                text: format!(
                    "supported\nReason: The source preserves spaces.\nSource quote: {GOOD}"
                ),
                finish_reason: "stop".into(),
                first_token_logprobs: Some(alternatives),
                ..Default::default()
            },
        };
        let checker = ClaimChecker::new(
            &model,
            SamplingOverride::deterministic(),
            512,
            CheckPolicy::Strict,
        );
        assert!(matches!(
            checker
                .check(GOOD, &evidence, Instant::now() + Duration::from_secs(5))
                .await,
            ClaimJudgment::Judged(outcome) if outcome.verdict == ClaimVerdict::Unsupported && outcome.quote.is_none()
        ));
        assert!(matches!(
            checker.check(GOOD, &evidence, Instant::now()).await,
            ClaimJudgment::OutOfTime
        ));
    }
}

pub(crate) async fn install_source(
    pool: &sqlx::SqlitePool,
    program_id: &str,
    text: &str,
) -> Result<()> {
    use crate::features::learning::{
        dto::{LearningSourceKind, LearningSourcePolicy},
        source_library::{CapturedLearningSource, LearningSourceLibraryRepository},
    };
    let id = || uuid::Uuid::new_v4().to_string();
    LearningSourceLibraryRepository::new(pool.clone())
        .add(
            crate::features::learning::source_library::NewLearningSource {
                operation_id: &id(),
                source_id: &id(),
                version_id: &id(),
                program_id,
                kind: LearningSourceKind::Pasted,
                operation_kind: "add_text",
                origin: "Test reference",
                requested_url: None,
                freshness: LearningSourcePolicy::Fixed,
            },
            CapturedLearningSource {
                title: "Test reference".into(),
                publisher: None,
                requested_url: None,
                resolved_url: None,
                text: text.into(),
                truncated: false,
                extraction_version: "test".into(),
            },
            "fixture".into(),
        )
        .await
}

#[tokio::test]
async fn unsupported_languages_are_disclosed_and_csv_fixture_is_test_only() -> Result<()> {
    let runtimes = BuiltinRuntimes::for_tests();
    let rust = json!({"blocks":[{"kind":"worked_example","body":"```rust\nfn main() {}\n```"}]});
    assert!(execution::observe(&runtimes, &rust).await?.is_empty());
    assert_eq!(execution::unexecuted_languages(&rust), vec!["rust"]);
    let csv = json!({"blocks":[{"kind":"worked_example","body":format!("```python\n{}\n```", include_str!("csv_reference.py"))}]});
    let observed = execution::observe(&runtimes, &csv).await?;
    assert!(observed[0].passed());
    assert!(observed[0].evidence().contains("header_whitespace"));
    Ok(())
}

#[tokio::test]
async fn evidence_view_hides_assessment_claims_and_marks_source_changes() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let program = crate::features::learning::tests::fixture();
    let repo = crate::features::learning::repository::LearningRepository::new(pool.clone());
    repo.create(&program).await?;
    let lesson = &program.modules[0].lessons[0];
    // This fixture deliberately puts a private key in an assessment unit. The
    // public projection must omit it even when there are zero teaching blocks.
    let source = &program.sources[0];
    let saved = repo.verification_sources(&program.summary.id).await?;
    sqlx::query("INSERT INTO learning_questions(id,lesson_id,module_id,kind,prompt,options_json,source_ids_json,ordinal) VALUES(?,?,?,'quiz','Question','[]','[]',0)")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&lesson.id).bind(&program.modules[0].id).execute(&pool).await?;
    let report = json!({"policy":POLICY,"lesson_id":lesson.id,"content_sha256":"fixture","checked_at":1,"checker_model":"fixture","sources":[{"id":source.id,"sha256":digest(&saved[0].excerpt)}],"findings":[{"unit":0,"quote":"PRIVATE QUOTE","statement":"PRIVATE ANSWER KEY","verdict":"supported","reason":"PRIVATE RATIONALE","evidence":[],"supporting_quote":null}],"coverage":[],"coverage_audit":[],"executions":[],"issues":[],"retrieval_mode":"hybrid","embedding_model":null,"unexecuted_languages":[]});
    sqlx::query("INSERT INTO learning_lesson_verifications(lesson_id,program_id,policy,content_sha256,report_json,checked_at) VALUES(?,?,?,?,?,?)").bind(&lesson.id).bind(&program.summary.id).bind(POLICY).bind("fixture").bind(report.to_string()).bind(1_i64).execute(&pool).await.map_err(|e|AppError::Database(e.to_string()))?;
    let view =
        crate::features::learning::lesson_evidence::get(&pool, &program.summary.id, &lesson.id)
            .await?
            .unwrap();
    assert!(view.teaching_claims.is_empty());
    assert!(!serde_json::to_string(&view)?.contains("PRIVATE"));
    assert!(view.sources_current);
    sqlx::query("UPDATE learning_source_library SET deleted_at=1 WHERE program_id=?")
        .bind(&program.summary.id)
        .execute(&pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let view =
        crate::features::learning::lesson_evidence::get(&pool, &program.summary.id, &lesson.id)
            .await?
            .unwrap();
    assert!(!view.sources_current);

    // Missing unit identity must not silently become teaching section zero.
    let mut malformed = report.clone();
    malformed["findings"][0]
        .as_object_mut()
        .unwrap()
        .remove("unit");
    sqlx::query("UPDATE learning_lesson_verifications SET report_json=? WHERE lesson_id=?")
        .bind(malformed.to_string())
        .bind(&lesson.id)
        .execute(&pool)
        .await?;
    let error =
        crate::features::learning::lesson_evidence::get(&pool, &program.summary.id, &lesson.id)
            .await
            .unwrap_err()
            .to_string();
    assert!(error.contains("unavailable"));
    assert!(!error.contains("PRIVATE"));

    let mut foreign = report;
    foreign["lesson_id"] = json!("another-lesson");
    sqlx::query("UPDATE learning_lesson_verifications SET report_json=? WHERE lesson_id=?")
        .bind(foreign.to_string())
        .bind(&lesson.id)
        .execute(&pool)
        .await?;
    assert!(crate::features::learning::lesson_evidence::get(
        &pool,
        &program.summary.id,
        &lesson.id
    )
    .await
    .is_err());
    Ok(())
}
