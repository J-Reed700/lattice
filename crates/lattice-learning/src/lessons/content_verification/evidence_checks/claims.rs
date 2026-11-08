//! One comparison of a claim with its evidence: restored from its receipt,
//! decided on the spot when it has nothing to compare, or left for the model;
//! and the page a statement's own citation gives (course evidence index §5.1).
use super::*;

/// One of a claim's comparisons.
pub(in crate::lessons::content_verification) enum Comparison {
    /// Its finding, and whether a saved check gave it.
    Done(Box<Finding>, bool),
    Pending(PendingClaim),
}

/// The page a statement's own citation gives for the claim `statement` of
/// section `unit`: its section's passed example runs and the passages the
/// claim's sentence cites (the section's when the writer cited none), while a
/// check of `checked` against them `fits` the judge that reads them, in
/// canonical order. Also the source passages among them.
pub(in crate::lessons::content_verification) fn attribution_page(
    executions: &[execution::Observation],
    citations: &Citations,
    (unit, statement): (usize, &str),
    checked: &str,
    fits: impl Fn(&str, &[EvidencePassage]) -> bool,
) -> (Vec<EvidencePassage>, Vec<EvidencePassage>) {
    let mut page = example_runs(executions, unit);
    let cited = take_cited(
        fits,
        checked,
        &mut page,
        &citations.of_claim(unit, statement),
    );
    canonicalize_evidence(&mut page);
    (page, cited)
}

/// Why a comparison with no passage to read fails.
pub(in crate::lessons::content_verification) const NO_PASSAGES: &str =
    "No relevant saved passages were found for this claim.";

/// One comparison: restored from its receipt (or a receipt saved under an
/// earlier key for the same claim and evidence), decided on the spot when it
/// has nothing to compare, or left for the model.
pub(in crate::lessons::content_verification) async fn compare(
    llm: &dyn LLMPort,
    pending: PendingClaim,
) -> Result<Comparison> {
    let (unit, claim, evidence) = (pending.unit, &pending.claim, &pending.evidence);
    if let Some(value) = crate::lesson_drafts::checkpoint(&pending.receipt_key).await? {
        let parts = bridge::saved_parts(&value);
        if let Some(mut finding) = serde_json::from_value::<ClaimReceipt>(value)
            .ok()
            .and_then(|receipt| receipt.finding(unit, claim, evidence))
        {
            finding.two_pass.parts = parts;
            return Ok(Comparison::Done(Box::new(finding), true));
        }
    }
    if let Some(receipt) = claim_reuse::legacy_receipt(llm, unit, claim, evidence).await? {
        let saved = serde_json::to_value(&receipt)?;
        if let Some(finding) = receipt.finding(unit, claim, evidence) {
            crate::lesson_drafts::record_checkpoint(&pending.receipt_key, saved).await?;
            return Ok(Comparison::Done(Box::new(finding), true));
        }
    }
    if evidence.is_empty() {
        let judgment = ClaimJudgment::Judged(JudgeOutcome {
            verdict: ClaimVerdict::Unsupported,
            reason: Some(NO_PASSAGES.into()),
            quote: None,
            confidence: None,
            uncertain: false,
        });
        let (_, _, finding, _) = finish(pending, judgment, None, Vec::new()).await?;
        return Ok(Comparison::Done(Box::new(finding), false));
    }
    Ok(Comparison::Pending(pending))
}
