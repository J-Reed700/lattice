#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::classes_tests::{checker, Reply, Scripted};
use super::*;
use crate::application::ports::llm_port::CompletionResponse;
use crate::shared::error::AppError;
use async_trait::async_trait;

fn evidence() -> ClaimEvidence {
    ClaimEvidence {
        text: "Rust has a borrow checker.".into(),
        quote: None,
    }
}

/// A model whose every call fails the same way.
struct Failing(fn() -> AppError);

#[async_trait]
impl LLMPort for Failing {
    async fn complete(&self, _request: &CompletionRequest) -> Result<CompletionResponse> {
        Err((self.0)())
    }
    fn model_name(&self) -> &str {
        "failing"
    }
    fn max_context_tokens(&self) -> usize {
        8_192
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
    fn enforces(
        &self,
        _schema: &crate::application::services::model_contract::shape::Schema,
    ) -> bool {
        false
    }
}

#[tokio::test]
async fn a_chat_judge_reply_that_cannot_be_read_gets_one_correction() {
    let llm = Scripted::new(vec![
        Reply::Text("I would rather not say.".into()),
        Reply::Text("supported\nReason: The passage says so.\nSource quote: none".into()),
    ]);
    let judgment = checker(&llm, CheckPolicy::Chat)
        .check("Rust has a borrow checker.", &evidence())
        .await;
    assert!(matches!(judgment, ClaimJudgment::Judged(_)), "{judgment:?}");
    assert_eq!(llm.sent().len(), 2);
}

#[tokio::test]
async fn a_misconfigured_judge_call_fails_the_check() {
    let llm = Failing(|| AppError::InvalidConfig("no model is set".into()));
    let judgment = ClaimChecker::new(
        &llm,
        SamplingOverride::deterministic(),
        512,
        CheckPolicy::Chat,
    )
    .check("Rust has a borrow checker.", &evidence())
    .await;
    assert!(matches!(judgment, ClaimJudgment::Failed(_)), "{judgment:?}");
}

#[tokio::test]
async fn an_unreadable_batch_reply_is_asked_again_for_each_claim_alone() {
    let claims = vec![
        LocatedClaim {
            claim: "The harbour bell rings twice at noon.".into(),
            passages: vec!["The bell rings twice at noon.".into()],
        },
        LocatedClaim {
            claim: "The kiln is fired at dawn.".into(),
            passages: vec!["The kiln hours are not listed.".into()],
        },
    ];
    let entry = |of: &str, source: &str, verdict: &str| {
        serde_json::json!({"entries": [{
            "of": of,
            "reason": "The passage decides this.",
            "source": source,
            "verdict": verdict,
        }]})
        .to_string()
    };
    let llm = Scripted::new(vec![
        Reply::Text("not json".into()),
        Reply::Text(entry("c1", "p1", "supported")),
        Reply::Text(entry("c2", "none", "unsupported")),
    ]);
    let answered = checker(&llm, CheckPolicy::Strict)
        .check_batch_without_deadline(&claims, &|_| Ok(()), &|_| Ok(()))
        .await
        .unwrap();
    assert!(
        answered
            .judgments
            .iter()
            .all(|judgment| matches!(judgment, ClaimJudgment::Judged(_))),
        "{:?}",
        answered.judgments
    );
    assert_eq!(llm.sent().len(), 3);
}

#[tokio::test]
async fn a_cancelled_judge_call_fails_the_check() {
    let llm = Failing(|| AppError::Cancelled("The check was cancelled.".into()));
    let judgment = ClaimChecker::new(
        &llm,
        SamplingOverride::deterministic(),
        512,
        CheckPolicy::Chat,
    )
    .check("Rust has a borrow checker.", &evidence())
    .await;
    assert!(matches!(judgment, ClaimJudgment::Failed(_)), "{judgment:?}");
}
