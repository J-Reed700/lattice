//! The review pass (§6.2): what the user did while the lesson waited, carried
//! into the draft at the loop's sync points. An edit replaces its sentence and
//! a removal takes it out; re-extraction then names an edited claim's
//! successors, which are checked like any new claim. A hint is researched
//! once. Settled claims are left alone, and claims still waiting stay waiting:
//! the loop's decision after the pass hands the lesson back when anything is
//! still open.
//! Design: docs/design/2026-10-16-lesson-claim-review.md §5.1, §6.2, §9.

use super::claim_ledger::ClaimLedger;
use super::*;
use lattice_core::domain::learning::claim_review::{
    Actor, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimState, UserAction,
};

/// The user's research hint for a claim waiting for them, until a research
/// round has run with it (§5.1 "Research again with a hint").
pub(super) fn pending_hint(entry: &ClaimLedgerEntry) -> Option<String> {
    if !entry.state().awaits_user() {
        return None;
    }
    let events = entry.events();
    let (position, hint) =
        events
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, record)| match &record.event {
                ClaimEvent::UserAction {
                    action: UserAction::ResearchWithHint { hint },
                    ..
                } if record.provenance.actor == Actor::User => Some((index, hint.clone())),
                _ => None,
            })?;
    let researched = events
        .iter()
        .skip(position + 1)
        .any(|record| matches!(record.event, ClaimEvent::Researched { .. }));
    (!researched).then_some(hint)
}

/// Whether re-extraction already named the successor of the claim's latest
/// edit, so the edit is in the draft.
fn edit_applied(entry: &ClaimLedgerEntry) -> bool {
    let events = entry.events();
    let edit = events.iter().rposition(|record| {
        matches!(
            record.event,
            ClaimEvent::UserAction {
                action: UserAction::Edit { .. },
                ..
            }
        )
    });
    edit.is_some_and(|edit| {
        events
            .iter()
            .skip(edit + 1)
            .any(|record| matches!(record.event, ClaimEvent::Superseded { .. }))
    })
}

fn pointer(candidate: &Value, unit: usize, field: &str) -> Option<String> {
    let blocks = candidate.get("blocks")?.as_array()?.len();
    Some(if unit < blocks {
        format!("/blocks/{unit}/{field}")
    } else {
        format!("/questions/{}/{field}", unit - blocks)
    })
}

/// One change the user asked for, located in the draft.
enum Change {
    Edit {
        claim: ClaimId,
        unit: usize,
        pointer: String,
        before: String,
        after: String,
    },
    Remove {
        pointer: String,
        quote: String,
    },
}

/// The user's edits and removals, located in the draft.
fn changes(ledger: &ClaimLedger, candidate: &Value) -> Vec<Change> {
    let mut changes = Vec::new();
    for entry in ledger.entries() {
        let location = entry.location();
        let Some(pointer) = pointer(candidate, location.unit_index, &location.field) else {
            continue;
        };
        match (entry.state(), ClaimLedger::user_change(entry)) {
            (
                ClaimState::Edited {
                    successor: None, ..
                },
                Some(UserAction::Edit { before, after }),
            ) if !edit_applied(entry) => changes.push(Change::Edit {
                claim: entry.claim_id().clone(),
                unit: location.unit_index,
                pointer,
                before: before.clone(),
                after: after.clone(),
            }),
            (
                ClaimState::Removed {
                    by: Actor::User, ..
                },
                _,
            ) => {
                // The claim's own sentence, not a passage it was located by
                // (claim sources design §18.3).
                let text = candidate
                    .pointer(&pointer)
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let quote = super::located_quote(location, entry.statement(), text);
                changes.push(Change::Remove { pointer, quote })
            }
            _ => {}
        }
    }
    changes
}

/// What applying the user's changes did beyond the text.
#[derive(Default)]
struct Applied {
    /// Claims whose edit is now in the draft.
    edited: Vec<ClaimId>,
    /// Units a repair rewrote after the user's edit: the user's text wins.
    superseded_units: Vec<usize>,
}

/// Applies `changes` to `candidate`. `previous` is the lesson as the user
/// saw it, when a repair has changed it since.
fn apply(previous: Option<&Value>, candidate: &mut Value, changes: Vec<Change>) -> Applied {
    let mut applied = Applied::default();
    for change in changes {
        match change {
            Change::Edit {
                claim,
                unit,
                pointer,
                before,
                after,
            } => {
                let Some(text) = candidate.pointer(&pointer).and_then(Value::as_str) else {
                    continue;
                };
                let text = if text.matches(before.as_str()).count() == 1 {
                    text.replacen(before.as_str(), &after, 1)
                } else if !after.is_empty() && text.contains(after.as_str()) {
                    continue;
                } else {
                    // A repair rewrote the section after the user's edit was
                    // recorded: the user's edit wins, on the section as the
                    // user saw it (§9).
                    let seen = previous
                        .and_then(|previous| previous.pointer(&pointer))
                        .and_then(Value::as_str)
                        .filter(|seen| seen.matches(before.as_str()).count() == 1);
                    let Some(seen) = seen else {
                        tracing::warn!(claim = %claim.0, "claim.user_action: the edited sentence is no longer in its section; the edit was not applied");
                        continue;
                    };
                    applied.superseded_units.push(unit);
                    seen.replacen(before.as_str(), &after, 1)
                };
                if let Some(slot) = candidate.pointer_mut(&pointer) {
                    *slot = Value::String(text);
                    applied.edited.push(claim);
                }
            }
            Change::Remove { pointer, quote } => {
                let Some(text) = candidate.pointer(&pointer).and_then(Value::as_str) else {
                    continue;
                };
                if !text.contains(quote.as_str()) {
                    continue;
                }
                let spaced = format!("{quote} ");
                let text = if text.contains(spaced.as_str()) {
                    text.replacen(spaced.as_str(), "", 1)
                } else {
                    text.replacen(quote.as_str(), "", 1)
                };
                if let Some(slot) = candidate.pointer_mut(&pointer) {
                    *slot = Value::String(text);
                }
            }
        }
    }
    applied
}

/// Whether the user changed something the draft does not hold yet.
pub(super) fn has_user_changes(ledger: &ClaimLedger, candidate: &Value) -> bool {
    let mut copy = candidate.clone();
    apply(
        ledger.pending.previous.as_ref(),
        &mut copy,
        changes(ledger, candidate),
    );
    copy != *candidate
}

/// Applies the user's edits and removals the draft does not hold yet. Returns
/// whether the draft changed; the ledger then remembers the lesson as it
/// stood, so identity compares each claim's old place with its new one.
pub(super) async fn apply_user_changes(
    ledger: &mut ClaimLedger,
    candidate: &mut Value,
) -> Result<bool> {
    let original = candidate.clone();
    let changes = changes(ledger, candidate);
    let applied = apply(ledger.pending.previous.as_ref(), candidate, changes);
    if *candidate == original {
        return Ok(false);
    }
    let superseded: Vec<ClaimId> = ledger
        .pending
        .repairs
        .iter()
        .filter(|record| applied.superseded_units.contains(&record.unit))
        .map(|record| record.claim_id.clone())
        .collect();
    if !superseded.is_empty() {
        ledger.supersede(&superseded).await?;
    }
    ledger.pending.previous.get_or_insert(original);
    ledger.edited_now.extend(applied.edited);
    ledger.save_pending().await?;
    Ok(true)
}
