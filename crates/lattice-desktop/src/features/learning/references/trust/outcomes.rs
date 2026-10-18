//! The outcomes calibration learns from (§13): written from the same human
//! verdicts the checker-accuracy examples are (claim-review §15), by the
//! same callers, and forgotten with them on Undo. Each verdict on a claim
//! whose state the provenance gate decided gives one outcome per supporting
//! cluster (lattice-core's `trust_outcomes` holds the table). Writing or
//! forgetting outcomes may move a bar; both say which kinds' bars moved, so
//! the caller can re-decide those claims.

use lattice_core::application::ports::SourceTrustStore;
use lattice_core::domain::learning::claim_review::{
    trust_outcomes, ClaimLedgerEntry, TrustVerdict,
};
use lattice_core::domain::provenance::{
    ClaimKind, LabelSource, OriginKey, OutcomeRecord, TRUST_POLICY_VERSION,
};
use lattice_core::shared::error::{AppError, Result};
use sqlx::SqlitePool;

use super::calibration::{bars_of, moved_kinds};
use super::SqliteSourceTrustStore;

/// Where a claim's verdict is recorded.
#[derive(Debug, Clone, Copy)]
pub struct VerdictScope<'a> {
    pub program_id: &'a str,
    pub lesson_id: &'a str,
}

/// Records the outcomes `verdict` on `entry` gives (`entry` as the claim
/// stood before the verdict) and returns the kinds whose bar the new
/// outcomes moved. A claim the gate did not decide gives none.
pub async fn record(
    pool: &SqlitePool,
    scope: VerdictScope<'_>,
    entry: &ClaimLedgerEntry,
    verdict: TrustVerdict,
) -> Result<Vec<ClaimKind>> {
    let outcomes = trust_outcomes(entry, verdict, chrono::Utc::now().timestamp());
    if outcomes.is_empty() {
        return Ok(Vec::new());
    }
    let store = SqliteSourceTrustStore::new(pool.clone());
    let before = store.outcomes().await?;
    let mut records = Vec::with_capacity(outcomes.len());
    for outcome in outcomes {
        let origin_key = store
            .provenance(&outcome.source_version_id)
            .await?
            .map(|record| record.origin.key)
            .unwrap_or_else(|| OriginKey(format!("version:{}", outcome.source_version_id)));
        records.push(OutcomeRecord {
            id: uuid::Uuid::new_v4().to_string(),
            program_id: scope.program_id.to_owned(),
            lesson_id: scope.lesson_id.to_owned(),
            claim_id: entry.claim_id().0.clone(),
            source_version_id: outcome.source_version_id,
            origin_key,
            outcome: outcome.outcome,
            label_source: outcome.label_source,
            policy: TRUST_POLICY_VERSION.to_owned(),
        });
    }
    for record in &records {
        store.record_outcome(record).await?;
    }
    let after: Vec<OutcomeRecord> = before.iter().cloned().chain(records).collect();
    Ok(moved_kinds(&bars_of(&before), &bars_of(&after)))
}

/// Forgets the outcomes an undone `verdict` on `claim_id` gave (claim-review
/// §5.1 Undo, as `CheckerExamples::forget` does) and returns the kinds whose
/// bar that moved.
pub async fn forget(
    pool: &SqlitePool,
    claim_id: &str,
    verdict: TrustVerdict,
) -> Result<Vec<ClaimKind>> {
    let store = SqliteSourceTrustStore::new(pool.clone());
    let before = store.outcomes().await?;
    let labels = labels_of(verdict);
    let forgotten = |record: &OutcomeRecord| {
        record.claim_id == claim_id && labels.contains(&record.label_source)
    };
    if !before.iter().any(forgotten) {
        return Ok(Vec::new());
    }
    for label in labels {
        sqlx::query("DELETE FROM source_trust_outcomes WHERE claim_id=? AND label_source=?")
            .bind(claim_id)
            .bind(label.as_str())
            .execute(pool)
            .await
            .map_err(|error| AppError::Database(error.to_string()))?;
    }
    let after: Vec<OutcomeRecord> = before
        .iter()
        .filter(|record| !forgotten(record))
        .cloned()
        .collect();
    Ok(moved_kinds(&bars_of(&before), &bars_of(&after)))
}

/// The labels a verdict's outcomes may carry (`trust_outcomes`).
fn labels_of(verdict: TrustVerdict) -> &'static [LabelSource] {
    match verdict {
        TrustVerdict::Confirm => &[LabelSource::UserConfirm],
        TrustVerdict::Remove => &[LabelSource::UserRemove, LabelSource::VerifierDisagree],
        TrustVerdict::Edit => &[LabelSource::UserEdit, LabelSource::VerifierDisagree],
        TrustVerdict::EvidenceAdded => &[LabelSource::EvidenceAdded],
        TrustVerdict::VerifierAgreed => &[LabelSource::VerifierAgree],
    }
}
