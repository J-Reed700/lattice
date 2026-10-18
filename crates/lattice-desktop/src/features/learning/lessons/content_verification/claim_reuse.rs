//! Locations explain where a fact appears; they are not factual judge inputs.
//! Recover prior exact-input receipts without equating paraphrased assertions.
use super::*;

pub(super) async fn load_prior_claims(retained: &ClaimChecks) -> Result<()> {
    let saved =
        crate::features::learning::lesson_drafts::checkpoints_with_prefix("inventory-section-v1:")
            .await?;
    let mut prior = retained
        .prior_claims
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    for value in saved {
        let Some(unit) = value
            .get("inventory")
            .cloned()
            .and_then(|value| serde_json::from_value::<UnitClaims>(value).ok())
        else {
            continue;
        };
        for claim in unit.claims {
            let aliases = prior.entry(claim.statement.clone()).or_default();
            if !aliases
                .iter()
                .any(|(index, old)| *index == unit.index && old.quote == claim.quote)
            {
                aliases.push((unit.index, claim));
            }
        }
    }
    Ok(())
}

pub(super) fn identities(
    retained: &ClaimChecks,
    unit: usize,
    claim: &Claim,
) -> Vec<(usize, Claim)> {
    let mut identities = vec![(unit, claim.clone())];
    if let Some(prior) = retained
        .prior_claims
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&claim.statement)
    {
        identities.extend(
            prior
                .iter()
                .filter(|(index, old)| *index == unit && old.quote != claim.quote)
                .cloned(),
        );
    }
    identities
}

pub(super) async fn legacy_receipt(
    llm: &dyn LLMPort,
    retained: &ClaimChecks,
    unit: usize,
    claim: &Claim,
    evidence: &[EvidencePassage],
) -> Result<Option<ClaimReceipt>> {
    for (index, old) in identities(retained, unit, claim) {
        if let Some(receipt) = crate::features::learning::lesson_drafts::checkpoint(&legacy_key(
            llm, index, &old, evidence,
        ))
        .await?
        .and_then(|value| serde_json::from_value::<ClaimReceipt>(value).ok())
        {
            return Ok(Some(receipt));
        }
    }
    Ok(None)
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
