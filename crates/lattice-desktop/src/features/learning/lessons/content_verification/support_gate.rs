//! Whether a claim's support is sufficient to verify it: the one place the
//! claim ledger turns a checker's "supported" into `ResolvedByEvidence`.
//!
//! The gate is [`super::provenance_gate::ProvenanceGate`] (source provenance
//! and trust, docs/design/2026-10-18-source-provenance-and-trust.md §6.2,
//! §11): the quotation must be in the passage it names, and the overturn
//! rule must leave the support standing. An insufficient support becomes an
//! unsupported claim with the gate's reason, and a winning contrary passage
//! a contradicted one, so each goes through research, repair and the user's
//! queue like any other. Nothing else in the loop decides sufficiency.
//! Design: docs/design/2026-10-16-lesson-claim-review.md §4.

use super::provenance_gate::FindingTrust;
use crate::features::learning::reference_collection::ReferencePassage;

/// A supported verdict, as the gate sees it.
pub(in crate::features::learning) struct Support<'a> {
    pub claim: &'a str,
    /// The source version holding the supporting passage.
    pub source_version_id: &'a str,
    pub supporting_quote: &'a str,
    /// Every passage the checker compared.
    pub passages: &'a [ReferencePassage],
    /// What the provenance gate weighed for this finding; `None` when it was
    /// not weighed.
    pub trust: Option<&'a FindingTrust>,
}

pub(in crate::features::learning) enum Sufficiency {
    Sufficient,
    /// Not enough to verify the claim; the reason is the claim's reason.
    /// `kind` is the ledger's failure when the gate's bar decided it
    /// (`InsufficientStanding`); `None` is a plain unsupported claim.
    Insufficient {
        reason: String,
        kind: Option<lattice_core::domain::learning::claim_review::VerdictKind>,
    },
    /// Contrary evidence outranks the support (§11); the reason is the
    /// claim's reason.
    Contradicted {
        reason: String,
    },
}

pub(in crate::features::learning) trait SupportGate:
    Send + Sync
{
    fn judge(&self, support: &Support<'_>) -> Sufficiency;
}
