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

/// The page a statement's own citation gives for the claim standing on
/// `sentence`, its quote, of section `unit`: its section's passed example runs
/// and the passages the claim's sentence cites (the section's when the writer
/// cited none), while a check of `checked` against them `fits` the judge that
/// reads them, in canonical order. Also the source passages among them.
pub(in crate::lessons::content_verification) fn attribution_page(
    executions: &[execution::Observation],
    citations: &Citations,
    (unit, sentence): (usize, &str),
    checked: &str,
    fits: impl Fn(&str, &[EvidencePassage]) -> bool,
) -> (Vec<EvidencePassage>, Vec<EvidencePassage>) {
    let mut page = example_runs(executions, unit);
    let cited = take_cited(
        fits,
        checked,
        &mut page,
        &citations.of_claim(unit, sentence),
    );
    canonicalize_evidence(&mut page);
    (page, cited)
}

/// Why a comparison with no passage to read fails.
pub(in crate::lessons::content_verification) const NO_PASSAGES: &str =
    "No relevant saved passages were found for this statement.";

/// One comparison: derived on the spot when it has nothing to compare, restored
/// from its receipt, or left for the model.
pub(in crate::lessons::content_verification) async fn compare(
    pending: PendingClaim,
) -> Result<Comparison> {
    let (unit, claim, evidence) = (pending.unit, &pending.claim, &pending.evidence);
    if evidence.is_empty() {
        return Ok(Comparison::Done(Box::new(no_passages(unit, claim)), false));
    }
    if let Some(value) = crate::lesson_drafts::checkpoint(&pending.receipt_key).await? {
        let restored = serde_json::from_value::<ClaimReceipt>(value)
            .ok()
            .and_then(|receipt| receipt.restored(unit, claim, evidence));
        if let Some(finding) = restored {
            return Ok(Comparison::Done(Box::new(finding), true));
        }
    }
    Ok(Comparison::Pending(pending))
}

/// The finding of a statement with no passage to compare: no judge is asked
/// and no receipt is saved.
fn no_passages(unit: usize, claim: &Claim) -> Finding {
    Finding {
        unit,
        quote: claim.quote.clone(),
        statement: claim.statement.clone(),
        verdict: ClaimVerdict::Unsupported,
        reason: NO_PASSAGES.into(),
        evidence: Vec::new(),
        supporting_quote: None,
        resolution: None,
        trust: None,
        claim_id: None,
        two_pass: TwoPass::default(),
        label: None,
        class: None,
    }
}
