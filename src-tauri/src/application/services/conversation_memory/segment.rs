//! Splitting source messages into extraction segments (§7.4).
//!
//! Design: `docs/design/2026-09-19-conversation-memory.md` §7.4.
//!
//! Two properties this module owes the rest of the job:
//!
//! 1. **Coverage.** The primary text of a message's segments, concatenated in
//!    order, is exactly the message content. Every byte belongs to exactly one
//!    primary segment, so "all segments accepted" really means "the whole
//!    message was submitted". Overlap is carried separately, as context.
//! 2. **Exactness.** A segment's text is a byte slice of the original at UTF-8
//!    boundaries. Nothing is normalised, trimmed, or re-wrapped, because a quote
//!    is later matched against the stored message, not against what the model
//!    was shown.
//!
//! The raw stored message is never modified. Segmentation exists only to bound
//! one model call.

use crate::domain::conversation_memory::{SourceMessage, SourceRole};

/// Primary bytes in one segment. A message at or under this is sent whole,
/// which is the case that matters: splitting a message is what puts a negation
/// and its subject in different requests.
pub const SEGMENT_MAX_BYTES: usize = 6 * 1024;

/// Unicode scalar characters of context carried either side of a cut (§7.4).
pub const SEGMENT_OVERLAP_CHARS: usize = 256;

/// Segments in one extraction request.
pub const MAX_SEGMENTS_PER_BATCH: usize = 16;

/// Primary bytes in one extraction request. Well inside a small utility model's
/// window once the instructions, existing items and overlap are added.
pub const MAX_BATCH_BYTES: usize = 24 * 1024;

/// Shortest primary segment a delimiter search may produce, as a fraction of
/// the maximum. Without a floor, a paragraph break near the start of the window
/// yields a one-line segment and the remaining text is re-scanned needlessly.
const MIN_SEGMENT_DIVISOR: usize = 4;

/// One unit of extraction input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSegment {
    /// `{message_id}:s{index}`. The model echoes these back so a truncated
    /// response is detectable (§6.2 rule 12).
    pub id: String,
    pub message_id: String,
    pub sequence: i64,
    pub role: SourceRole,
    /// Inclusive start of the primary text, relative to the message.
    pub start_byte: usize,
    /// Exclusive end of the primary text, relative to the message.
    pub end_byte: usize,
    /// Exact primary text.
    pub text: String,
    /// Characters immediately before `start_byte`. Context, not coverage.
    pub leading_overlap: String,
    /// Characters immediately after `end_byte`. Context, not coverage.
    pub trailing_overlap: String,
    /// True when this segment is the entire message.
    pub whole_message: bool,
}

impl SourceSegment {
    pub fn byte_len(&self) -> usize {
        self.end_byte.saturating_sub(self.start_byte)
    }
}

/// Split one message into segments that tile its content exactly.
///
/// Cuts prefer a paragraph break, then a line break, then a sentence end, and
/// fall back to the last UTF-8 boundary inside the window. A code block or a
/// single enormous line therefore still gets cut, but never mid-character.
pub fn segment_message(
    message: &SourceMessage,
    max_bytes: usize,
    overlap_chars: usize,
) -> Vec<SourceSegment> {
    let content = message.content.as_str();
    let max_bytes = max_bytes.max(1);

    if content.len() <= max_bytes {
        return vec![SourceSegment {
            id: format!("{}:s0", message.id),
            message_id: message.id.clone(),
            sequence: message.sequence,
            role: message.role,
            start_byte: 0,
            end_byte: content.len(),
            text: content.to_string(),
            leading_overlap: String::new(),
            trailing_overlap: String::new(),
            whole_message: true,
        }];
    }

    let mut segments = Vec::new();
    let mut start = 0usize;
    while start < content.len() {
        let end = next_cut(content, start, max_bytes);
        let Some(text) = content.get(start..end) else {
            // Unreachable while `next_cut` returns boundaries; stopping is the
            // only safe response, because emitting a shorter tile would claim
            // coverage of bytes nobody read.
            break;
        };
        let leading = content
            .get(..start)
            .map(|before| tail_chars(before, overlap_chars))
            .unwrap_or_default();
        let trailing = content
            .get(end..)
            .map(|after| head_chars(after, overlap_chars))
            .unwrap_or_default();
        segments.push(SourceSegment {
            id: format!("{}:s{}", message.id, segments.len()),
            message_id: message.id.clone(),
            sequence: message.sequence,
            role: message.role,
            start_byte: start,
            end_byte: end,
            text: text.to_string(),
            leading_overlap: leading.to_string(),
            trailing_overlap: trailing.to_string(),
            whole_message: false,
        });
        start = end;
    }
    segments
}

