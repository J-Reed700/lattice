//! Cutting forgotten spans out of source text before it is summarised.
//!
//! A user's forget records the item's assertion spans as suppressions and
//! clears the working summary. The summary is then rebuilt from the original
//! messages — by a refold, or by a full rebuild — so without this the
//! forgotten value would come back in the summary's prose.

use std::borrow::Cow;

use super::compute_digest;

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
