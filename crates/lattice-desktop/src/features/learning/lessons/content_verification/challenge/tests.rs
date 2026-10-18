#![allow(clippy::unwrap_used)]
use super::*;
use crate::features::learning::lessons::course_generation_tests::ScriptedModel;
use std::sync::Mutex;

fn model(reply: &str) -> ScriptedModel {
    ScriptedModel {
        outputs: Mutex::new([reply.to_owned()].into()),
        prompts: Mutex::new(vec![]),
    }
}
fn decision(verdict: ClaimVerdict) -> ClaimJudgment {
    ClaimJudgment::Judged(JudgeOutcome {
        verdict,
        reason: Some("The fixture source states this.".into()),
        quote: Some("fixture evidence".into()),
        confidence: None,
    })
}

const PASSAGE: &str = "List all enum values explicitly: it prevents the model from inventing invalid values. Strict mode applies only to nested objects.";

fn objection(ground: &str, quote: &str) -> String {
    json!({"objections":[{"ground":ground,"quote":quote,"rationale":"The fixture's reason."}]})
        .to_string()
}

async fn reviewed(reply: &str) -> ClaimJudgment {
    guard(
        &model(reply),
        "Listing all enum values prevents invalid values.",
        &[PASSAGE.into()],
        decision(ClaimVerdict::Supported),
    )
    .await
}

#[tokio::test]
async fn a_source_that_agrees_with_the_claim_never_overturns_it() {
    for reply in [
        json!({"objections":[]}).to_string(),
        // Doubt the passages do not show: no quote of theirs.
        objection(
            "contradiction",
            "Enums cannot guarantee valid calls in every API.",
        ),
    ] {
        assert!(
            matches!(reviewed(&reply).await, ClaimJudgment::Judged(outcome) if outcome.verdict == ClaimVerdict::Supported && outcome.quote.as_deref() == Some("fixture evidence")),
            "{reply}"
        );
    }
}

#[tokio::test]
async fn a_quoted_contradicting_passage_overturns_the_approval() {
    let ClaimJudgment::Judged(outcome) = reviewed(&objection(
        "contradiction",
        "strict mode  applies only to nested objects",
    ))
    .await
    else {
        panic!("a judged outcome");
    };
    assert_eq!(outcome.verdict, ClaimVerdict::Contradicted);
    assert!(outcome
        .reason
        .unwrap()
        .contains("contradicts the claim: \"strict mode  applies only to nested objects\""));
}

#[tokio::test]
async fn a_quoted_unreliable_source_leaves_the_claim_unsupported() {
    let ClaimJudgment::Judged(outcome) = reviewed(&objection(
        "unreliable_source",
        "List all enum values explicitly",
    ))
    .await
    else {
        panic!("a judged outcome");
    };
    assert_eq!(outcome.verdict, ClaimVerdict::Unsupported);
    assert!(outcome.reason.unwrap().contains("not reliable"));
}

#[tokio::test]
async fn challenge_cannot_approve_or_relabel_a_negative_evidence_check() {
    let llm = model(r#"{"objections":[]}"#);
    for verdict in [
        ClaimVerdict::Unsupported,
        ClaimVerdict::Contradicted,
        ClaimVerdict::Unverified,
    ] {
        assert!(
            matches!(guard(&llm, "A claim.", &[], decision(verdict)).await, ClaimJudgment::Judged(outcome) if outcome.verdict == verdict)
        );
    }
    assert!(llm.prompts.lock().unwrap().is_empty());
}

#[tokio::test]
async fn malformed_or_failed_challenge_never_inherits_provisional_approval() {
    for raw in [
        "not json",
        r#"{"objections":[],"approved":true}"#,
        r#"{"objections":[{"ground":"contradiction","quote":" ","rationale":"A reason"}]}"#,
        r#"{"objections":[{"ground":"contradiction","quote":"fixture evidence","rationale":""}]}"#,
        r#"{"objections":[{"ground":"doubt","quote":"fixture evidence","rationale":"A reason"}]}"#,
    ] {
        // Left unchecked, with the failure as its cause; never approved.
        let llm = model(raw);
        assert!(matches!(
            guard(&llm, "A claim.", &[], decision(ClaimVerdict::Supported)).await,
            ClaimJudgment::Judged(outcome) if outcome.verdict == ClaimVerdict::Unverified
                && outcome.reason.as_deref().is_some_and(|reason| reason.starts_with("The evidence challenge could not run"))
        ));
    }
    let offline = ScriptedModel {
        outputs: Mutex::new(Default::default()),
        prompts: Mutex::new(vec![]),
    };
    assert!(matches!(
        guard(&offline, "A claim.", &[], decision(ClaimVerdict::Supported)).await,
        ClaimJudgment::Judged(outcome) if outcome.verdict == ClaimVerdict::Unverified
    ));
}

/// A small window that answers every challenge with no objections and records
/// what each challenge request carried.
struct SmallWindow {
    window: usize,
    prompts: Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl LLMPort for SmallWindow {
    async fn complete(
        &self,
        request: &lattice_core::application::ports::llm_port::CompletionRequest,
    ) -> Result<lattice_core::application::ports::llm_port::CompletionResponse> {
        let (_, user) = crate::features::learning::lessons::structured_call::task_messages(request);
        self.prompts.lock().unwrap().push(user.to_owned());
        Ok(
            lattice_core::application::ports::llm_port::CompletionResponse::from_text(
                r#"{"objections":[]}"#,
            ),
        )
    }
    fn model_name(&self) -> &str {
        "small-window"
    }
    fn max_context_tokens(&self) -> usize {
        self.window
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

fn supported_by(quote: &str) -> ClaimJudgment {
    ClaimJudgment::Judged(JudgeOutcome {
        verdict: ClaimVerdict::Supported,
        reason: Some("The passage states this.".into()),
        quote: Some(quote.into()),
        confidence: None,
    })
}

#[tokio::test]
async fn a_full_judge_page_is_cut_to_the_challenges_own_room_with_its_support_first() {
    let passages: Vec<String> = (0..20)
        .map(|i| format!("Passage {i}: {}", "reference text ".repeat(120)))
        .collect();
    let llm = SmallWindow {
        window: 8_000,
        prompts: Mutex::new(vec![]),
    };
    let result = guard(&llm, "A claim.", &passages, supported_by(&passages[12])).await;
    assert!(
        matches!(&result, ClaimJudgment::Judged(outcome) if outcome.verdict == ClaimVerdict::Supported)
    );
    let prompt = &llm.prompts.lock().unwrap()[0];
    assert!(prompt.starts_with(&format!("Source passages:\n[passage-0]\n{}", passages[12])));
    assert!(prompt.matches("[passage-").count() < passages.len());
}

#[tokio::test]
async fn only_a_claim_and_support_too_large_together_leave_the_claim_unchecked() {
    let huge = format!("Passage: {}", "reference text ".repeat(4_000));
    let llm = SmallWindow {
        window: 8_000,
        prompts: Mutex::new(vec![]),
    };
    let result = guard(
        &llm,
        "A claim.",
        std::slice::from_ref(&huge),
        supported_by(&huge),
    )
    .await;
    let ClaimJudgment::Judged(outcome) = result else {
        panic!("an unchecked judgment carries its cause");
    };
    assert_eq!(outcome.verdict, ClaimVerdict::Unverified);
    assert!(outcome
        .reason
        .unwrap()
        .contains("do not fit the evidence challenge"));
    assert!(llm.prompts.lock().unwrap().is_empty(), "nothing was sent");
}