/// Segment a whole run of messages, in sequence order.
pub fn segment_messages(
    messages: &[SourceMessage],
    max_bytes: usize,
    overlap_chars: usize,
) -> Vec<SourceSegment> {
    messages
        .iter()
        .flat_map(|message| segment_message(message, max_bytes, overlap_chars))
        .collect()
}

/// Group segments into model requests, preserving order.
///
/// A message's segments stay together unless the message alone exceeds
/// `max_bytes`, so the usual case keeps a whole message in one request and the
/// giant-paste case still makes progress.
pub fn plan_batches(
    segments: Vec<SourceSegment>,
    max_segments: usize,
    max_bytes: usize,
) -> Vec<Vec<SourceSegment>> {
    let max_segments = max_segments.max(1);
    let max_bytes = max_bytes.max(1);
    let mut batches: Vec<Vec<SourceSegment>> = Vec::new();
    let mut current: Vec<SourceSegment> = Vec::new();
    let mut bytes = 0usize;

    for segment in segments {
        let size = segment.byte_len();
        let full = current.len() >= max_segments
            || (!current.is_empty() && bytes.saturating_add(size) > max_bytes);
        if full {
            batches.push(std::mem::take(&mut current));
            bytes = 0;
        }
        bytes = bytes.saturating_add(size);
        current.push(segment);
    }
    if !current.is_empty() {
        batches.push(current);
    }
    batches
}

/// End offset of the next primary segment starting at `start`.
///
/// Always returns a UTF-8 boundary strictly greater than `start`, so the tiling
/// loop makes progress and no tile is ever dropped for being unsliceable.
fn next_cut(content: &str, start: usize, max_bytes: usize) -> usize {
    let mut hard = start.saturating_add(max_bytes);
    if hard >= content.len() {
        return content.len();
    }
    while hard > start && !content.is_char_boundary(hard) {
        hard -= 1;
    }
    if hard <= start {
        // One character is wider than the whole budget. Overshooting by that
        // character is the only alternative to emitting an empty segment.
        let mut forward = start.saturating_add(max_bytes).saturating_add(1);
        while forward < content.len() && !content.is_char_boundary(forward) {
            forward += 1;
        }
        return forward.min(content.len());
    }
    let Some(window) = content.get(start..hard) else {
        return hard;
    };
    let floor = (max_bytes / MIN_SEGMENT_DIVISOR).min(window.len());

    // Paragraph, then line, then sentence. Each candidate is the offset *after*
    // the delimiter, so the delimiter stays with the text it terminates.
    for delimiter in ["\n\n", "\n"] {
        if let Some(found) = window.rfind(delimiter) {
            let cut = found + delimiter.len();
            if cut >= floor && cut > 0 {
                return start + cut;
            }
        }
    }
    for delimiter in [". ", ".\n", "! ", "? "] {
        if let Some(found) = window.rfind(delimiter) {
            let cut = found + delimiter.len();
            if cut >= floor && cut > 0 {
                return start + cut;
            }
        }
    }
    hard
}

/// Last `count` characters of `text`.
fn tail_chars(text: &str, count: usize) -> &str {
    if count == 0 {
        return "";
    }
    match text.char_indices().rev().nth(count.saturating_sub(1)) {
        Some((offset, _)) => text.get(offset..).unwrap_or(""),
        None => text,
    }
}

/// First `count` characters of `text`.
fn head_chars(text: &str, count: usize) -> &str {
    if count == 0 {
        return "";
    }
    match text.char_indices().nth(count) {
        Some((offset, _)) => text.get(..offset).unwrap_or(""),
        None => text,
    }
}

/// Clip text to a character count, on a character boundary.
///
/// Returns the clipped text and whether anything was dropped, so a prompt can
/// say that a passage continues instead of implying the message ends there.
pub fn clip_chars(text: &str, max_chars: usize) -> (&str, bool) {
    match text.char_indices().nth(max_chars) {
        Some((offset, _)) => (text.get(..offset).unwrap_or(text), true),
        None => (text, false),
    }
}
