//! No repair writes a new statement (acceptance design §2.6). Before a pass
//! records anything, each hunk a pending repair changed is weighed: every
//! statement extracted from its new text must continue a distinct one that
//! stood on its old sentences. A hunk that holds a statement beyond them is
//! put back and only the targets in it are charged, as `sections::unmend`
//! puts back a flow repair at the grain of a section.

use super::super::*;
use super::fields::{field_pointer, field_text};
use super::identify::{Extracted, Matched};
use super::{is_example, is_gone, ClaimLedger, RepairRecord};
use lattice_core::domain::learning::acceptance::AcceptancePolicy;
use lattice_core::domain::learning::claim_review::{span_of, ClaimId, ClaimLedgerEntry};
use lattice_core::shared::text::segmentation::{align, Span};
use lattice_core::shared::text::sentences;
use std::collections::{BTreeMap, BTreeSet};

/// What a pass's repairs wrote beyond the statements that stood before them:
/// by section and field, the hunks to put back, each its span in the lesson
/// before the repairs and in the draft; and each target charged for them, with
/// the statement its correction wrote.
#[derive(Default)]
struct Unwritten {
    hunks: BTreeMap<(usize, String), Vec<(Span, Span)>>,
    charged: Vec<(ClaimId, String)>,
}

/// A change to one field, as the alignment of its text before the repairs with
/// its text now gives it, and the repair targets whose sentences it touched.
struct Hunk {
    old: Span,
    new: Span,
    targets: Vec<ClaimId>,
}

/// Whether the byte range `span` overlaps `range`.
fn overlaps((start, end): (usize, usize), range: &Span) -> bool {
    start < range.end && end > range.start
}

/// Whether a claim standing at `span` of a field's old text is in the hunk that
/// replaced `old` there: they overlap, or, where the hunk took nothing out, the
/// claim stands on a sentence beside what it wrote. `spans` are the old text's
/// sentences.
fn touches(span: (usize, usize), old: &Span, spans: &[(usize, usize)]) -> bool {
    if !old.is_empty() {
        return overlaps(span, old);
    }
    let before = spans.iter().rev().find(|&&(_, end)| end <= old.start);
    let after = spans.iter().find(|&&(start, _)| start >= old.start);
    [before, after]
        .into_iter()
        .flatten()
        .any(|&(start, end)| overlaps(span, &(start..end)))
}

