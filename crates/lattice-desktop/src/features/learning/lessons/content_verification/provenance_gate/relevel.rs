//! Levelling a claim's supports again after an override changed (source
//! provenance §18.7): from the judgements already stored and the course's
//! overrides as they are now, with no model call. The gate then re-decides
//! with lattice-core's `decide` (`TrustAssessment::redecide`).

use super::context::Judgements;
use lattice_core::domain::learning::claim_review::TrustAssessment;
use lattice_core::domain::provenance::Support;
use lattice_core::shared::error::Result;
use sqlx::SqlitePool;

/// `trust`'s supports with each one's level for the claim's subject read
/// again: the stored standing and ownership (judged by the model that
/// profiled the claim), the copy relations and the overrides now in force.
/// Everything else about a support (its cluster, dates, passage kind) is
/// as the gate recorded it. Errs, changing nothing, when a judgement was
/// never stored, since only a model could level that support.
pub(in crate::features::learning) async fn relevel(
    pool: &SqlitePool,
    program_id: &str,
    trust: &TrustAssessment,
) -> Result<Vec<Support>> {
    let mut judgements = Judgements::cached(pool.clone(), program_id, &trust.profile.model).await?;
    let versions: Vec<String> = trust
        .supports
        .iter()
        .map(|support| support.source_version_id.clone())
        .collect();
    let facts = judgements.facts(None, &versions, &trust.profile).await?;
    Ok(trust
        .supports
        .iter()
        .map(|support| {
            let mut support = support.clone();
            if let Some(facts) = facts.get(&support.source_version_id) {
                support.trust = facts.trust.clone();
            }
            support
        })
        .collect())
}
