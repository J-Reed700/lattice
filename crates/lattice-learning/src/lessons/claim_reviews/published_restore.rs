//! A sentence put back into a published lesson, the original put in place of
//! a rewrite, and the undo of either (or of an edit): the section's text, the
//! ledger's events and the stored report change in one transaction, so a
//! refusal writes nothing.

use std::collections::HashSet;

use lattice_core::domain::learning::claim_review::{
    span_of, Actor, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimState, EventProvenance, Revision,
    UserAction,
};
use lattice_core::shared::error::{AppError, Result};
use sqlx::{Sqlite, SqlitePool, Transaction};

use super::actions::{in_section_body, ONLY_SECTION};
use super::published_edit::CHANGED;
use super::repository::{append_in, bump_revision, load_in, revision_conflict};
use crate::content_verification::{
    insert_sentence, located, remove_sentence, LessonVerificationReport, WrittenBack,
};
use crate::persistence::db;

const BY_HAND: &str = "The section changed after this; undo it by hand.";
const PUT_BACK_REASON: &str = "You put this sentence back; it was not checked against the sources.";
const RESTORED_REASON: &str = "You restored the original sentence; it was not checked again.";

/// What the user puts into the section's text.
pub(super) enum Restoring<'a> {
    /// The sentence the machine took out, at `place`.
    PutBack,
    /// The original in place of the machine's rewrite.
    Original { original: &'a str, rewrite: &'a str },
}

pub(super) struct Restore<'a> {
    pub lesson_id: &'a str,
    pub claim_id: &'a ClaimId,
    pub restoring: Restoring<'a>,
    /// The user's event on the claim: `UserAction::PutBack` or
    /// `UserAction::RestoreOriginal`.
    pub event: ClaimEvent,
    pub expected_revision: Revision,
    pub user: EventProvenance,
}

fn gone(entry: &ClaimLedgerEntry) -> bool {
    matches!(
        entry.state(),
        ClaimState::Removed { .. } | ClaimState::Edited { .. }
    )
}

/// Where the claim's sentence is in `body`: the span it recorded when the
/// text there is still the sentence, else the sentence's only occurrence.
fn placed(entry: &ClaimLedgerEntry, body: &str) -> Option<(usize, usize)> {
    let (sentence, span) = located(entry.location(), entry.statement(), body);
    span.filter(|&(start, end)| body.get(start..end) == Some(sentence.as_str()))
        .or_else(|| span_of(body, &sentence))
}

async fn body_of(tx: &mut Transaction<'_, Sqlite>, lesson_id: &str, unit: usize) -> Result<String> {
    sqlx::query_scalar("SELECT body FROM learning_blocks WHERE lesson_id=? AND ordinal=?")
        .bind(lesson_id)
        .bind(i64::try_from(unit).unwrap_or(i64::MAX))
        .fetch_optional(&mut **tx)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::InvalidInput(CHANGED.into()))
}

async fn write_body(
    tx: &mut Transaction<'_, Sqlite>,
    lesson_id: &str,
    unit: usize,
    body: &str,
) -> Result<()> {
    sqlx::query("UPDATE learning_blocks SET body=? WHERE lesson_id=? AND ordinal=?")
        .bind(body)
        .bind(lesson_id)
        .bind(i64::try_from(unit).unwrap_or(i64::MAX))
        .execute(&mut **tx)
        .await
        .map_err(db)?;
    Ok(())
}

/// Applies `change` to the stored report of the lesson, when it has one.
async fn with_report(
    tx: &mut Transaction<'_, Sqlite>,
    program_id: &str,
    lesson_id: &str,
    change: impl FnOnce(&mut LessonVerificationReport),
) -> Result<()> {
    let stored: Option<String> = sqlx::query_scalar(
        "SELECT report_json FROM learning_lesson_verifications WHERE program_id=? AND lesson_id=?",
    )
    .bind(program_id)
    .bind(lesson_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db)?;
    let Some(stored) = stored else {
        return Ok(());
    };
    let mut report: LessonVerificationReport = serde_json::from_str(&stored)?;
    change(&mut report);
    sqlx::query(
        "UPDATE learning_lesson_verifications SET content_sha256=?,report_json=? WHERE program_id=? AND lesson_id=?",
    )
    .bind(report.content_sha256())
    .bind(serde_json::to_string(&report)?)
    .bind(program_id)
    .bind(lesson_id)
    .execute(&mut **tx)
    .await
    .map_err(db)?;
    Ok(())
}

