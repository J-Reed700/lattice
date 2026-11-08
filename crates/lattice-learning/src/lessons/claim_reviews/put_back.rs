//! Where a sentence the machine took out goes back, and how it is written into
//! or out of a field's text. Pure: the lesson comes in as the units a review
//! card reads (`blocks`, then `questions`), so a draft and a published lesson
//! are placed by the same rules.

use lattice_core::domain::learning::acceptance::Outcome;
use lattice_core::domain::learning::claim_review::{
    span_of, Anchor, ClaimActionKind, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimState, CutPlace,
    EventProvenance, PutPlace, UserAction,
};
use lattice_core::shared::error::{AppError, Result};
use serde_json::Value;
use sqlx::SqliteConnection;
use std::collections::HashSet;

use super::repository::{append_in, load_in};

use crate::content_verification::{
    cuts_for, field_pointer, field_text, insert_sentence, located_quote, put_place, put_sentence,
    Placing,
};

pub(super) const CHOOSE: &str = "Choose the section to put it back in.";
pub(super) const NO_SECTION: &str = "That section is not in the lesson.";

/// The lesson's text a sentence is placed in. A published lesson offers only
/// its sections' bodies.
pub(super) struct Lesson<'a> {
    pub units: &'a Value,
    pub published: bool,
}

impl Lesson<'_> {
    fn blocks(&self) -> usize {
        self.units
            .get("blocks")
            .and_then(Value::as_array)
            .map_or(0, Vec::len)
    }

    fn count(&self) -> usize {
        let questions = self
            .units
            .get("questions")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        self.blocks() + questions
    }

    fn text(&self, unit: usize, field: &str) -> Option<String> {
        if self.published && (unit >= self.blocks() || field != "body") {
            return None;
        }
        field_text(self.units, unit, field)
    }

    /// The one unit whose `field` holds `text`, once.
    fn unit_holding(&self, field: &str, text: &str) -> Option<usize> {
        let mut holding = (0..self.count()).filter(|unit| {
            self.text(*unit, field)
                .is_some_and(|body| body.contains(text))
        });
        let unit = holding.next().filter(|_| holding.next().is_none())?;
        span_of(&self.text(unit, field)?, text).map(|_| unit)
    }

    /// The sections of the lesson, by index and title.
    pub(super) fn sections(&self) -> Vec<(usize, String)> {
        self.units
            .get("blocks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(index, block)| {
                let title = block.get("title").and_then(Value::as_str);
                (index, title.unwrap_or_default().to_owned())
            })
            .collect()
    }
}

/// The sentence that goes back: what the machine cut when it recorded it,
/// else the sentence the claim was extracted from.
pub(super) fn sentence_of(entry: &ClaimLedgerEntry) -> String {
    entry.cut_place().map_or_else(
        || entry.location().quote.clone(),
        |cut| cut.sentence.clone(),
    )
}

fn original_unit(entry: &ClaimLedgerEntry) -> Option<usize> {
    entry
        .events()
        .iter()
        .find_map(|record| match &record.event {
            ClaimEvent::Extracted { location, .. } => Some(location.unit_index),
            _ => None,
        })
}

/// Whether a section went out of the lesson: a cut that could not take a
/// sentence took its section, or a claim was moved to another unit when the
/// sections after one that went were renumbered.
fn section_went_out(claims: &[ClaimLedgerEntry]) -> bool {
    claims.iter().any(|entry| {
        let mut units = entry
            .events()
            .iter()
            .filter_map(|record| match &record.event {
                ClaimEvent::Extracted { location, .. } => Some(location.unit_index),
                _ => None,
            });
        let first = units.next();
        let moved = units.any(|unit| Some(unit) != first);
        moved
            || entry.events().iter().any(|record| {
                matches!(
                    record.event,
                    ClaimEvent::Stepped {
                        outcome: Outcome::CannotCut,
                        ..
                    }
                )
            })
    })
}

fn gone(entry: &ClaimLedgerEntry) -> bool {
    matches!(
        entry.state(),
        ClaimState::Removed { .. } | ClaimState::Edited { .. }
    )
}

