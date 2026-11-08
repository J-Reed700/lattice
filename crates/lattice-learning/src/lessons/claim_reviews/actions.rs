//! What the user may do to a claim, one rule for the review's cards and for
//! the service that carries the action out. A published lesson has no draft
//! and no pass to run, so it offers what the ledger and the published text can
//! carry: Confirm, Unconfirm, Add a source, Edit and Remove on a section's own
//! text, and Undo of an Accept.

use lattice_core::domain::learning::claim_review::{
    ClaimActionKind, ClaimEvent, ClaimLedgerEntry, UnitKind, UserAction,
};
use lattice_core::shared::error::AppError;

use super::published_restore::{ended_by_section_edit, undoable_in_published};

pub(super) const ONLY_SECTION: &str = "Only a section's text can be changed in a published lesson.";

const EDITED_THE_SECTION: &str =
    "This changed when you edited the section. Edit the section again to change it back.";

const PUBLISHED: &str = "This lesson is published. Prepare it again to do that.";

/// Whether the claim is in a section's body, the one text a published
/// lesson's claim can be edited or removed in.
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

/// Whether the claim's last user event is an Accept, the one thing a
/// published lesson's undo takes back from the ledger alone.
fn undoes_an_accept(entry: &ClaimLedgerEntry) -> bool {
    entry
        .undo_target()
        .and_then(|ordinal| {
            entry
                .events()
                .iter()
                .find(|record| record.ordinal == ordinal)
        })
        .is_some_and(|record| {
            matches!(
                record.event,
                ClaimEvent::UserAction {
                    action: UserAction::Confirm { .. },
                    ..
                }
            )
        })
}

fn offered_when_published(entry: &ClaimLedgerEntry, action: ClaimActionKind) -> bool {
    match action {
        ClaimActionKind::Confirm | ClaimActionKind::Unconfirm | ClaimActionKind::AttachEvidence => {
            true
        }
        ClaimActionKind::Edit
        | ClaimActionKind::Remove
        | ClaimActionKind::PutBack
        | ClaimActionKind::RestoreOriginal => in_section_body(entry),
        ClaimActionKind::Undo => undoes_an_accept(entry) || undoable_in_published(entry),
        ClaimActionKind::ResearchWithHint => false,
    }
}

/// Why `action` is not carried out on a published lesson's claim; `None`
/// when it is offered, or the ledger alone is to refuse it.
pub(super) fn refusal(entry: &ClaimLedgerEntry, action: ClaimActionKind) -> Option<AppError> {
    if action == ClaimActionKind::Undo && ended_by_section_edit(entry) {
        return Some(AppError::InvalidState(EDITED_THE_SECTION.into()));
    }
    if !entry.allows(action) || offered_when_published(entry, action) {
        return None;
    }
    Some(match action {
        ClaimActionKind::Edit
        | ClaimActionKind::Remove
        | ClaimActionKind::PutBack
        | ClaimActionKind::RestoreOriginal => AppError::InvalidInput(ONLY_SECTION.into()),
        _ => AppError::InvalidState(PUBLISHED.into()),
    })
}
