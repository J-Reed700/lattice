use super::*;

#[tokio::test]
async fn a_supported_answer_whose_quotation_is_blank_is_unusable() -> Result<()> {
    let text = "Quillet turns characters into tokens.";
    let pending = PendingClaim {
        ordinal: 0,
        key: String::new(),
        receipt_key: String::new(),
        unit: 0,
        claim: Claim {
            quote: text.into(),
            statement: text.into(),
        },
        checked: text.into(),
        evidence: vec![EvidencePassage {
            source_id: "quillet".into(),
            text: text.into(),
            start_byte: 0,
            end_byte: text.len(),
            retrieval_kind: "lexical".into(),
            score: 1.0,
        }],
        pass: Pass::Attribution,
    };
    let judgment = ClaimJudgment::Judged(JudgeOutcome {
        verdict: ClaimVerdict::Supported,
        reason: Some("Stated.".into()),
        quote: Some(" ".into()),
        confidence: None,
        uncertain: false,
    });

    let (_, _, finding, _) = finish(pending, judgment, None, Vec::new()).await?;

    assert_eq!(finding.verdict, ClaimVerdict::Unverified);
    assert_eq!(finding.supporting_quote, None);
    Ok(())
}
