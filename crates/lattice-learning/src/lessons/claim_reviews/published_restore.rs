//! Planning and recording put-back / restore-original in a published lesson,
//! plus undoability checks backed by `ContentEdits::newest`.

use std::collections::HashSet;

use lattice_core::domain::learning::claim_review::{
    span_of, Actor, ClaimEvent, ClaimId, ClaimLedgerEntry, EventProvenance, UserAction,
};
use lattice_core::shared::edits::{EditOrigin, NewestEdit};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::WriteTx;

use super::actions::{in_section_body, ONLY_SECTION};
use super::edit_reactor::ClaimsReceipt;
use super::published_edit::{placed, update_report, Recorded, CHANGED};
use super::put_back::siblings;
use super::repository::append_in;
use crate::content_verification::{insert_sentence, WrittenBack};

const PUT_BACK_REASON: &str = "You put this sentence back; it was not checked against the sources.";
const RESTORED_REASON: &str = "You restored the original sentence; it was not checked again.";

/// Planned put-back or restore-original change on a section's body.
pub(super) struct RestorePlan {
    pub unit: usize,
    pub old_body: String,
    pub range: (usize, usize),
    pub new_body: String,
    pub replaced: Vec<ClaimId>,
    pub siblings: Vec<ClaimId>,
    pub sentence: String,
    pub statement: String,
    pub reason: &'static str,
    pub event: ClaimEvent,
    pub claim_id: ClaimId,
}

/// The machine's record written right before the user event at `ordinal`.
fn recorded_with(entry: &ClaimLedgerEntry, ordinal: u32) -> Option<&ClaimEvent> {
    entry
        .events()
        .iter()
        .rev()
        .find(|record| record.ordinal < ordinal)
        .map(|record| &record.event)
}

/// Whether the user's edit of the claim's whole section ended it: that is
/// taken back by editing the section again, not by an undo of the claim.
pub(super) fn ended_by_section_edit(entry: &ClaimLedgerEntry) -> bool {
    last_user_event(entry).is_some_and(|(ordinal, _)| {
        matches!(
            recorded_with(entry, ordinal),
            Some(ClaimEvent::SectionEdited {})
        )
    })
}

pub(super) fn last_user_event(entry: &ClaimLedgerEntry) -> Option<(u32, &ClaimEvent)> {
    let ordinal = entry.undo_target()?;
    entry
        .events()
        .iter()
        .find(|record| record.ordinal == ordinal)
        .map(|record| (ordinal, &record.event))
}

/// The section index affected by `entry`'s latest action (either the chosen
/// `PutBack` section or `entry.location().unit_index`).
pub(super) fn action_unit(entry: &ClaimLedgerEntry) -> usize {
    last_user_event(entry)
        .and_then(|(_, event)| match event {
            ClaimEvent::UserAction {
                action: UserAction::PutBack { place, .. },
                ..
            } => Some(place.unit_index),
            _ => None,
        })
        .unwrap_or_else(|| entry.location().unit_index)
}

/// Parses `receipt` on `newest` into `ClaimsReceipt`.
pub(super) fn claims_receipt(newest: &NewestEdit) -> Option<ClaimsReceipt> {
    serde_json::from_value(newest.receipt.as_ref()?.clone()).ok()
}

/// Whether `newest` is an unbroken `Action` edit whose receipt acted on
/// `(claim_id, ordinal)`.
pub(super) fn receipt_holds(newest: &NewestEdit, claim_id: &str, ordinal: u32) -> bool {
    newest.unbroken
        && matches!(newest.origin, EditOrigin::Action { .. })
        && claims_receipt(newest).is_some_and(|receipt| {
            !receipt.revert
                && receipt
                    .acted
                    .iter()
                    .any(|(id, ord)| id == claim_id && *ord == ordinal)
        })
}

/// Whether the claim's undo target is one a published lesson undoes: a text
/// action whose unit's `newest` unbroken `Action` edit matches it, or an
/// attached source that came after a put-back.
pub(super) fn undoable_in_published(entry: &ClaimLedgerEntry, newest: Option<&NewestEdit>) -> bool {
    let Some((ordinal, event)) = last_user_event(entry) else {
        return false;
    };
    if ended_by_section_edit(entry) {
        return false;
    }
    match event {
        ClaimEvent::UserAction {
            action:
                UserAction::Edit { .. }
                | UserAction::Remove
                | UserAction::PutBack { .. }
                | UserAction::RestoreOriginal { .. },
            ..
        } => newest.is_some_and(|edit| receipt_holds(edit, &entry.claim_id().0, ordinal)),
        ClaimEvent::EvidenceAttached { .. } => entry
            .events()
            .iter()
            .rev()
            .skip_while(|record| record.ordinal >= ordinal)
            .find(|record| record.provenance.actor == Actor::User)
            .is_some_and(|record| {
                matches!(
                    record.event,
                    ClaimEvent::UserAction {
                        action: UserAction::PutBack { .. },
                        ..
                    }
                )
            }),
        _ => false,
    }
}

