//! A sentence put back into a published lesson, the original put in place of
//! a rewrite, and the undo of either (or of an edit): the section's text, the
//! ledger's events and the stored report change on the caller's transaction,
//! which owns the revision check and its advance, so a refusal writes nothing.

use std::collections::HashSet;

use lattice_core::domain::learning::claim_review::{
    span_of, Actor, ClaimEvent, ClaimId, ClaimLedgerEntry, EventProvenance, UserAction,
};
use lattice_core::shared::error::{AppError, Result};
use sqlx::SqliteConnection;

use super::actions::{in_section_body, ONLY_SECTION};
use super::published_edit::{gone, placed, section_body, update_report, CHANGED, UNDO_BLOCKED};
use super::published_remove::Removal;
use super::put_back::siblings;
use super::repository::{append_in, blocks, load_in};
use crate::content_verification::{insert_sentence, put_at_junction, remove_sentence, WrittenBack};

const AFTER_IT: &str = "This section changed after you removed this sentence, so it can't go back on its own. Edit the section to add it back.";
const FIRST_UNDO: &str = "First undo the other sentences you removed from this section.";
const PUT_BACK_REASON: &str = "You put this sentence back; it was not checked against the sources.";
const RESTORED_REASON: &str = "You restored the original sentence; it was not checked again.";

/// Writes the user's put back or restore `event` for `claim_id` on
/// `connection`: the text, the claim's event, and the report. The claims it
/// ended or restored are the ones it returns.
pub(super) async fn apply_in(
    connection: &mut SqliteConnection,
    lesson_id: &str,
    claim_id: &ClaimId,
    event: &ClaimEvent,
    user: &EventProvenance,
) -> Result<HashSet<ClaimId>> {
    let ledger = load_in(connection, lesson_id).await?;
    let review = ledger
        .review
        .as_ref()
        .ok_or_else(|| AppError::NotFound("This lesson has no statement review.".into()))?;
    let target = ledger
        .claims
        .iter()
        .find(|entry| entry.claim_id() == claim_id)
        .ok_or_else(|| AppError::NotFound("No statement with that id in this lesson.".into()))?;
    let (unit, new_body, old_body, range, replaced, sentence, reason) = match event {
        ClaimEvent::UserAction {
            action: UserAction::PutBack {
                sentence, place, ..
            },
            ..
        } => {
            let unit = place.unit_index;
            let body = section_body(connection, lesson_id, unit).await?;
            let new_body = insert_sentence(&body, &place.anchor, sentence)
                .ok_or_else(|| AppError::InvalidInput(CHANGED.into()))?;
            (
                unit,
                new_body,
                body,
                (0, 0),
                Vec::new(),
                sentence.clone(),
                PUT_BACK_REASON,
            )
        }
        ClaimEvent::UserAction {
            action: UserAction::RestoreOriginal { original, rewrite },
            ..
        } => {
            if !in_section_body(target) {
                return Err(AppError::InvalidInput(ONLY_SECTION.into()));
            }
            let unit = target.location().unit_index;
            let body = section_body(connection, lesson_id, unit).await?;
            let range =
                span_of(&body, rewrite).ok_or_else(|| AppError::InvalidInput(CHANGED.into()))?;
            let mut new_body = body.clone();
            new_body.replace_range(range.0..range.1, original);
            let replaced: Vec<ClaimId> = ledger
                .claims
                .iter()
                .filter(|entry| {
                    entry.claim_id() != claim_id
                        && in_section_body(entry)
                        && entry.location().unit_index == unit
                        && !gone(entry)
                        && placed(entry, &body)
                            .is_some_and(|(start, end)| start < range.1 && range.0 < end)
                })
                .map(|entry| entry.claim_id().clone())
                .collect();
            (
                unit,
                new_body,
                body,
                range,
                replaced,
                original.clone(),
                RESTORED_REASON,
            )
        }
        _ => {
            return Err(AppError::InternalError(
                "A restore carries the user's put back or restore event.".into(),
            ))
        }
    };
    append_in(connection, lesson_id, claim_id, user.clone(), event.clone()).await?;
    let mut done = HashSet::from([claim_id.clone()]);
    let mut taken_with: HashSet<String> = HashSet::new();
    if is_put_back(event) {
        for other in siblings(&ledger.claims, target) {
            append_in(
                connection,
                lesson_id,
                other.claim_id(),
                user.clone(),
                event.clone(),
            )
            .await?;
            done.insert(other.claim_id().clone());
            taken_with.insert(other.claim_id().0.clone());
        }
    }
    for id in &replaced {
        let superseded = ClaimEvent::Superseded {
            successor: Some(claim_id.clone()),
            by: Actor::User,
        };
        append_in(connection, lesson_id, id, user.clone(), superseded).await?;
    }
    blocks::write_section_body(connection, lesson_id, unit, &new_body).await?;
    let mut ids: HashSet<String> = replaced.iter().map(|id| id.0.clone()).collect();
    ids.extend(taken_with);
    update_report(connection, &review.program_id, lesson_id, |report| {
        report.written_back(&WrittenBack {
            unit,
            old_body: &old_body,
            range,
            new_body: &new_body,
            replaced: &ids,
            claim: claim_id,
            sentence: &sentence,
            statement: target.statement(),
            reason,
            at: user.at,
        })
    })
    .await?;
    Ok(done)
}