/// The unit the claim's sentence is in now, found through the claims still
/// standing that were extracted from the same unit it was: both are numbered
/// as the lesson was extracted. `None` when none survived or they disagree.
fn surviving_unit(claims: &[ClaimLedgerEntry], entry: &ClaimLedgerEntry) -> Option<usize> {
    let original = original_unit(entry)?;
    let mut units = claims
        .iter()
        .filter(|other| {
            other.claim_id() != entry.claim_id()
                && !gone(other)
                && other.location().unit_kind == entry.location().unit_kind
                && original_unit(other) == Some(original)
        })
        .map(|other| other.location().unit_index);
    let unit = units.next()?;
    units.all(|other| other == unit).then_some(unit)
}

/// Every cut the machine recorded in the section's `field`, the claim's own
/// included.
fn cuts_with(
    claims: &[ClaimLedgerEntry],
    entry: &ClaimLedgerEntry,
    unit_index: usize,
    field: &str,
) -> Vec<CutPlace> {
    cuts_for(
        claims.iter().chain(std::iter::once(entry)),
        entry,
        unit_index,
        field,
    )
}

/// Where the claim's taken-out sentence goes back, when the ledger and the
/// text say: where the cuts' records put it among the sentences that are
/// still in the text, else beside the claims around it, else the end of its
/// field. `None` when the place cannot be known, or the text holds a
/// sentence the records do not account for where it stood.
pub(super) fn place(
    lesson: &Lesson<'_>,
    claims: &[ClaimLedgerEntry],
    entry: &ClaimLedgerEntry,
) -> Option<PutPlace> {
    let field = entry.location().field.clone();
    if let Some(cut) = entry.cut_place() {
        let unit = [cut.before.as_ref(), cut.after.as_ref()]
            .into_iter()
            .flatten()
            .find_map(|text| lesson.unit_holding(&field, text));
        if let Some(unit_index) = unit {
            let text = lesson.text(unit_index, &field)?;
            let cuts = cuts_with(claims, entry, unit_index, &field);
            let refs: Vec<&CutPlace> = cuts.iter().collect();
            match put_place(&text, &cut.sentence, &refs) {
                Placing::At(anchor) => {
                    return Some(PutPlace {
                        unit_index,
                        field,
                        anchor,
                    })
                }
                Placing::Ambiguous => return None,
                Placing::Unknown => {}
            }
        }
    }
    let unit_index = if section_went_out(claims) {
        surviving_unit(claims, entry)?
    } else {
        entry.location().unit_index
    };
    let text = lesson.text(unit_index, &field)?;
    let anchor = beside_claims(claims, entry, unit_index, &field, &text);
    Some(PutPlace {
        unit_index,
        field,
        anchor,
    })
}

/// After the nearest earlier claim of the unit and field whose sentence is
/// still in `text`, else before the nearest later one, else the end; by the
/// spans the claims recorded.
fn beside_claims(
    claims: &[ClaimLedgerEntry],
    entry: &ClaimLedgerEntry,
    unit: usize,
    field: &str,
    text: &str,
) -> Anchor {
    let Some((start, _)) = entry.location().span else {
        return Anchor::End;
    };
    let mut earlier: Option<(usize, String)> = None;
    let mut later: Option<(usize, String)> = None;
    for other in claims.iter().filter(|other| {
        other.claim_id() != entry.claim_id()
            && other.location().unit_index == unit
            && other.location().field == field
    }) {
        let Some((at, _)) = other.location().span else {
            continue;
        };
        let sentence = located_quote(other.location(), other.statement(), text);
        if span_of(text, &sentence).is_none() {
            continue;
        }
        let nearest = if at < start { &mut earlier } else { &mut later };
        let closer = match (&*nearest, at < start) {
            (None, _) => true,
            (Some((held, _)), true) => at > *held,
            (Some((held, _)), false) => at < *held,
        };
        if closer {
            *nearest = Some((at, sentence));
        }
    }
    match (earlier, later) {
        (Some((_, sentence)), _) => Anchor::After(sentence),
        (None, Some((_, sentence))) => Anchor::Before(sentence),
        (None, None) => Anchor::End,
    }
}

