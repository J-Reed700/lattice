//! Whether a claim's support is sufficient to verify it: the one place the
//! claim ledger turns a checker's "supported" into `ResolvedByEvidence`.
//!
//! Today the checker's verdict suffices. Source provenance and trust
//! (docs/design/2026-10-18-source-provenance-and-trust.md, track P4) replaces
//! [`CheckerVerdict`] with a gate that weighs where the supporting passage
//! came from; an insufficient support becomes an unsupported claim with the
//! gate's reason, so it goes through research, repair and the user's queue
//! like any other. Nothing else in the loop decides sufficiency.
//! Design: docs/design/2026-10-16-lesson-claim-review.md §4.

use crate::features::learning::reference_collection::ReferencePassage;

/// A supported verdict, as the gate sees it.
pub(in crate::features::learning) struct Support<'a> {
    pub claim: &'a str,
    /// The source version holding the supporting passage.
    pub source_version_id: &'a str,
    pub supporting_quote: &'a str,
    /// Every passage the checker compared.
    pub passages: &'a [ReferencePassage],
}

pub(in crate::features::learning) enum Sufficiency {
    Sufficient,
    /// Not enough to verify the claim; the reason is the claim's reason.
    Insufficient {
        reason: String,
    },
}

pub(in crate::features::learning) trait SupportGate:
    Send + Sync
{
    fn judge(&self, support: &Support<'_>) -> Sufficiency;
}

/// The rule before provenance and trust: the strict checker's supported
/// verdict is sufficient when the quote it gives is in a passage it compared
/// from the source it names.
pub(in crate::features::learning) struct CheckerVerdict;

impl SupportGate for CheckerVerdict {
    fn judge(&self, support: &Support<'_>) -> Sufficiency {
        let quoted = support.supporting_quote.is_empty()
            || support.passages.iter().any(|passage| {
                passage.source_id == support.source_version_id
                    && passage.text.contains(support.supporting_quote)
            });
        if quoted {
            Sufficiency::Sufficient
        } else {
            Sufficiency::Insufficient {
                reason: format!(
                    "The checker supported \"{}\" with a quotation that is not in the passage it names.",
                    support.claim
                ),
            }
        }
    }
}