/// Plans a `PutBack` or `RestoreOriginal` on `body`.
pub(super) fn plan(
    claims: &[ClaimLedgerEntry],
    target: &ClaimLedgerEntry,
    event: &ClaimEvent,
    body: &str,
) -> Result<RestorePlan> {
    match event {
        ClaimEvent::UserAction {
            action: UserAction::PutBack {
                sentence, place, ..
            },
            ..
        } => {
            let new_body = insert_sentence(body, &place.anchor, sentence)
                .ok_or_else(|| AppError::InvalidInput(CHANGED.into()))?;
            let sibling_ids = siblings(claims, target)
                .into_iter()
                .map(|entry| entry.claim_id().clone())
                .collect();
            Ok(RestorePlan {
                unit: place.unit_index,
                old_body: body.to_owned(),
                range: (0, 0),
                new_body,
                replaced: Vec::new(),
                siblings: sibling_ids,
                sentence: sentence.clone(),
                statement: target.statement().to_owned(),
                reason: PUT_BACK_REASON,
                event: event.clone(),
                claim_id: target.claim_id().clone(),
            })
        }
        ClaimEvent::UserAction {
            action: UserAction::RestoreOriginal { original, rewrite },
            ..
        } => {
            if !in_section_body(target) {
                return Err(AppError::InvalidInput(ONLY_SECTION.into()));
            }
            let unit = target.location().unit_index;
            let range =
                span_of(body, rewrite).ok_or_else(|| AppError::InvalidInput(CHANGED.into()))?;
            let mut new_body = body.to_owned();
            new_body.replace_range(range.0..range.1, original);
            let replaced = claims
                .iter()
                .filter(|entry| {
                    entry.claim_id() != target.claim_id()
                        && in_section_body(entry)
                        && entry.location().unit_index == unit
                        && !entry.state().is_gone()
                        && placed(entry, body)
                            .is_some_and(|(start, end)| start < range.1 && range.0 < end)
                })
                .map(|entry| entry.claim_id().clone())
                .collect();
            Ok(RestorePlan {
                unit,
                old_body: body.to_owned(),
                range,
                new_body,
                replaced,
                siblings: Vec::new(),
                sentence: original.clone(),
                statement: target.statement().to_owned(),
                reason: RESTORED_REASON,
                event: event.clone(),
                claim_id: target.claim_id().clone(),
            })
        }
        _ => Err(AppError::InternalError(
            "A restore carries the user's put back or restore event.".into(),
        )),
    }
}

/// Writes the put-back or restore-original events and report update inside `tx`.
pub(super) async fn record_in(
    tx: &mut WriteTx,
    program_id: &str,
    lesson_id: &str,
    restore: &RestorePlan,
    user: &EventProvenance,
) -> Result<Recorded> {
    let ordinal = append_in(
        tx,
        lesson_id,
        &restore.claim_id,
        user.clone(),
        restore.event.clone(),
    )
    .await?;
    let mut acted = vec![(restore.claim_id.clone(), ordinal)];
    for other in &restore.siblings {
        let sibling_ord =
            append_in(tx, lesson_id, other, user.clone(), restore.event.clone()).await?;
        acted.push((other.clone(), sibling_ord));
    }
    let machine = user.as_machine();
    for id in &restore.replaced {
        let superseded = ClaimEvent::Superseded {
            successor: Some(restore.claim_id.clone()),
            by: Actor::User,
        };
        append_in(tx, lesson_id, id, machine.clone(), superseded).await?;
    }
    let mut ids: HashSet<String> = restore.replaced.iter().map(|id| id.0.clone()).collect();
    ids.extend(restore.siblings.iter().map(|id| id.0.clone()));
    update_report(tx, program_id, lesson_id, |report| {
        report.written_back(&WrittenBack {
            unit: restore.unit,
            old_body: &restore.old_body,
            range: restore.range,
            new_body: &restore.new_body,
            replaced: &ids,
            claim: &restore.claim_id,
            sentence: &restore.sentence,
            statement: &restore.statement,
            reason: restore.reason,
            at: user.at,
        })
    })
    .await?;
    Ok(Recorded {
        acted,
        created: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edits::keys;
    use lattice_core::domain::learning::claim_review::{ClaimLocation, UnitKind};

    const SENTENCE: &str = "The reader keeps every field name exactly as written.";
    const BODY: &str = "The reader keeps every field name exactly as written. Another follows.";

    fn provenance(actor: Actor) -> EventProvenance {
        EventProvenance {
            actor,
            at: 1,
            job_id: None,
            app_version: "test".into(),
            checker_model: None,
        }
    }

    fn append(entry: &mut ClaimLedgerEntry, actor: Actor, event: ClaimEvent) {
        entry.append(provenance(actor), event).unwrap();
    }

    fn remove() -> ClaimEvent {
        ClaimEvent::UserAction {
            action: UserAction::Remove,
            note: None,
        }
    }

    fn newest_for(entry: &ClaimLedgerEntry, ordinal: u32, unbroken: bool) -> NewestEdit {
        NewestEdit {
            content: keys::section_ref("l1", 0),
            edit_id: "e1".into(),
            origin: EditOrigin::Action {
                name: "claim.remove".into(),
            },
            unbroken,
            revision: "rev".into(),
            receipt: Some(
                serde_json::to_value(ClaimsReceipt {
                    acted: vec![(entry.claim_id().0.clone(), ordinal)],
                    created: Vec::new(),
                    step: "taken_out".into(),
                    revert: false,
                })
                .unwrap(),
            ),
        }
    }

    #[test]
    fn undoable_requires_an_unbroken_action_edit_matching_the_claim_ordinal() {
        let mut entry = ClaimLedgerEntry::extracted(
            ClaimId("a".into()),
            provenance(Actor::Machine),
            ClaimLocation::locate(0, UnitKind::Block, "body", SENTENCE, BODY),
            SENTENCE.into(),
        )
        .unwrap();
        append(&mut entry, Actor::User, remove());
        let removed = entry.undo_target().unwrap();
        assert!(!undoable_in_published(&entry, None));
        assert!(undoable_in_published(
            &entry,
            Some(&newest_for(&entry, removed, true))
        ));
        assert!(!undoable_in_published(
            &entry,
            Some(&newest_for(&entry, removed, false))
        ));
        assert!(!undoable_in_published(
            &entry,
            Some(&newest_for(&entry, removed + 1, true))
        ));
    }
}