pub(super) async fn apply(pool: &SqlitePool, restore: Restore<'_>) -> Result<Revision> {
    let Restore {
        lesson_id,
        claim_id,
        restoring,
        event,
        expected_revision,
        user,
    } = restore;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;
    let ledger = load_in(&mut tx, lesson_id).await?;
    let review = ledger
        .review
        .as_ref()
        .ok_or_else(|| AppError::NotFound("This lesson has no claim review.".into()))?;
    if ledger.revision != expected_revision {
        return Err(revision_conflict());
    }
    let target = ledger
        .claims
        .iter()
        .find(|entry| entry.claim_id() == claim_id)
        .ok_or_else(|| AppError::NotFound("No claim with that id in this lesson.".into()))?;
    let (unit, new_body, old_body, range, replaced, sentence, reason) = match (&restoring, &event) {
        (
            Restoring::PutBack,
            ClaimEvent::UserAction {
                action:
                    UserAction::PutBack {
                        sentence, place, ..
                    },
                ..
            },
        ) => {
            let unit = place.unit_index;
            let body = body_of(&mut tx, lesson_id, unit).await?;
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
        (Restoring::Original { original, rewrite }, _) => {
            if !in_section_body(target) {
                return Err(AppError::InvalidInput(ONLY_SECTION.into()));
            }
            let unit = target.location().unit_index;
            let body = body_of(&mut tx, lesson_id, unit).await?;
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
                (*original).to_owned(),
                RESTORED_REASON,
            )
        }
        _ => {
            return Err(AppError::InternalError(
                "A restore carries the user's put back or restore event.".into(),
            ))
        }
    };
    append_in(&mut tx, lesson_id, claim_id, user.clone(), event).await?;
    for id in &replaced {
        let superseded = ClaimEvent::Superseded {
            successor: Some(claim_id.clone()),
            by: Actor::User,
        };
        append_in(&mut tx, lesson_id, id, user.clone(), superseded).await?;
    }
    write_body(&mut tx, lesson_id, unit, &new_body).await?;
    let ids: HashSet<String> = replaced.iter().map(|id| id.0.clone()).collect();
    with_report(&mut tx, &review.program_id, lesson_id, |report| {
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
    bump_revision(&mut tx, lesson_id).await?;
    tx.commit().await.map_err(db)?;
    Ok(Revision(expected_revision.0 + 1))
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
    match event {
        ClaimEvent::UserAction {
            action:
                UserAction::Edit { .. }
                | UserAction::PutBack { .. }
                | UserAction::RestoreOriginal { .. },
            ..
        } => true,
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

/// Takes back the claim's last user event on a published lesson.
pub(super) async fn undo(
    pool: &SqlitePool,
    lesson_id: &str,
    claim_id: &ClaimId,
    expected_revision: Revision,
    user: EventProvenance,
) -> Result<Revision> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;
    let ledger = load_in(&mut tx, lesson_id).await?;
    let review = ledger
        .review
        .as_ref()
        .ok_or_else(|| AppError::NotFound("This lesson has no claim review.".into()))?;
    if ledger.revision != expected_revision {
        return Err(revision_conflict());
    }
    let target = ledger
        .claims
        .iter()
        .find(|entry| entry.claim_id() == claim_id)
        .ok_or_else(|| AppError::NotFound("No claim with that id in this lesson.".into()))?;
    let (ordinal, event) = last_user_event(target)
        .ok_or_else(|| AppError::InvalidState("There is nothing to undo on this claim.".into()))?;
    let by_hand = || AppError::InvalidState(BY_HAND.into());
    let undoing = match event {
        ClaimEvent::UserAction {
            action: UserAction::PutBack {
                sentence, place, ..
            },
            ..
        } => {
            let unit = place.unit_index;
            let body = body_of(&mut tx, lesson_id, unit).await?;
            let new_body = remove_sentence(&body, sentence).ok_or_else(by_hand)?;
            Some(Undoing {
                unit,
                then: new_body,
                undone: vec![(claim_id.clone(), ordinal)],
                successor: None,
            })
        }
        ClaimEvent::UserAction {
            action: UserAction::RestoreOriginal { original, rewrite },
            ..
        } => {
            let unit = target.location().unit_index;
            let body = body_of(&mut tx, lesson_id, unit).await?;
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
            let body = body_of(&mut tx, lesson_id, unit).await?;
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
            })
        }
        _ => None,
    };
    let written = match undoing {
        Some(undoing) => {
            for (id, ordinal) in &undoing.undone {
                let undone = ClaimEvent::Undone {
                    event_ordinal: *ordinal,
                };
                append_in(&mut tx, lesson_id, id, user.clone(), undone).await?;
            }
            if let Some(successor) = &undoing.successor {
                let gone = ClaimEvent::Superseded {
                    successor: None,
                    by: Actor::User,
                };
                let machine = EventProvenance {
                    actor: Actor::Machine,
                    ..user.clone()
                };
                append_in(&mut tx, lesson_id, successor, machine, gone).await?;
            }
            write_body(&mut tx, lesson_id, undoing.unit, &undoing.then).await?;
            let claim = undoing
                .successor
                .or_else(|| undoing.undone.first().map(|(id, _)| id.clone()));
            claim.map(|claim| (undoing.unit, undoing.then, claim))
        }
        None => {
            let undone = ClaimEvent::Undone {
                event_ordinal: ordinal,
            };
            append_in(&mut tx, lesson_id, claim_id, user.clone(), undone).await?;
            None
        }
    };
    if let Some((unit, body, claim)) = written {
        with_report(&mut tx, &review.program_id, lesson_id, |report| {
            report.written_out(unit, &body, &claim)
        })
        .await?;
    }
    bump_revision(&mut tx, lesson_id).await?;
    tx.commit().await.map_err(db)?;
    Ok(Revision(expected_revision.0 + 1))
}
