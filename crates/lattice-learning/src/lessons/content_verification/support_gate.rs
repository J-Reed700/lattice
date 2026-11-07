//! Whether a claim's support is sufficient to verify it: the one place the
//! claim ledger turns a checker's "supported" into `ResolvedByEvidence`, via
//! [`super::provenance_gate::ProvenanceGate`]. The quotation the checker cites
//! must be in the passage it names; what the provenance gate weighed of the
//! source labels the claim and never takes the verdict away (claim classes
//! design §3.2).
//! Design: docs/design/2026-10-16-lesson-claim-review.md §4.

use crate::reference_collection::ReferencePassage;

/// A supported verdict, as the gate sees it.
pub(crate) struct Support<'a> {
    pub claim: &'a str,
    /// The source version holding the supporting passage.
    pub source_version_id: &'a str,
    pub supporting_quote: &'a str,
    /// Every passage the checker compared.
    pub passages: &'a [ReferencePassage],
}

pub(crate) enum Sufficiency {
    Sufficient,
    /// The quotation is not in the passage it names: a plain unsupported
    /// claim; the reason is the claim's reason.
    Insufficient {
        reason: String,
    },
}

pub(crate) trait SupportGate:
    Send + Sync
{
    fn judge(&self, support: &Support<'_>) -> Sufficiency;
}