/// Whether `event` is the user's put back of a sentence.
fn is_put_back(event: &ClaimEvent) -> bool {
    matches!(
        event,
        ClaimEvent::UserAction {
            action: UserAction::PutBack { .. },
            ..
        }
    )
}

/// The claims an undo of `target`'s last user event takes back, with the
/// text that goes back into the section and the one it replaces.
struct Undoing {
    unit: usize,
    /// The section's text once the undo is written.
    then: String,
    /// Claims whose last user event is undone with the claim's.
    undone: Vec<(ClaimId, u32)>,
    /// A claim an edit made that goes out with the edit.
    successor: Option<ClaimId>,
    /// The undo takes a removal back: the findings it dropped return.
    removal: bool,
}

/// What the removal at `ordinal` took out of the claim's section: the text and
/// the texts either side of it, as the `TextRemoved` recorded with it noted
/// them.
fn removal_of(entry: &ClaimLedgerEntry, ordinal: u32) -> Option<Removal> {
    match recorded_with(entry, ordinal)? {
        ClaimEvent::TextRemoved {
            text,
            follows,
            precedes,
        } => Some((text.clone(), follows.clone(), precedes.clone())),
        _ => None,
    }
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

/// Why a removal cannot go back: another sentence the user removed from the
/// section is still out, or the section changed.
fn removal_refusal(claims: &[ClaimLedgerEntry], unit: usize, removed: &Removal) -> AppError {
    let still_out = claims.iter().any(|entry| {
        entry.location().unit_index == unit
            && in_section_body(entry)
            && last_user_event(entry).is_some_and(|(ordinal, event)| {
                matches!(
                    event,
                    ClaimEvent::UserAction {
                        action: UserAction::Remove,
                        ..
                    }
                ) && removal_of(entry, ordinal).is_some_and(|other| other != *removed)
            })
    });
    AppError::InvalidState(if still_out { FIRST_UNDO } else { AFTER_IT }.into())
}

fn last_user_event(entry: &ClaimLedgerEntry) -> Option<(u32, &ClaimEvent)> {
    let ordinal = entry.undo_target()?;
    entry
        .events()
        .iter()
        .find(|record| record.ordinal == ordinal)
        .map(|record| (ordinal, &record.event))
}

/// Whether the claim's undo target is one a published lesson undoes: the
/// user's edit, put back or restore of the sentence, or the source that
/// came with a put back.
pub(super) fn undoable_in_published(entry: &ClaimLedgerEntry) -> bool {
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
                | UserAction::PutBack { .. }
                | UserAction::RestoreOriginal { .. },
            ..
        } => true,
        ClaimEvent::UserAction {
            action: UserAction::Remove,
            ..
        } => removal_of(entry, ordinal).is_some(),
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

/// Takes back the claim's last user event on a published lesson, on
/// `connection`. The claims it took the event back from are the ones it
/// returns.
pub(super) async fn undo_in(
    connection: &mut SqliteConnection,
    lesson_id: &str,
    claim_id: &ClaimId,
    user: &EventProvenance,
) -> Result<HashSet<ClaimId>> {
    let ledger = load_in(connection, lesson_id).await?;
    let review = ledger
        .review
        .as_ref()
        .ok_or_else(|| AppError::NotFound("This lesson has no statement review.".into()))?;
    let target = ledger
        .claims
        .iter()
        .find(|entry| entry.claim_id() == claim_id)
        .ok_or_else(|| AppError::NotFound("No statement with that id in this lesson.".into()))?;
    let (ordinal, event) = last_user_event(target).ok_or_else(|| {
        AppError::InvalidState("There is nothing to undo on this statement.".into())
    })?;
    let by_hand = || AppError::InvalidState(UNDO_BLOCKED.into());
    let undoing = match event {
        ClaimEvent::UserAction {
            action: UserAction::PutBack {
                sentence, place, ..
            },
            ..
        } => {
            let unit = place.unit_index;
            let body = section_body(connection, lesson_id, unit).await?;
            let new_body = remove_sentence(&body, sentence).ok_or_else(by_hand)?;
            let mut undone = vec![(claim_id.clone(), ordinal)];
            undone.extend(ledger.claims.iter().filter_map(|entry| {
                let (other, last) = last_user_event(entry)?;
                let same = matches!(
                    last,
                    ClaimEvent::UserAction {
                        action: UserAction::PutBack { sentence: put, place: at, .. },
                        ..
                    } if put == sentence && at == place
                );
                (same && entry.claim_id() != claim_id).then(|| (entry.claim_id().clone(), other))
            }));
            Some(Undoing {
                unit,
                then: new_body,
                undone,
                successor: None,
                removal: false,
            })
        }
        ClaimEvent::UserAction {
            action: UserAction::Remove,
            ..
        } => {
            let removed = removal_of(target, ordinal).ok_or_else(by_hand)?;
            let unit = target.location().unit_index;
            let body = section_body(connection, lesson_id, unit).await?;
            let new_body = put_at_junction(
                &body,
                &removed.0,
                removed.1.as_deref(),
                removed.2.as_deref(),
            )
            .ok_or_else(|| removal_refusal(&ledger.claims, unit, &removed))?;
            let mut undone = vec![(claim_id.clone(), ordinal)];
            undone.extend(ledger.claims.iter().filter_map(|entry| {
                let (other, last) = last_user_event(entry)?;
                let same = matches!(
                    last,
                    ClaimEvent::UserAction {
                        action: UserAction::Remove,
                        ..
                    }
                ) && entry.location().unit_index == unit
                    && removal_of(entry, other).as_ref() == Some(&removed);
                (same && entry.claim_id() != claim_id).then(|| (entry.claim_id().clone(), other))
            }));
            Some(Undoing {
                unit,
                then: new_body,
                undone,
                successor: None,
                removal: true,
            })
        }
        ClaimEvent::UserAction {
            action: UserAction::RestoreOriginal { original, rewrite },
            ..
        } => {
            let unit = target.location().unit_index;
            let body = section_body(connection, lesson_id, unit).await?;
            let range = span_of(&body, original).ok_or_else(by_hand)?;
            let mut new_body = body.clone();
            new_body.replace_range(range.0..range.1, rewrite);
            let mut undone = vec![(claim_id.clone(), ordinal)];
            undone.extend(ledger.claims.iter().filter_map(|entry| {
                let (other, event) = last_user_event(entry)?;
                matches!(
                    event,
                    ClaimEvent::Superseded { successor: Some(by), .. } if by == claim_id
                )
                .then(|| (entry.claim_id().clone(), other))
            }));
            Some(Undoing {
                unit,
                then: new_body,
                undone,
                successor: None,
                removal: false,
            })
        }
        ClaimEvent::UserAction {
            action: UserAction::Edit { before, after },
            ..
        } => {
            let successor = target
                .events()
                .iter()
                .find_map(|record| match &record.event {
                    ClaimEvent::Superseded {
                        successor: Some(successor),
                        ..
                    } if record.ordinal > ordinal => Some(successor.clone()),
                    _ => None,
                });
            let successor = successor.ok_or_else(by_hand)?;
            let unit = target.location().unit_index;
            let body = section_body(connection, lesson_id, unit).await?;
            let range = span_of(&body, after).ok_or_else(by_hand)?;
            let mut new_body = body.clone();
            new_body.replace_range(range.0..range.1, before);
            let undone = ledger
                .claims
                .iter()
                .filter_map(|entry| {
                    let (other, event) = last_user_event(entry)?;
                    let edited = matches!(
                        event,
                        ClaimEvent::UserAction {
                            action: UserAction::Edit { .. },
                            ..
                        }
                    );
                    let by_edit = entry.events().iter().any(|record| {
                        record.ordinal > other
                            && matches!(
                                &record.event,
                                ClaimEvent::Superseded { successor: Some(s), .. } if *s == successor
                            )
                    });
                    (edited && by_edit).then(|| (entry.claim_id().clone(), other))
                })
                .collect();
            Some(Undoing {
                unit,
                then: new_body,
                undone,
                successor: Some(successor),
                removal: false,
            })
        }
        _ => None,
    };
    let mut done = HashSet::from([claim_id.clone()]);
    let written = match undoing {
        Some(undoing) => {
            done.extend(undoing.undone.iter().map(|(id, _)| id.clone()));
            for (id, ordinal) in &undoing.undone {
                let undone = ClaimEvent::Undone {
                    event_ordinal: *ordinal,
                };
                append_in(connection, lesson_id, id, user.clone(), undone).await?;
            }
            if let Some(successor) = &undoing.successor {
                let ended = ClaimEvent::Superseded {
                    successor: None,
                    by: Actor::User,
                };
                let machine = EventProvenance {
                    actor: Actor::Machine,
                    ..user.clone()
                };
                append_in(connection, lesson_id, successor, machine, ended).await?;
            }
            blocks::write_section_body(connection, lesson_id, undoing.unit, &undoing.then).await?;
            let mut claims: HashSet<String> =
                undoing.undone.iter().map(|(id, _)| id.0.clone()).collect();
            claims.extend(undoing.successor.iter().map(|id| id.0.clone()));
            Some((undoing.unit, undoing.then, claims, undoing.removal))
        }
        None => {
            let undone = ClaimEvent::Undone {
                event_ordinal: ordinal,
            };
            append_in(connection, lesson_id, claim_id, user.clone(), undone).await?;
            None
        }
    };
    if let Some((unit, body, claims, removal)) = written {
        update_report(connection, &review.program_id, lesson_id, |report| {
            if removal {
                report.taken_back(unit, &body, &claims);
            } else {
                report.written_out(unit, &body, &claims);
            }
        })
        .await?;
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn a_removal_with_no_record_of_its_own_is_not_undoable_though_an_earlier_one_has_a_record() {
        let mut entry = ClaimLedgerEntry::extracted(
            ClaimId("a".into()),
            provenance(Actor::Machine),
            ClaimLocation::locate(0, UnitKind::Block, "body", SENTENCE, BODY),
            SENTENCE.into(),
        )
        .unwrap();
        append(
            &mut entry,
            Actor::Machine,
            ClaimEvent::TextRemoved {
                text: SENTENCE.into(),
                follows: None,
                precedes: Some("Another follows.".into()),
            },
        );
        append(&mut entry, Actor::User, remove());
        assert!(undoable_in_published(&entry));
        let removed = entry.undo_target().unwrap();
        append(
            &mut entry,
            Actor::User,
            ClaimEvent::Undone {
                event_ordinal: removed,
            },
        );
        append(&mut entry, Actor::User, remove());
        assert!(!undoable_in_published(&entry));
    }
}
