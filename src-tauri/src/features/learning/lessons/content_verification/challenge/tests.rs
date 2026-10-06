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
        let llm = model(raw);
        assert!(matches!(
            guard(&llm, "A claim.", &[], decision(ClaimVerdict::Supported)).await,
            ClaimJudgment::Unusable
        ));
    }
    let offline = ScriptedModel {
        outputs: Mutex::new(Default::default()),
        prompts: Mutex::new(vec![]),
    };
    assert!(matches!(
        guard(&offline, "A claim.", &[], decision(ClaimVerdict::Supported)).await,
        ClaimJudgment::Unusable
    ));
}
