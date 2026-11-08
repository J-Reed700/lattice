//! Locations explain where a fact appears; they are not factual judge inputs.
//! A receipt saved under the earlier key is found by the claim's own unit,
//! quote, statement and passages, never by an equal statement on another quote.
use super::*;

pub(super) async fn legacy_receipt(
    llm: &dyn LLMPort,
    unit: usize,
    claim: &Claim,
    evidence: &[EvidencePassage],
) -> Result<Option<ClaimReceipt>> {
    let key = legacy_key(llm, unit, claim, evidence);
    Ok(crate::lesson_drafts::checkpoint(&key)
        .await?
        .and_then(|value| serde_json::from_value::<ClaimReceipt>(value).ok()))
}

pub(super) fn legacy_key(
    llm: &dyn LLMPort,
    unit: usize,
    claim: &Claim,
    evidence: &[EvidencePassage],
) -> String {
    let passages: Vec<_> = evidence
        .iter()
        .map(|p| (&p.source_id, &p.text, p.start_byte, p.end_byte))
        .collect();
    claim_receipt_key(
        llm,
        &digest(&json!({"unit":unit,"claim":claim,"passages":passages}).to_string()),
    )
}