/// The hunks of `current` to put back (acceptance design §2.6). Each field
/// that holds a pending repair's target is aligned from `previous`, the lesson
/// before the repairs. A hunk with new text and a statement among its targets
/// is weighed: every statement of `extracted` standing on its new text must
/// continue, as `matched` has it, a distinct claim that stood on its old
/// sentences (a target, a statement riding on one, any other). A narrowing
/// target continues in nothing it wrote, and a statement that still continues
/// it is the narrowing's own outcome, not a take-back. A hunk that holds a
/// statement beyond them is put back, and with it every other hunk of each
/// target it charges. A hunk with no statement among its targets (an edit of
/// the user's), or that holds a claim the user edited, is not weighed.
fn unwritten(
    repairs: &[RepairRecord],
    entries: &[ClaimLedgerEntry],
    edited: &HashSet<ClaimId>,
    (previous, current): (&Value, &Value),
    (extracted, matched): (&[Extracted], &Matched),
) -> Unwritten {
    let entry = |id: &ClaimId| entries.iter().find(|claim| claim.claim_id() == id);
    let targets: Vec<&ClaimLedgerEntry> = repairs
        .iter()
        .filter(|record| record.rides_on.is_none())
        .filter_map(|record| entry(&record.claim_id))
        .collect();
    let fields: BTreeSet<(usize, String)> = targets
        .iter()
        .filter(|target| !is_example(target))
        .map(|target| {
            let at = target.location();
            (at.unit_index, at.field.clone())
        })
        .collect();
    let mut unwritten = Unwritten::default();
    for (unit, field) in fields {
        let (Some(before), Some(after)) = (
            field_text(previous, unit, &field),
            field_text(current, unit, &field),
        ) else {
            continue;
        };
        if before == after {
            continue;
        }
        let stood = |claim: &ClaimLedgerEntry| {
            let at = claim.location();
            (at.unit_index == unit && at.field == field)
                .then(|| span_of(&before, &at.quote).or(at.span))
                .flatten()
        };
        let spans = sentences(&before);
        let hunks: Vec<Hunk> = align(&before, &after)
            .hunks()
            .iter()
            .map(|(old, new)| Hunk {
                targets: targets
                    .iter()
                    .filter(|target| stood(target).is_some_and(|span| touches(span, old, &spans)))
                    .map(|target| target.claim_id().clone())
                    .collect(),
                old: old.clone(),
                new: new.clone(),
            })
            .collect();
        // The statement each weighed hunk wrote beyond those that stood on it.
        let mut wrote: Vec<Option<&str>> = hunks
            .iter()
            .map(|hunk| {
                let weighed = hunk
                    .targets
                    .iter()
                    .any(|id| entry(id).is_some_and(|target| !is_example(target)));
                if hunk.new.is_empty() || !weighed {
                    return None;
                }
                let held: Vec<&ClaimId> = entries
                    .iter()
                    .filter(|claim| !is_gone(claim.state()) || edited.contains(claim.claim_id()))
                    .filter(|claim| stood(claim).is_some_and(|span| overlaps(span, &hunk.old)))
                    .map(ClaimLedgerEntry::claim_id)
                    .collect();
                if held.iter().any(|id| edited.contains(*id)) {
                    return None;
                }
                extracted
                    .iter()
                    .enumerate()
                    .filter(|(_, statement)| statement.example.is_none())
                    .filter(|(_, statement)| {
                        let at = &statement.claim.location;
                        at.unit_index == unit
                            && at.field == field
                            && at.span.is_some_and(|span| overlaps(span, &hunk.new))
                    })
                    .find(|(index, _)| {
                        matched
                            .continues(*index)
                            .is_none_or(|id| !held.contains(&id))
                    })
                    .map(|(_, statement)| statement.claim.statement.as_str())
            })
            .collect();
        // A target charged for one hunk loses every change to its sentences.
        loop {
            let charged: Vec<(&ClaimId, &str)> = hunks
                .iter()
                .zip(&wrote)
                .filter_map(|(hunk, statement)| Some((hunk, (*statement)?)))
                .flat_map(|(hunk, statement)| hunk.targets.iter().map(move |id| (id, statement)))
                .collect();
            let mut grew = false;
            for (hunk, statement) in hunks.iter().zip(wrote.iter_mut()) {
                let charge = charged.iter().find(|(id, _)| hunk.targets.contains(*id));
                if let (None, Some((_, written))) = (*statement, charge) {
                    *statement = Some(*written);
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
        let back: Vec<(Span, Span)> = hunks
            .iter()
            .zip(&wrote)
            .filter(|(_, statement)| statement.is_some())
            .map(|(hunk, _)| (hunk.old.clone(), hunk.new.clone()))
            .collect();
        if back.is_empty() {
            continue;
        }
        for (hunk, statement) in hunks.iter().zip(&wrote) {
            let Some(statement) = statement else {
                continue;
            };
            for id in &hunk.targets {
                if !unwritten.charged.iter().any(|(charged, _)| charged == id) {
                    unwritten
                        .charged
                        .push((id.clone(), (*statement).to_owned()));
                }
            }
        }
        unwritten.hunks.insert((unit, field), back);
    }
    unwritten
}

/// The bytes of `text` a hunk's `region` takes when it is put back or taken
/// out: its sentences and the separator after them, up to the next sentence.
/// At the end of the text, a hunk one side of which is empty (`alone`) takes
/// the separator before it instead, so the text keeps no space it lacked.
fn cell(text: &str, region: &Span, alone: bool) -> Span {
    let spans = sentences(text);
    if let Some(&(next, _)) = spans.iter().find(|&&(start, _)| start >= region.end) {
        return region.start..next;
    }
    if !alone {
        return region.start..text.len();
    }
    let before = spans
        .iter()
        .rev()
        .find(|&&(_, end)| end <= region.start)
        .map_or(0, |&(_, end)| end);
    before..text.len()
}

/// `current`, a field's text, with `hunks` of its alignment to `previous`, its
/// text before the repairs, put back: each takes its sentences and the
/// separators beside them from `previous` again. When every hunk goes back the
/// field is `previous` itself.
fn put_back(previous: &str, current: &str, hunks: &[(Span, Span)]) -> String {
    if align(previous, current).hunks().len() == hunks.len() {
        return previous.to_owned();
    }
    let mut cells: Vec<(Span, Span)> = hunks
        .iter()
        .map(|(old, new)| {
            let alone = old.is_empty() || new.is_empty();
            (cell(previous, old, alone), cell(current, new, alone))
        })
        .collect();
    cells.sort_by_key(|(_, new)| std::cmp::Reverse(new.start));
    let mut text = current.to_owned();
    for (old, new) in cells {
        let (Some(head), Some(was), Some(tail)) = (
            text.get(..new.start),
            previous.get(old),
            text.get(new.end..),
        ) else {
            continue;
        };
        text = format!("{head}{was}{tail}");
    }
    text
}

/// Why a correction that wrote `statement`, which its sentence did not make,
/// is not applied: what its retry is told.
fn not_made(statement: &str) -> String {
    format!("The correction wrote a statement its sentence did not make (\"{statement}\"), so it was put back; correct only what was asked and add nothing.")
}

impl ClaimLedger {
    /// Before a pass records anything, the hunks of `candidate` a pending
    /// repair changed that wrote a statement none of those standing on their
    /// old sentences continues (`unwritten`) are put back, as `sections::unmend`
    /// puts back a flow repair; `claims` and `executions` are what the pass
    /// extracted. The draft with those hunks as they were is saved first. Then
    /// only the targets in them are charged: a rewrite is not applied, told why,
    /// and goes to its retry or its cut; a narrowing is spent; a failed example
    /// uses its attempt; and the statements riding on them lose their records,
    /// their sentence not rewritten after all. `Some` is the draft.
    pub(in super::super) async fn unwrite(
        &mut self,
        policy: &dyn AcceptancePolicy,
        candidate: &Value,
        (claims, executions): (&[(usize, Claim)], &[execution::Observation]),
    ) -> Result<Option<String>> {
        let Some(previous) = self.pending.previous.clone() else {
            return Ok(None);
        };
        if self.pending.repairs.is_empty() {
            return Ok(None);
        }
        let extracted = self.extracted(candidate, claims, executions);
        let matched = self.match_pass(candidate, &extracted);
        let unwritten = unwritten(
            &self.pending.repairs,
            &self.entries,
            &self.edited_now,
            (&previous, candidate),
            (extracted.as_slice(), &matched),
        );
        if unwritten.charged.is_empty() {
            return Ok(None);
        }
        tracing::info!(
            targets = unwritten.charged.len(),
            "claim.state: a correction wrote a statement its sentence did not make; its change is put back"
        );
        let mut restored = candidate.clone();
        for ((unit, field), hunks) in &unwritten.hunks {
            let (Some(before), Some(after), Some(pointer)) = (
                field_text(&previous, *unit, field),
                field_text(candidate, *unit, field),
                field_pointer(candidate, *unit, field),
            ) else {
                continue;
            };
            if let Some(slot) = restored.pointer_mut(&pointer) {
                *slot = Value::String(put_back(&before, &after, hunks));
            }
        }
        let raw = restored.to_string();
        crate::lesson_drafts::save(&raw).await?;
        for (id, statement) in &unwritten.charged {
            self.reject(policy, std::slice::from_ref(id), &not_made(statement))
                .await?;
        }
        Ok(Some(raw))
    }
}
