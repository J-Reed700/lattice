//! The coverage audit completes the inventory itself. An assertion the fidelity
//! judge names as unrepresented is recorded as a claim and decided by the
//! evidence check, so representation can never stall or fail a lesson.
use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// An exercise that presupposes the false default without stating it in the
/// words the fixture extractor recognises, so extraction misses the premise.
const PREMISE: &str = "Exercise: assuming the reader strips field-name whitespace by default, write code that relies on that default.";
/// An exercise that presupposes a behaviour no supplied source mentions, the
/// shape of the live failure: the judge named a feature the sources lacked.
const UNSOURCED_PREMISE: &str = "Exercise: assuming the reader logs every parsed row to standard error by default, write code that silences that log.";
const UNSOURCED: &str = "DictReader logs every parsed row to standard error by default.";
/// A deliverable and rubric: the lesson's own choices, not assertions.
const RUBRIC: &str = "Deliverable: submit a markdown report. Rubric: Strong answers cite their sources; Adequate answers name the behaviour.";

struct JudgingModel {
    inner: Model,
    /// The mapper proposes an omission for this passage, as the live mapper
    /// did for instructions and rubrics; the judge answers independently.
    proposed_passage: &'static str,
    judge_reply: Value,
    /// The statement whose evidence check the test observes. When no source
    /// mentions it, the evidence judge answers unsupported, as it did live.
    watched_claim: &'static str,
    unsourced: bool,
    judged: AtomicUsize,
    judged_watched_claim: AtomicBool,
}

