//! Ending and recording replaced/removed claims for a unit rewrite.

use lattice_core::domain::learning::claim_review::{
    Actor, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimLocation, EventProvenance, UnitKind,
    UserAction,
};
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::WriteTx;

use super::{user_resolution, LabelSource, Planned};
use crate::lessons::claim_reviews::published_edit::{confirmed_sentence_in, placed};
use crate::lessons::claim_reviews::published_rewrite::{FieldPlan, Written};
use crate::lessons::claim_reviews::repository::append_in;

/// A statement that ends with the rewrite, and what takes its place.
pub(in crate::lessons::claim_reviews) struct Ending<'a> {
    pub entry: &'a ClaimLedgerEntry,
    /// The statement's sentence as its field wrote it.
    pub before: String,
    /// The sentence that replaces it and its claim; none when it was deleted.
    pub after: Option<(ClaimId, String)>,
}

/// What `record_edit` wrote and ended for the claims reactor's receipt and settlement.
pub(in crate::lessons::claim_reviews) struct EditRecord<'a> {
    pub planned: Vec<Planned<'a>>,
    pub acted: Vec<(String, u32)>,
    pub created: Vec<String>,
}

pub(in crate::lessons::claim_reviews) fn endings<'a>(
    claims: &'a [ClaimLedgerEntry],
    plan: &FieldPlan,
    old: &str,
) -> Vec<Ending<'a>> {
    let find = |id: &ClaimId| claims.iter().find(|entry| entry.claim_id() == id);
    let sentence = |entry: &ClaimLedgerEntry| {
        placed(entry, old)
            .and_then(|(start, end)| old.get(start..end))
            .map_or_else(|| entry.location().quote.clone(), str::to_owned)
    };
    let edited = plan.edited.iter().filter_map(|(id, at)| {
        let entry = find(id)?;
        let written = plan.written.get(*at)?;
        Some(Ending {
            entry,
            before: sentence(entry),
            after: Some((written.successor.clone(), written.text.clone())),
        })
    });
    let removed = plan.removed.iter().filter_map(|id| {
        let entry = find(id)?;
        Some(Ending {
            entry,
            before: sentence(entry),
            after: None,
        })
    });
    edited.chain(removed).collect()
}

/// Writes the events of an edit inside its transaction: each sentence the user
/// wrote as a statement they confirmed, and for each statement that ends the
/// edit or removal it stood under. Returns what to settle plus acted/created IDs.
pub(in crate::lessons::claim_reviews) async fn record_edit<'a>(
    tx: &mut WriteTx,
    lesson_id: &str,
    unit: usize,
    written: &[(&str, UnitKind, &Written)],
    ending: &[Ending<'a>],
    user: &EventProvenance,
) -> Result<EditRecord<'a>> {
    let mut created = Vec::with_capacity(written.len());
    for (field, kind, item) in written {
        let location = ClaimLocation {
            unit_index: unit,
            unit_kind: *kind,
            field: (*field).into(),
            quote: item.text.clone(),
            span: Some(item.span),
        };
        confirmed_sentence_in(tx, lesson_id, &item.successor, location, user).await?;
        created.push(item.successor.0.clone());
    }
    let machine = user.as_machine();
    let mut planned = Vec::with_capacity(ending.len());
    let mut acted = Vec::with_capacity(ending.len());
    for item in ending {
        let (action, label) = match &item.after {
            Some((_, text)) => (
                UserAction::Edit {
                    before: item.before.clone(),
                    after: text.clone(),
                },
                LabelSource::UserEdit,
            ),
            None => (UserAction::Remove, LabelSource::UserRemove),
        };
        let resolution = user_resolution(action, None, Some(label));
        append_in(
            tx,
            lesson_id,
            item.entry.claim_id(),
            machine.clone(),
            ClaimEvent::SectionEdited {},
        )
        .await?;
        let ordinal = append_in(
            tx,
            lesson_id,
            item.entry.claim_id(),
            user.clone(),
            resolution.event.clone(),
        )
        .await?;
        acted.push((item.entry.claim_id().0.clone(), ordinal));
        if let Some((successor, _)) = &item.after {
            let superseded = ClaimEvent::Superseded {
                successor: Some(successor.clone()),
                by: Actor::User,
            };
            append_in(
                tx,
                lesson_id,
                item.entry.claim_id(),
                machine.clone(),
                superseded,
            )
            .await?;
        }
        planned.push(Planned {
            entry: item.entry,
            resolution,
        });
    }
    Ok(EditRecord {
        planned,
        acted,
        created,
    })
}
