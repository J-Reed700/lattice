//! Sentence splitting and chip-to-sentence assignment for reply support.

use std::ops::Range;

use lattice_core::shared::text::segmentation::CodeFence;

use super::super::reply_text::ReplyText;

#[derive(Debug, Clone)]
pub(super) struct SentenceSpan {
    pub(super) range: Range<usize>,
    pub(super) ended_by_newline: bool,
    pub(super) has_words: bool,
    pub(super) in_fence: bool,
    pub(super) is_heading: bool,
}

pub(super) fn split_sentence_spans(
    reply: &ReplyText,
    fences: &[CodeFence],
    formulas: &[Range<usize>],
) -> Vec<SentenceSpan> {
    let text = &reply.text;
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut cursor = 0usize;

    while cursor < text.len() {
        // Keep placed references `[s1]` / `[f1:2-5]` intact inside their span.
        if let Some(placed) = reply
            .references
            .iter()
            .find(|placed| placed.range.start == cursor)
        {
            cursor = placed.range.end;
            continue;
        }
        if let Some(formula) = formulas.iter().find(|formula| formula.start == cursor) {
            cursor = formula.end.max(cursor + 1);
            continue;
        }
        let Some(ch) = text.get(cursor..).and_then(|rest| rest.chars().next()) else {
            break;
        };
        let next_pos = cursor + ch.len_utf8();
        let inside_fence = fences.iter().any(|fence| {
            let whole = fence.whole();
            cursor >= whole.start && cursor < whole.end
        });
        let next_char = text.get(next_pos..).and_then(|rest| rest.chars().next());
        let ends_line = ch == '\n';
        let ends_stop = !inside_fence
            && matches!(ch, '.' | '!' | '?')
            && !next_char.is_some_and(|next| next.is_alphanumeric() || next == '.');

        if ends_line || ends_stop {
            let span_end = if ends_line { cursor } else { next_pos };
            push_sentence_span(reply, start..span_end, ends_line, fences, &mut out);
            start = next_pos;
        }
        cursor = next_pos;
    }

    if start < text.len() {
        push_sentence_span(reply, start..text.len(), false, fences, &mut out);
    }
    out
}

fn push_sentence_span(
    reply: &ReplyText,
    range: Range<usize>,
    ended_by_newline: bool,
    fences: &[CodeFence],
    out: &mut Vec<SentenceSpan>,
) {
    let Some(raw) = reply.text.get(range.clone()) else {
        return;
    };
    let has_ref = reply
        .references
        .iter()
        .any(|p| p.range.start >= range.start && p.range.end <= range.end);
    if raw.trim().is_empty() && !has_ref {
        return;
    }
    let plain = reply.without_references_in(range.clone());
    let has_words = plain.chars().any(char::is_alphanumeric);
    let in_fence = fences.iter().any(|fence| {
        let whole = fence.whole();
        range.start >= whole.start && range.end <= whole.end
    });
    let trimmed = raw.trim_start();
    let hashes = trimmed.chars().take_while(|ch| *ch == '#').count();
    let is_heading = (1..=6).contains(&hashes)
        && trimmed
            .get(hashes..)
            .is_some_and(|rest| rest.starts_with(' '));
    out.push(SentenceSpan {
        range,
        ended_by_newline,
        has_words,
        in_fence,
        is_heading,
    });
}

pub(super) fn assign_sentences(
    reply: &ReplyText,
    sentences: &[SentenceSpan],
) -> Vec<Option<usize>> {
    reply
        .references
        .iter()
        .map(|placed| {
            let own_idx = sentences.iter().position(|span| {
                placed.range.start >= span.range.start && placed.range.end <= span.range.end
            })?;
            let own = sentences.get(own_idx)?;
            let prefix_before = reply.without_references_in(own.range.start..placed.range.start);
            if prefix_before.chars().any(char::is_alphanumeric) {
                return Some(own_idx);
            }
            // No words before `[sN]` inside its own split chunk: check if the
            // immediately preceding same-paragraph chunk ended at punctuation
            // with words (e.g. `"Alpha." [s1]`), provided that earlier chunk
            // doesn't already carry its own reference when this chunk continues
            // with words after `[sN]`.
            if let Some(prev_idx) = own_idx.checked_sub(1) {
                if let Some(prev) = sentences.get(prev_idx) {
                    let prev_has_ref = reply.references.iter().any(|other| {
                        other.range.start >= prev.range.start && other.range.end <= prev.range.end
                    });
                    let after_has_words = reply
                        .without_references_in(placed.range.end..own.range.end)
                        .chars()
                        .any(char::is_alphanumeric);
                    if !prev.ended_by_newline
                        && prev.has_words
                        && !(prev_has_ref && after_has_words)
                    {
                        return Some(prev_idx);
                    }
                }
            }
            Some(own_idx)
        })
        .collect()
}
