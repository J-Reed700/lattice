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
async fn an_unreadable_batch_reply_is_corrected_once() {
    let claims = vec![
        LocatedClaim {
            claim: "A is 12.".into(),
            passages: vec!["A is 12.".into()],
        },
        LocatedClaim {
            claim: "B is 20.".into(),
            passages: vec!["B is measured.".into()],
        },
    ];
    let supported =
        "Reason: The passage states this.\nSource passage: passage-0\nVerdict: supported";
    let unsupported =
        "Reason: The passage gives no figure.\nSource passage: none\nVerdict: unsupported";
    let valid = serde_json::json!({"checks": [
        {"id": "claim-0", "response": supported},
        {"id": "claim-1", "response": unsupported},
    ]})
    .to_string();
    let llm = Scripted::new(vec![Reply::Text("not json".into()), Reply::Text(valid)]);
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
    assert_eq!(llm.sent().len(), 2);
}
