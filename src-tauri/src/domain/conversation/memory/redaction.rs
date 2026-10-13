//! Keeping what a user forgot out of memory that is rebuilt from source.
//!
//! A user's forget records the item's assertion spans as suppressions and
//! clears the working summary. The summary is then rebuilt from the original
//! messages — by a refold, or by a full rebuild — and a rebuild extracts items
//! from those messages again. Without this the forgotten value would come back
//! in the summary's prose, and as a fresh item quoting the same words.

use std::borrow::Cow;
use std::collections::HashSet;

use super::{
    compute_digest, EvidencePurpose, EvidenceSpan, ValidatedPatch, ValidatedTransitionTarget,
};

/// What stands in for a forgotten span, so the sentence around it still reads.
pub const FORGOTTEN_MARKER: &str = "[forgotten]";

/// A byte range of one message the user asked memory to forget, with the
/// digest the message had when the range was recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuppressedSpan {
    pub message_id: String,
    pub start_byte: i64,
    pub end_byte: i64,
    pub content_digest: String,
}

impl SuppressedSpan {
    /// Whether `evidence` asserts something inside this span: the same message,
    /// unchanged since the forget, with overlapping bytes. The rule
    /// `trg_memory_suppression_evidence` applies when such an item is stored.
    pub fn covers(&self, evidence: &EvidenceSpan) -> bool {
        evidence.purpose == EvidencePurpose::Assertion
            && evidence.message_id == self.message_id
            && evidence.content_digest == self.content_digest
            && self.start_byte < i64::from(evidence.end_byte)
            && self.end_byte > i64::from(evidence.start_byte)
    }
}

/// Take out of `patch` every addition that asserts a forgotten span, with each
/// transition that names one, and return how many additions went.
///
/// The store marks such an item forgotten as it lands, but until then the run
/// would hold it as active memory: reviewed, and named to the summariser among
/// the active requirements. A transition that names a dropped addition as its
/// replacement goes too, so the item it would have retired stays as it was.
pub fn drop_forgotten_additions(
    patch: &mut ValidatedPatch,
    suppressed: &[SuppressedSpan],
) -> usize {
    if suppressed.is_empty() {
        return 0;
    }
    let forgotten: HashSet<String> = patch
        .additions
        .iter()
        .filter(|addition| {
            addition
                .evidence
                .iter()
                .any(|evidence| suppressed.iter().any(|span| span.covers(evidence)))
        })
        .map(|addition| addition.candidate_id.clone())
        .collect();
    if forgotten.is_empty() {
        return 0;
    }
    patch
        .additions
        .retain(|addition| !forgotten.contains(&addition.candidate_id));
    patch.transitions.retain(|transition| {
        let replaced_by_forgotten = transition
            .replacement_candidate_id
            .as_ref()
            .is_some_and(|candidate| forgotten.contains(candidate));
        let targets_forgotten = matches!(
            &transition.target,
            ValidatedTransitionTarget::Candidate(candidate) if forgotten.contains(candidate)
        );
        !replaced_by_forgotten && !targets_forgotten
    });
    forgotten.len()
}

/// `content` with each applicable suppressed range replaced by
/// [`FORGOTTEN_MARKER`].
///
/// A suppression applies only while `content` still hashes to its digest, as
/// an evidence span resolves only then: its offsets mean nothing against other
/// text. Ranges are clamped to the content, widened outward to character
/// boundaries (a partly covered character is removed, never kept), and merged
/// where they overlap or touch, so neighbouring spans read as one marker.
/// Never panics.
pub fn redact_suppressed<'a>(content: &'a str, spans: &[&SuppressedSpan]) -> Cow<'a, str> {
    if spans.is_empty() {
        return Cow::Borrowed(content);
    }
    let digest = compute_digest(content);
    let len = content.len();
    let mut ranges: Vec<(usize, usize)> = spans
        .iter()
        .filter(|span| span.content_digest == digest)
        .filter_map(|span| {
            let start = clamp_offset(span.start_byte, len);
            let end = clamp_offset(span.end_byte, len);
            (start < end).then(|| (floor_boundary(content, start), ceil_boundary(content, end)))
        })
        .collect();
    if ranges.is_empty() {
        return Cow::Borrowed(content);
    }
    ranges.sort_unstable();

    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }

    let mut redacted = String::with_capacity(len);
    let mut cursor = 0usize;
    for (start, end) in merged {
        redacted.push_str(content.get(cursor..start).unwrap_or_default());
        redacted.push_str(FORGOTTEN_MARKER);
        cursor = end;
    }
    redacted.push_str(content.get(cursor..).unwrap_or_default());
    Cow::Owned(redacted)
}

/// A stored offset as an index into `len` bytes. Negative reads as zero and
/// past-the-end as the end, so a damaged row narrows rather than fails.
fn clamp_offset(offset: i64, len: usize) -> usize {
    usize::try_from(offset.max(0)).map_or(len, |offset| offset.min(len))
}

/// The nearest character boundary at or before `index`.
fn floor_boundary(content: &str, index: usize) -> usize {
    (0..=index)
        .rev()
        .find(|&at| content.is_char_boundary(at))
        .unwrap_or(0)
}

/// The nearest character boundary at or after `index`.
fn ceil_boundary(content: &str, index: usize) -> usize {
    (index..=content.len())
        .find(|&at| content.is_char_boundary(at))
        .unwrap_or(content.len())
}
