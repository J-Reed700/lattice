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

#[tokio::test]
async fn questions_block_publication_without_claiming_a_proven_contradiction() {
    let llm = model(
        r#"{"queries":[{"query":"process outcome necessary conditions","rationale":"Does the source establish all necessary conditions for the claimed outcome?"}]}"#,
    );
    let result = guard(
        &llm,
        "One controlled input guarantees the overall outcome.",
        &["fixture evidence".into()],
        decision(ClaimVerdict::Supported),
    )
    .await;
    assert!(
        matches!(result, ClaimJudgment::Judged(outcome) if outcome.verdict == ClaimVerdict::Unsupported && outcome.quote.is_none() && outcome.reason.as_deref().unwrap().contains("not established contradictions"))
    );
}

#[tokio::test]
async fn no_questions_retains_the_original_evidence_decision() {
    let llm = model(r#"{"queries":[]}"#);
    let result = guard(
        &llm,
        "A scoped statement.",
        &["fixture evidence".into()],
        decision(ClaimVerdict::Supported),
    )
    .await;
    assert!(
        matches!(result, ClaimJudgment::Judged(outcome) if outcome.verdict == ClaimVerdict::Supported && outcome.quote.as_deref() == Some("fixture evidence"))
    );
}

#[tokio::test]
async fn challenge_cannot_approve_or_relabel_a_negative_evidence_check() {
    let llm = model(r#"{"queries":[]}"#);
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
        r#"{"queries":[],"approved":true}"#,
        r#"{"queries":[{"query":"","rationale":"Missing query"}]}"#,
        r#"{"queries":[{"query":"query one","rationale":""}]}"#,
        r#"{"queries":[{"query":"query one","rationale":"A concern"},{"query":"QUERY ONE","rationale":"Repeated concern"}]}"#,
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

#[test]
fn the_challenge_may_raise_as_many_distinct_questions_as_its_concerns_need() {
    let questions: Questions = serde_json::from_value(serde_json::json!({"queries":(0..5)
        .map(|i| serde_json::json!({"query":format!("distinct concern search {i}"),"rationale":"A specific omitted condition."}))
        .collect::<Vec<_>>()}))
    .unwrap();
    assert!(
        validate_questions(&questions).is_ok(),
        "not the former three"
    );
}

/// A small window that answers every challenge with no questions and records
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
                r#"{"queries":[]}"#,
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
