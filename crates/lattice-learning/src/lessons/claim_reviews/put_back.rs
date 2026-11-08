//! Where a sentence the machine took out goes back, and how it is written into
//! or out of a field's text. Pure: the lesson comes in as the units a review
//! card reads (`blocks`, then `questions`), so a draft and a published lesson
//! are placed by the same rules.

use lattice_core::domain::learning::acceptance::Outcome;
use lattice_core::domain::learning::claim_review::{
    span_of, Anchor, ClaimEvent, ClaimLedgerEntry, ClaimState, PutPlace,
};
use lattice_core::shared::error::{AppError, Result};
use serde_json::Value;

use crate::content_verification::{field_text, located_quote};

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

/// Where the claim's taken-out sentence goes back, when the ledger and the
/// text say: beside a recorded neighbour that is still in the text, else
/// beside the claims around it, else the end of its field. `None` when the
/// place cannot be known.
pub(super) fn place(
    lesson: &Lesson<'_>,
    claims: &[ClaimLedgerEntry],
    entry: &ClaimLedgerEntry,
) -> Option<PutPlace> {
    let field = entry.location().field.clone();
    if let Some(cut) = entry.cut_place() {
        let neighbours = [
            cut.before.as_ref().map(|text| (text, true)),
            cut.after.as_ref().map(|text| (text, false)),
        ];
        for (text, after) in neighbours.into_iter().flatten() {
            if let Some(unit_index) = lesson.unit_holding(&field, text) {
                let anchor = if after {
                    Anchor::After(text.clone())
                } else {
                    Anchor::Before(text.clone())
                };
                return Some(PutPlace {
                    unit_index,
                    field,
                    anchor,
                });
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
