//! The user's rewrite of a published section: which sentences changed, and
//! what each claim on the old text becomes.

use std::ops::Range;

use lattice_core::domain::learning::claim_review::{ClaimId, ClaimLedgerEntry, UnitKind};
use lattice_core::shared::text::segmentation::{common_subsequence, sentences};

use super::published_edit::{gone, placed};

/// A run of sentences that differ between the old and new text, as index
/// ranges into each text's sentences; either side may be empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Hunk {
    pub old: Range<usize>,
    pub new: Range<usize>,
}

/// The hunks between `old` and `new`, sentence by sentence.
pub(super) fn sentence_diff(old: &str, new: &str) -> Vec<Hunk> {
    hunks(
        &compared(old, &sentences(old)),
        &compared(new, &sentences(new)),
    )
}

/// The sentences' texts as they are compared: a line break is one character
/// whichever way the text was written.
fn compared(text: &str, spans: &[(usize, usize)]) -> Vec<String> {
    spans
        .iter()
        .map(|&(start, end)| {
            text.get(start..end)
                .unwrap_or_default()
                .replace("\r\n", "\n")
        })
        .collect()
}

fn hunks(old: &[String], new: &[String]) -> Vec<Hunk> {
    let ends = (old.len(), new.len());
    let (mut i, mut j) = (0, 0);
    let mut found = Vec::new();
    for (next_i, next_j) in common_subsequence(old, new)
        .into_iter()
        .chain(std::iter::once(ends))
    {
        if i < next_i || j < next_j {
            found.push(Hunk {
                old: i..next_i,
                new: j..next_j,
            });
        }
        i = next_i + 1;
        j = next_j + 1;
    }
    found
}

/// What a rewrite of one field of a section does to the claims on it.
#[derive(Debug, Default)]
pub(super) struct FieldPlan {
    pub kept: Vec<ClaimId>,
    /// Claims on changed sentences, each with the index into `written` of the
    /// sentence that takes its place.
    pub edited: Vec<(ClaimId, usize)>,
    /// Claims on sentences deleted with nothing in their place.
    pub removed: Vec<ClaimId>,
    /// Every sentence of the new text that the old text did not have.
    pub written: Vec<Written>,
}

/// A sentence the user wrote, and the claim that stands for it.
#[derive(Debug)]
pub(super) struct Written {
    pub text: String,
    pub span: (usize, usize),
    pub successor: ClaimId,
}

/// What rewriting `field` of section `unit` from `old` to `new` does to the
/// claims on it; claims of other sections or fields, and claims that already
/// ended, are left out.
pub(super) fn plan(
    claims: &[ClaimLedgerEntry],
    unit: usize,
    field: &str,
    old: &str,
    new: &str,
) -> FieldPlan {
    let (old_spans, new_spans) = (sentences(old), sentences(new));
    let found = hunks(&compared(old, &old_spans), &compared(new, &new_spans));
    let mut written = Vec::new();
    let mut first_written = Vec::new();
    let mut hunk_of_old: Vec<Option<usize>> = vec![None; old_spans.len()];
    for (at, hunk) in found.iter().enumerate() {
        first_written.push(written.len());
        for index in hunk.new.clone() {
            let Some(&(start, end)) = new_spans.get(index) else {
                continue;
            };
            written.push(Written {
                text: new.get(start..end).unwrap_or_default().to_owned(),
                span: (start, end),
                successor: ClaimId::generate(),
            });
        }
        for index in hunk.old.clone() {
            if let Some(slot) = hunk_of_old.get_mut(index) {
                *slot = Some(at);
            }
        }
    }
    let mut plan = FieldPlan::default();
    for entry in claims {
        let location = entry.location();
        if location.unit_kind != UnitKind::Block
            || location.unit_index != unit
            || location.field != field
            || gone(entry)
        {
            continue;
        }
        let id = entry.claim_id().clone();
        let hunk = placed(entry, old).and_then(|(start, end)| {
            old_spans
                .iter()
                .zip(&hunk_of_old)
                .filter(|(&(from, to), _)| from < end && start < to)
                .find_map(|(_, hunk)| *hunk)
        });
        match hunk.and_then(|at| found.get(at).map(|hunk| (at, hunk))) {
            None => plan.kept.push(id),
            Some((_, hunk)) if hunk.new.is_empty() => plan.removed.push(id),
            Some((at, _)) => plan
                .edited
                .push((id, first_written.get(at).copied().unwrap_or_default())),
        }
    }
    plan.written = written;
    plan
}