/// The end of section `section`'s text.
pub(super) fn end_of_section(lesson: &Lesson<'_>, section: u32) -> Result<PutPlace> {
    let unit_index = usize::try_from(section).unwrap_or(usize::MAX);
    if unit_index >= lesson.blocks() || lesson.text(unit_index, "body").is_none() {
        return Err(AppError::InvalidInput(NO_SECTION.into()));
    }
    Ok(PutPlace {
        unit_index,
        field: "body".into(),
        anchor: Anchor::End,
    })
}

/// The place for `entry`'s sentence: the end of the section the user chose,
/// else where it was; refused when neither is known.
pub(super) fn resolve(
    lesson: &Lesson<'_>,
    claims: &[ClaimLedgerEntry],
    entry: &ClaimLedgerEntry,
    section: Option<u32>,
) -> Result<PutPlace> {
    match section {
        Some(section) => end_of_section(lesson, section),
        None => place(lesson, claims, entry).ok_or_else(|| AppError::InvalidInput(CHOOSE.into())),
    }
}

/// The other claims the machine took out with `entry`'s sentence: the same
/// field of the same unit, the same sentence. Putting the sentence back
/// brings every one of them back.
pub(super) fn siblings<'a>(
    claims: &'a [ClaimLedgerEntry],
    entry: &ClaimLedgerEntry,
) -> Vec<&'a ClaimLedgerEntry> {
    let sentence = sentence_of(entry);
    claims
        .iter()
        .filter(|other| {
            other.claim_id() != entry.claim_id()
                && other.allows(ClaimActionKind::PutBack)
                && other.location().unit_index == entry.location().unit_index
                && other.location().field == entry.location().field
                && sentence_of(other) == sentence
        })
        .collect()
}

/// Writes the user's put back `event` for `claim_id`, and for the claims
/// taken out with its sentence, into a draft's ledger: the next pass writes
/// the text, once. The claims it put back are the ones it returns.
pub(super) async fn write_draft_in(
    connection: &mut SqliteConnection,
    lesson_id: &str,
    claim_id: &ClaimId,
    event: &ClaimEvent,
    user: &EventProvenance,
) -> Result<HashSet<ClaimId>> {
    let ledger = load_in(connection, lesson_id).await?;
    let mut put: HashSet<ClaimId> = HashSet::from([claim_id.clone()]);
    if let Some(entry) = ledger
        .claims
        .iter()
        .find(|entry| entry.claim_id() == claim_id)
    {
        put.extend(
            siblings(&ledger.claims, entry)
                .into_iter()
                .map(|other| other.claim_id().clone()),
        );
    }
    let mut ordered: Vec<&ClaimId> = put.iter().collect();
    ordered.sort();
    for id in ordered {
        append_in(connection, lesson_id, id, user.clone(), event.clone()).await?;
    }
    Ok(put)
}

/// `units` with the put back `event` written in, as the commit (a published
/// lesson) or the next pass (a draft) writes it: what the put backs after it
/// in a batch are placed against.
pub(super) fn stage(
    units: &mut Value,
    claims: &[ClaimLedgerEntry],
    entry: &ClaimLedgerEntry,
    event: &ClaimEvent,
    published: bool,
) {
    let ClaimEvent::UserAction {
        action: UserAction::PutBack {
            sentence, place, ..
        },
        ..
    } = event
    else {
        return;
    };
    let Some(pointer) = field_pointer(units, place.unit_index, &place.field) else {
        return;
    };
    let Some(text) = units.pointer(&pointer).and_then(Value::as_str) else {
        return;
    };
    let put = if published {
        insert_sentence(text, &place.anchor, sentence)
    } else {
        let cuts = cuts_with(claims, entry, place.unit_index, &place.field);
        put_sentence(text, sentence, &cuts, &place.anchor)
    };
    if let (Some(put), Some(slot)) = (put, units.pointer_mut(&pointer)) {
        *slot = Value::String(put);
    }
}