#[async_trait::async_trait]
impl LLMPort for JudgingModel {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let mut replies = Vec::new();
        for prompt in fixture_prompts(request) {
            replies.push(self.respond(&prompt).await?);
        }
        Ok(fixture_completion(request, replies))
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

impl JudgingModel {
    fn new(proposed_passage: &'static str, missing: &[&str], watched_claim: &'static str) -> Self {
        let reason = if missing.is_empty() {
            "Deliverables and grading levels are the lesson's own choices."
        } else {
            "The exercise presupposes a default behaviour the inventory does not state."
        };
        Self {
            inner: Model::new(),
            proposed_passage,
            judge_reply: json!({"missingAssertions":missing,"reason":reason}),
            watched_claim,
            unsourced: watched_claim == UNSOURCED,
            judged: AtomicUsize::new(0),
            judged_watched_claim: AtomicBool::new(false),
        }
    }
    async fn respond(&self, prompt: &str) -> Result<String> {
        if prompt.starts_with("Audit inventory fidelity.") {
            let data = context(prompt);
            if data["targetPassage"]["text"] == self.proposed_passage {
                self.judged.fetch_add(1, Ordering::Relaxed);
                return Ok(self.judge_reply.to_string());
            }
        }
        if prompt.starts_with("Audit claim coverage independently.") {
            let mut response: Value = serde_json::from_str(&fixture_response(prompt).unwrap())?;
            for unit in context(prompt)["units"].as_array().unwrap() {
                for passage in unit["passages"].as_array().unwrap() {
                    if passage["text"] == self.proposed_passage {
                        let id = passage["id"].as_str().unwrap();
                        response[id]["missingClaims"] =
                            json!(["The passage's premise is not in the inventory."]);
                    }
                }
            }
            return Ok(response.to_string());
        }
        if prompt.starts_with("Source passages:")
            && prompt.contains(&format!("\n\nClaim: {}", self.watched_claim))
        {
            self.judged_watched_claim.store(true, Ordering::Relaxed);
            if self.unsourced {
                return Ok("unsupported\nReason: No supplied passage mentions this behaviour.\nSource passage: none".into());
            }
        }
        self.inner.respond(prompt).await
    }
}

fn lesson(second_body: &str) -> Value {
    let mut lesson = candidate(GOOD);
    lesson["blocks"].as_array_mut().unwrap().push(json!({
        "kind":"explanation","title":"Header exercise","body":second_body,"rubric":[]
    }));
    lesson
}

/// The 2026-10-14 live failure: extraction missed a factual premise embedded
/// in an exercise, the mapper proposed the omission and the judge named it.
/// The named assertion becomes a claim, fails its evidence check and is
/// repaired, instead of stalling coverage and failing the whole job.
async fn adopted_assertion_is_checked_and_repaired(
    premise: &'static str,
    statement: &'static str,
    verdict: ClaimVerdict,
    removed_wording: &str,
) -> Result<()> {
    let model = JudgingModel::new(premise, &[statement], statement);
    let original = lesson(premise);
    let report = verify(&model, &original, &[source()]).await?;
    assert!(
        report.coverage_audit.iter().all(|unit| unit.complete),
        "the audit's own finding completes coverage"
    );
    assert!(report.coverage_audit[1].reason.contains("unit-1-passage"));
    let adopted: Vec<_> = report.coverage[1]
        .claims
        .iter()
        .filter(|claim| claim.statement == statement)
        .collect();
    assert_eq!(adopted.len(), 1);
    assert_eq!(
        adopted[0].quote, premise,
        "an adopted claim is located at the judged passage"
    );
    assert!(model.judged_watched_claim.load(Ordering::Relaxed));
    assert!(report
        .findings
        .iter()
        .any(|f| f.statement == statement && f.verdict == verdict));
    assert!(
        !report.issues.is_empty(),
        "an adopted claim without support blocks publication"
    );

    let publishing = JudgingModel::new(premise, &[statement], statement);
    let (raw, report) = verify_and_repair(
        &publishing,
        "{}",
        &json!({}),
        original.to_string(),
        &[source()],
        3000,
    )
    .await?;
    assert!(report.issues.is_empty());
    assert!(*publishing.inner.repaired.lock().unwrap());
    let repaired: Value = serde_json::from_str(&raw)?;
    assert_eq!(
        repaired["blocks"][0], original["blocks"][0],
        "the sound section is untouched"
    );
    assert_ne!(repaired["blocks"][1]["body"], original["blocks"][1]["body"]);
    assert!(!repaired.to_string().contains(removed_wording));
    assert!(report.coverage_audit.iter().all(|unit| unit.complete));
    assert!(report
        .findings
        .iter()
        .all(|f| f.verdict == ClaimVerdict::Supported && f.statement != statement));
    Ok(())
}

#[tokio::test]
async fn an_adopted_assertion_the_sources_never_mention_is_repaired_not_fatal() -> Result<()> {
    adopted_assertion_is_checked_and_repaired(
        UNSOURCED_PREMISE,
        UNSOURCED,
        ClaimVerdict::Unsupported,
        "logs every parsed row",
    )
    .await
}

#[tokio::test]
async fn an_adopted_assertion_the_sources_contradict_is_repaired() -> Result<()> {
    adopted_assertion_is_checked_and_repaired(
        PREMISE,
        BAD,
        ClaimVerdict::Contradicted,
        "strips field-name whitespace",
    )
    .await
}

/// The correct neighbour: a mapper proposing an omission for a deliverable
/// and rubric is answered by the judge with no missing assertion. Nothing is
/// adopted, nothing is repaired, and the lesson publishes unchanged.
#[tokio::test]
async fn a_rubric_without_a_missing_assertion_is_neither_adopted_nor_repaired() -> Result<()> {
    let model = JudgingModel::new(RUBRIC, &[], BAD);
    let original = lesson(RUBRIC);
    let (raw, report) = verify_and_repair(
        &model,
        "{}",
        &json!({}),
        original.to_string(),
        &[source()],
        3000,
    )
    .await?;
    assert_eq!(model.judged.load(Ordering::Relaxed), 1);
    assert!(report.issues.is_empty());
    assert!(!*model.inner.repaired.lock().unwrap());
    assert_eq!(serde_json::from_str::<Value>(&raw)?, original);
    assert!(report.coverage_audit.iter().all(|unit| unit.complete));
    assert!(!report.coverage_audit[1]
        .reason
        .contains("recorded as claims"));
    assert_eq!(
        report.coverage[1].claims.len(),
        1,
        "only the extractor's own claim remains; no proposal was adopted"
    );
    assert!(!model.judged_watched_claim.load(Ordering::Relaxed));
    Ok(())
}
