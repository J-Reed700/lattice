//! Running published-lesson claim text actions and published text undos
//! through `ContentEdits`.

use std::collections::{HashMap, HashSet};

use lattice_core::domain::learning::claim_review::{
    ClaimEvent, ClaimId, ClaimLedgerEntry, Ledger, Revision, UserAction,
};
use lattice_core::shared::edits::{
    ContentAddress, ContentEditRequest, ContentEdits, EditOp, EditOrigin, EditSource,
    RevertContentEditRequest, Uncommitted,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::WriteTx;
use lattice_core::shared::text::selector::TextSelector;
use serde_json::{json, Value};
use uuid::Uuid;

use super::edit_reactor::ClaimsContext;
use super::published_edit::{self, UNDO_BLOCKED};
use super::published_remove;
use super::published_restore::{self, action_unit, claims_receipt, last_user_event, receipt_holds};
use super::repository::{bump_revision, load_in, revision_in};
use super::teaching_claims::removable_span;
use super::teaching_findings::TeachingFindingDto;
use crate::edits::keys;

/// Returns `Some(ordinal)` when `entry`'s undo target is a published claim
/// action that changed a section's body text.
pub(super) fn text_undo_ordinal(entry: &ClaimLedgerEntry) -> Option<u32> {
    let (ordinal, event) = last_user_event(entry)?;
    matches!(
        event,
        ClaimEvent::UserAction {
            action: UserAction::Edit { .. }
                | UserAction::Remove
                | UserAction::PutBack { .. }
                | UserAction::RestoreOriginal { .. },
            ..
        }
    )
    .then_some(ordinal)
}

/// Applies one published text-changing claim action (`Edit`, `Remove`, `PutBack`,
/// `RestoreOriginal`) through `ContentEdits::apply_in`, updating `ended` with
/// every claim ended by the edit.
pub(super) async fn apply_claim_action_in(
    tx: &mut WriteTx,
    edits: &ContentEdits,
    lesson_id: &str,
    entry: &ClaimLedgerEntry,
    event: &ClaimEvent,
    ended: &mut HashSet<ClaimId>,
) -> Result<Uncommitted> {
    let claims = load_in(&mut *tx, lesson_id).await?.claims;
    let id = entry.claim_id();
    let unit = match event {
        ClaimEvent::UserAction {
            action: UserAction::PutBack { place, .. },
            ..
        } => place.unit_index,
        _ => entry.location().unit_index,
    };
    let content = keys::section_ref(lesson_id, unit);
    let opened = edits.open_in(&mut tx.reading(), &content).await?;
    let body = opened
        .document
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();

    let (action_name, new_body, fragment_span) = match event {
        ClaimEvent::UserAction {
            action: UserAction::Edit { before, after },
            ..
        } => {
            let plan = published_edit::plan(&claims, id, before, after, &body)?;
            ended.extend(plan.replaced);
            ("claim.edit", plan.new_body, Some(plan.range))
        }
        ClaimEvent::UserAction {
            action: UserAction::Remove,
            ..
        } => {
            let plan = published_remove::plan(&claims, id, &body)?;
            ended.extend(plan.removed);
            ("claim.remove", plan.new_body, Some(plan.range))
        }
        ClaimEvent::UserAction {
            action: UserAction::PutBack { .. },
            ..
        } => {
            let plan = published_restore::plan(&claims, entry, event, &body)?;
            ended.insert(id.clone());
            ended.extend(plan.siblings);
            ("claim.put_back", plan.new_body, None)
        }
        ClaimEvent::UserAction {
            action: UserAction::RestoreOriginal { .. },
            ..
        } => {
            let plan = published_restore::plan(&claims, entry, event, &body)?;
            ended.insert(id.clone());
            ended.extend(plan.replaced);
            ("claim.restore", plan.new_body, Some(plan.range))
        }
        _ => {
            return Err(AppError::InternalError(
                "Unexpected non-text claim action.".into(),
            ))
        }
    };

    let fragment = fragment_span.and_then(|(start, end)| TextSelector::of(&body, start, end));
    let context = serde_json::to_value(&ClaimsContext::Claim {
        claim_id: id.0.clone(),
        event: Box::new(event.clone()),
    })?;
    edits
        .apply_in(
            tx,
            &ContentEditRequest {
                edit_id: Uuid::new_v4().to_string(),
                content: content.clone(),
                expected_revision: opened.revision,
                ops: vec![EditOp::Replace {
                    path: "/body".into(),
                    value: json!(new_body),
                }],
                source: EditSource::Action {
                    name: action_name.into(),
                },
                target: Some(ContentAddress {
                    content,
                    path: Some("/body".into()),
                    fragment,
                }),
            },
            Some(&context),
        )
        .await
}

/// Applies an ad-hoc sentence removal (for a waiting note with no existing live
/// claim) through `ContentEdits::apply_in`, returning the created note claim ID,
/// all removed claim IDs, and the uncommitted follow-up.
pub(super) async fn apply_sentence_removal_in(
    tx: &mut WriteTx,
    edits: &ContentEdits,
    lesson_id: &str,
    unit: usize,
    sentence: &str,
) -> Result<(ClaimId, Vec<ClaimId>, Uncommitted)> {
    let claims = load_in(&mut *tx, lesson_id).await?.claims;
    let content = keys::section_ref(lesson_id, unit);
    let opened = edits.open_in(&mut tx.reading(), &content).await?;
    let body = opened
        .document
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let plan = published_remove::plan_sentence(&claims, unit, sentence, &body)?;
    let fragment = TextSelector::of(&body, plan.range.0, plan.range.1);
    let context = serde_json::to_value(&ClaimsContext::Sentence {
        sentence: sentence.to_owned(),
    })?;
    let uncommitted = edits
        .apply_in(
            tx,
            &ContentEditRequest {
                edit_id: Uuid::new_v4().to_string(),
                content: content.clone(),
                expected_revision: opened.revision,
                ops: vec![EditOp::Replace {
                    path: "/body".into(),
                    value: json!(plan.new_body),
                }],
                source: EditSource::Action {
                    name: "claim.remove_sentence".into(),
                },
                target: Some(ContentAddress {
                    content: content.clone(),
                    path: Some("/body".into()),
                    fragment,
                }),
            },
            Some(&context),
        )
        .await?;
    let mut list = edits
        .newest(&mut tx.reading(), &[content], keys::CLAIMS_REACTOR)
        .await?;
    let created = list
        .pop()
        .and_then(|newest| claims_receipt(&newest))
        .and_then(|receipt| receipt.created.into_iter().next())
        .map(ClaimId)
        .ok_or_else(|| {
            AppError::InternalError("Missing sentence claim created by reactor".into())
        })?;
    Ok((created, plan.removed, uncommitted))
}

/// Applies all chosen teaching-note sentence removals through `ContentEdits`, returning
/// ended claims, claims settled via claim actions, the resulting revision, and follow-ups.
pub(super) async fn remove_note_sentences_in(
    tx: &mut WriteTx,
    edits: &ContentEdits,
    ledger: &Ledger,
    chosen: &[&TeachingFindingDto],
) -> Result<(Vec<ClaimId>, Vec<ClaimId>, Revision, Vec<Uncommitted>)> {
    let lesson_id = ledger.lesson_id.as_str();
    let mut ended: Vec<ClaimId> = Vec::new();
    let mut through_claims: Vec<ClaimId> = Vec::new();
    let mut taken: HashSet<(u32, String)> = HashSet::new();
    let mut uncommitted: Vec<Uncommitted> = Vec::new();
    let mut wrote_via_edits = false;
    let remove_event = ClaimEvent::UserAction {
        action: UserAction::Remove,
        note: None,
    };
    for &finding in chosen {
        let (Some(unit), Some(sentence)) = (finding.unit_index, finding.sentence.as_deref()) else {
            continue;
        };
        let sentence = sentence.trim();
        if !taken.insert((unit, sentence.to_owned())) {
            continue;
        }
        let unit = usize::try_from(unit).unwrap_or(usize::MAX);
        let content = keys::section_ref(lesson_id, unit);
        let opened = edits.open_in(&mut tx.reading(), &content).await?;
        let body = opened
            .document
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if removable_span(finding, &body).is_none() {
            return Err(AppError::InvalidInput(published_edit::CHANGED.into()));
        }
        let target = finding
            .claim_ids
            .iter()
            .map(|id| ClaimId(id.clone()))
            .find(|id| !ended.contains(id));
        let (target, mut others) = match target {
            Some(id) => {
                let Some(entry) = ledger.claims.iter().find(|entry| entry.claim_id() == &id) else {
                    continue;
                };
                let mut step_ended = HashSet::new();
                let pending = apply_claim_action_in(
                    tx,
                    edits,
                    lesson_id,
                    entry,
                    &remove_event,
                    &mut step_ended,
                )
                .await?;
                uncommitted.push(pending);
                wrote_via_edits = true;
                through_claims.push(id.clone());
                let others: Vec<ClaimId> = step_ended
                    .into_iter()
                    .filter(|other| other != &id)
                    .collect();
                (id, others)
            }
            None if finding.claim_ids.is_empty() => {
                let (target, others, pending) =
                    apply_sentence_removal_in(tx, edits, lesson_id, unit, sentence).await?;
                uncommitted.push(pending);
                wrote_via_edits = true;
                (target, others)
            }
            None => continue,
        };
        others.sort_by_key(|id| id.0.clone());
        for id in std::iter::once(target).chain(others) {
            if !ended.contains(&id) {
                ended.push(id);
            }
        }
    }
    let written = if wrote_via_edits {
        revision_in(tx, lesson_id).await?.unwrap_or(ledger.revision)
    } else {
        bump_revision(tx, lesson_id).await?
    };
    Ok((ended, through_claims, written, uncommitted))
}

/// Undoes one published claim text action via `ContentEdits::revert_in`, or
/// returns `InvalidState(UNDO_BLOCKED)`.
pub(super) async fn revert_single_in(
    tx: &mut WriteTx,
    edits: &ContentEdits,
    lesson_id: &str,
    entry: &ClaimLedgerEntry,
    ordinal: u32,
    ended: &mut HashSet<ClaimId>,
) -> Result<Uncommitted> {
    let unit = action_unit(entry);
    let content = keys::section_ref(lesson_id, unit);
    let mut list = edits
        .newest(&mut tx.reading(), &[content], keys::CLAIMS_REACTOR)
        .await?;
    let Some(newest) = list.pop() else {
        return Err(AppError::InvalidState(UNDO_BLOCKED.into()));
    };
    if !receipt_holds(&newest, &entry.claim_id().0, ordinal) {
        return Err(AppError::InvalidState(UNDO_BLOCKED.into()));
    }
    if let Some(receipt) = claims_receipt(&newest) {
        for (id, _) in receipt.acted {
            ended.insert(ClaimId(id));
        }
    }
    edits
        .revert_in(
            tx,
            &RevertContentEditRequest {
                revert_id: Uuid::new_v4().to_string(),
                edit_id: newest.edit_id,
                expected_revision: newest.revision,
                and_later: false,
            },
        )
        .await
}

/// Undoes a batch of published text-changing claim actions per unit, newest edit first.
/// Refuses with `InvalidState(UNDO_BLOCKED)` if any requested statement cannot be reached
/// or if any other claim acted with a reverted edit is absent from `requested`.
pub(super) async fn revert_batch_in(
    tx: &mut WriteTx,
    edits: &ContentEdits,
    lesson_id: &str,
    targets: &[(&ClaimLedgerEntry, u32)],
) -> Result<Vec<Uncommitted>> {
    let mut by_unit: HashMap<usize, HashMap<String, u32>> = HashMap::new();
    let mut units = Vec::new();
    for &(entry, ordinal) in targets {
        let unit = action_unit(entry);
        let group = by_unit.entry(unit).or_insert_with(|| {
            units.push(unit);
            HashMap::new()
        });
        group.insert(entry.claim_id().0.clone(), ordinal);
    }

    let mut uncommitted = Vec::new();
    for unit in units {
        let content = keys::section_ref(lesson_id, unit);
        let Some(mut remaining) = by_unit.remove(&unit) else {
            continue;
        };
        while !remaining.is_empty() {
            let mut list = edits
                .newest(
                    &mut tx.reading(),
                    std::slice::from_ref(&content),
                    keys::CLAIMS_REACTOR,
                )
                .await?;
            let Some(newest) = list.pop() else {
                return Err(AppError::InvalidState(UNDO_BLOCKED.into()));
            };
            if !newest.unbroken || !matches!(newest.origin, EditOrigin::Action { .. }) {
                return Err(AppError::InvalidState(UNDO_BLOCKED.into()));
            }
            let Some(receipt) =
                claims_receipt(&newest).filter(|r| !r.revert && !r.acted.is_empty())
            else {
                return Err(AppError::InvalidState(UNDO_BLOCKED.into()));
            };
            let all_requested = receipt
                .acted
                .iter()
                .all(|(id, ord)| remaining.get(id) == Some(ord));
            if !all_requested {
                return Err(AppError::InvalidState(UNDO_BLOCKED.into()));
            }
            let pending = edits
                .revert_in(
                    tx,
                    &RevertContentEditRequest {
                        revert_id: Uuid::new_v4().to_string(),
                        edit_id: newest.edit_id,
                        expected_revision: newest.revision,
                        and_later: false,
                    },
                )
                .await?;
            uncommitted.push(pending);
            for (id, _) in receipt.acted {
                remaining.remove(&id);
            }
        }
    }
    Ok(uncommitted)
}
