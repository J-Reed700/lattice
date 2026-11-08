//! What the user may do to a claim, one rule for the review's cards and for
//! the service that carries the action out. A published lesson has no draft
//! and no pass to run, so it offers what the ledger and the published text can
//! carry: Confirm, Unconfirm, Add a source, and Edit on a section's own text.

use lattice_core::domain::learning::claim_review::{ClaimActionKind, ClaimLedgerEntry, UnitKind};
use lattice_core::shared::error::AppError;

use super::published_restore::undoable_in_published;

pub(super) const ONLY_SECTION: &str = "Only a section's text can be edited in a published lesson.";

const PUBLISHED: &str = "This lesson is published. Prepare it again to do that.";

/// Whether the claim is in a section's body, the one text a published
/// lesson's claim can be edited in.
pub(super) fn in_section_body(entry: &ClaimLedgerEntry) -> bool {
    let location = entry.location();
    location.unit_kind == UnitKind::Block && location.field == "body"
}

/// The actions the claim offers now.
pub(super) fn offered(entry: &ClaimLedgerEntry, published: bool) -> Vec<ClaimActionKind> {
    entry
        .available_actions()
        .into_iter()
        .filter(|action| !published || offered_when_published(entry, *action))
        .collect()
}

fn offered_when_published(entry: &ClaimLedgerEntry, action: ClaimActionKind) -> bool {
    match action {
        ClaimActionKind::Confirm | ClaimActionKind::Unconfirm | ClaimActionKind::AttachEvidence => {
            true
        }
        ClaimActionKind::Edit | ClaimActionKind::PutBack | ClaimActionKind::RestoreOriginal => {
            in_section_body(entry)
        }
        ClaimActionKind::Undo => undoable_in_published(entry),
        ClaimActionKind::Remove | ClaimActionKind::ResearchWithHint => false,
    }
}

/// Why `action` is not carried out on a published lesson's claim; `None`
/// when it is offered, or the ledger alone is to refuse it.
pub(super) fn refusal(entry: &ClaimLedgerEntry, action: ClaimActionKind) -> Option<AppError> {
    if !entry.allows(action) || offered_when_published(entry, action) {
        return None;
    }
    Some(match action {
        ClaimActionKind::Edit | ClaimActionKind::PutBack | ClaimActionKind::RestoreOriginal => {
            AppError::InvalidInput(ONLY_SECTION.into())
        }
        _ => AppError::InvalidState(PUBLISHED.into()),
    })
}
