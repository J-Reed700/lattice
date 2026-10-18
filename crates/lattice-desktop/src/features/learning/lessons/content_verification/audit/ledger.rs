//! The audit's side of the claim ledger: which claims are due (no current
//! receipt), what a Verifier check records, and appending it under the
//! review's revision (§9), so a user acting meanwhile is never overwritten.

use std::collections::HashMap;

use lattice_core::domain::learning::claim_review::{
    text_sha256, Actor, Check, Checker, ClaimEvent, ClaimId, ClaimState, EventProvenance,
    EvidenceFingerprint, EvidenceSeen, Ledger, ReopenCause, Revision, VerdictKind,
};
use lattice_core::shared::error::{AppError, Result};

use super::policy::{Judged, AUDIT_POLICY_ID};
use super::{AuditClaim, AuditDeps, AuditPassage, AuditRequest};

/// The passages a claim's audit compared, pinned to their source versions.
pub(super) fn evidence(passages: &[AuditPassage]) -> Vec<EvidenceSeen> {
    passages
        .iter()
        .map(|passage| EvidenceSeen {
            source_version_id: passage.source_version_id.clone(),
            start_byte: passage.start_byte,
            end_byte: passage.end_byte,
            text_sha256: text_sha256(&passage.text),
        })
        .collect()
}

/// A claim's audit receipt: the policy, the claim's wording, the Verifier
/// model and the passages. A Verifier check with this fingerprint means the
/// claim needs no audit until one of them changes.
pub(super) fn receipt(claim: &AuditClaim, model: &str) -> EvidenceFingerprint {
    let wording = text_sha256(&format!("{}\n{}", claim.statement, claim.quote));
    EvidenceFingerprint::of(
        &format!("{AUDIT_POLICY_ID}:{wording}"),
        model,
        &evidence(&claim.passages),
    )
}

/// The claims to send, in lesson order: verified, confirmed or accepted as
/// general knowledge, without a current receipt. Claims waiting for the
/// user already block publishing and are not sent; removed or edited ones
/// are gone.
pub(super) fn due(
    ledger: &Ledger,
    passages: &HashMap<ClaimId, Vec<AuditPassage>>,
    model: &str,
) -> Vec<AuditClaim> {
    let mut claims: Vec<(Option<usize>, AuditClaim)> = ledger
        .claims
        .iter()
        .filter_map(|entry| {
            let general_knowledge = match entry.state() {
                ClaimState::ResolvedByEvidence { .. } | ClaimState::Confirmed { .. } => false,
                ClaimState::GeneralKnowledge { .. } => true,
                _ => return None,
            };
            let claim = AuditClaim {
                claim_id: entry.claim_id().clone(),
                unit_index: entry.location().unit_index,
                statement: entry.statement().to_owned(),
                quote: entry.location().quote.clone(),
                passages: passages.get(entry.claim_id()).cloned().unwrap_or_default(),
                general_knowledge,
            };
            let latest = entry
                .events()
                .iter()
                .rev()
                .find_map(|recorded| match &recorded.event {
                    ClaimEvent::Checked(check)
                        if matches!(check.checker, Checker::Verifier { .. }) =>
                    {
                        Some(&check.fingerprint)
                    }
                    _ => None,
                });
            (latest != Some(&receipt(&claim, model)))
                .then(|| (entry.location().span.map(|(start, _)| start), claim))
        })
        .collect();
    claims.sort_by_key(|(start, claim)| (claim.unit_index, *start));
    claims.into_iter().map(|(_, claim)| claim).collect()
}

/// Claims the Verifier disputes, waiting for the user.
pub(super) fn disputes(ledger: &Ledger) -> usize {
    ledger
        .claims
        .iter()
        .filter(|entry| {
            matches!(
                entry.state(),
                ClaimState::Queued {
                    kind: VerdictKind::Disputed { .. },
                    ..
                } | ClaimState::Reopened {
                    cause: ReopenCause::VerifierDispute,
                    ..
                }
            )
        })
        .count()
}

/// Appends Verifier checks, keeping the review's revision.
pub(super) struct LedgerWriter<'a> {
    deps: &'a AuditDeps<'a>,
    request: &'a AuditRequest<'a>,
    model: &'a str,
    window_tokens: usize,
    revision: Revision,
}

impl<'a> LedgerWriter<'a> {
    pub(super) fn new(
        deps: &'a AuditDeps<'a>,
        request: &'a AuditRequest<'a>,
        model: &'a str,
        window_tokens: usize,
        loaded: &Ledger,
    ) -> Self {
        Self {
            deps,
            request,
            model,
            window_tokens,
            revision: loaded.revision,
        }
    }

    /// Records `judged` as the claim's Verifier check. When the review moved
    /// on meanwhile (the user acted), the ledger is read again and the check
    /// is appended only if the claim is still due as it was judged.
    pub(super) async fn append(&mut self, claim: &AuditClaim, judged: Judged) -> Result<()> {
        let check = Check {
            checker: Checker::Verifier {
                model: self.model.to_owned(),
            },
            result: judged.result,
            reason: judged.reason,
            fingerprint: receipt(claim, self.model),
            evidence: evidence(&claim.passages),
            window_tokens: self.window_tokens,
        };
        match self.try_append(claim, &check).await {
            Err(AppError::InvalidState(_)) => {
                let ledger = self.deps.store.load(self.request.lesson_id).await?;
                self.revision = ledger.revision;
                let still_due = due(&ledger, self.request.passages, self.model)
                    .iter()
                    .any(|due| due.claim_id == claim.claim_id && due == claim);
                if still_due {
                    self.try_append(claim, &check).await?;
                }
                Ok(())
            }
            other => other,
        }
    }

    async fn try_append(&mut self, claim: &AuditClaim, check: &Check) -> Result<()> {
        let provenance = EventProvenance {
            actor: Actor::Machine,
            at: (self.deps.clock)(),
            job_id: self.request.job_id.map(str::to_owned),
            app_version: self.request.app_version.to_owned(),
            checker_model: Some(self.model.to_owned()),
        };
        self.revision = self
            .deps
            .store
            .append(
                self.request.lesson_id,
                &claim.claim_id,
                provenance,
                ClaimEvent::Checked(check.clone()),
                self.revision,
            )
            .await?;
        Ok(())
    }

    /// The disputes now in the ledger.
    pub(super) async fn disputes(&self) -> Result<usize> {
        Ok(disputes(
            &self.deps.store.load(self.request.lesson_id).await?,
        ))
    }
}
