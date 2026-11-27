//! Locating exact unquoted source quotations inside prose sentences (§4).

use std::ops::Range;

use lattice_core::shared::text::code_spans;
use lattice_core::shared::text::same_text;
use lattice_core::shared::text::segmentation::CodeFence;

use super::super::reply_text::{Placed, ReplyText};
use super::super::verification::quotes;
use super::sentences::SentenceSpan;

pub(super) fn find_unquoted_in_sentence(
    reply: &ReplyText,
    sentence: &SentenceSpan,
    placed: &Placed,
    fences: &[CodeFence],
    formulas: &[Range<usize>],
    haystack: &str,
) -> Option<(Range<usize>, String)> {
    // Only search the clause inside this sentence that precedes `placed` (or
    // the immediately adjacent chip cluster `placed` belongs to) without
    // crossing prose separated by other chips.
    let mut seg_end = placed.range.start.min(sentence.range.end);
    let mut seg_start = sentence.range.start;
    while let Some(prev) = reply
        .references
        .iter()
        .filter(|other| other.range.end <= seg_end && other.range.start >= sentence.range.start)
        .max_by_key(|other| other.range.end)
    {
        let gap = reply.text.get(prev.range.end..seg_end).unwrap_or_default();
        if gap.trim().is_empty() {
            seg_end = prev.range.start;
        } else {
            seg_start = prev.range.end;
            break;
        }
    }
    if seg_start >= seg_end {
        return None;
    }

    let line_start = reply
        .text
        .get(..sentence.range.start)
        .and_then(|head| head.rfind('\n'))
        .map_or(0, |at| at + 1);
    let line_end = reply
        .text
        .get(sentence.range.end..)
        .and_then(|tail| tail.find('\n'))
        .map_or(reply.text.len(), |at| sentence.range.end + at);
    let line = reply.text.get(line_start..line_end).unwrap_or_default();
    let seg_rel_start = seg_start.saturating_sub(line_start);
    let seg_rel_end = seg_end.saturating_sub(line_start).min(line.len());

    let mut excluded: Vec<Range<usize>> = Vec::new();
    for span in code_spans(line) {
        excluded.push(span);
    }
    for (q_start, q_end) in quotes::quoted_ranges(line) {
        excluded.push(q_start..q_end);
    }
    for formula in formulas {
        if formula.end > line_start && formula.start < line_end {
            let start = formula.start.saturating_sub(line_start);
            let end = formula.end.saturating_sub(line_start).min(line.len());
            excluded.push(start..end);
        }
    }
    for fence in fences {
        let whole = fence.whole();
        if whole.end > line_start && whole.start < line_end {
            let start = whole.start.saturating_sub(line_start);
            let end = whole.end.saturating_sub(line_start).min(line.len());
            excluded.push(start..end);
        }
    }

    excluded.sort_by_key(|range| (range.start, range.end));
    let mut cursor = seg_rel_start;
    let mut candidates = Vec::new();
    for ex in excluded {
        if ex.end <= cursor {
            continue;
        }
        if ex.start >= seg_rel_end {
            break;
        }
        if cursor < ex.start {
            candidates.push((line_start + cursor)..(line_start + ex.start.min(seg_rel_end)));
        }
        cursor = cursor.max(ex.end);
    }
    if cursor < seg_rel_end {
        candidates.push((line_start + cursor)..(line_start + seg_rel_end));
    }

    let hay_folded = same_text::fold(haystack);
    if hay_folded.is_empty() {
        return None;
    }

    let mut best: Option<(Range<usize>, String)> = None;
    for candidate_range in candidates {
        if let Some((found_range, exact_quote)) =
            longest_unquoted_match(&reply.text, candidate_range, haystack, &hay_folded)
        {
            if best
                .as_ref()
                .is_none_or(|(prev, _)| found_range.len() > prev.len())
            {
                best = Some((found_range, exact_quote));
            }
        }
    }
    best
}

fn longest_unquoted_match(
    text: &str,
    range: Range<usize>,
    haystack: &str,
    hay_folded: &str,
) -> Option<(Range<usize>, String)> {
    let trimmed_range = trim_markdown_prose_prefix(text, range);
    if trimmed_range.start >= trimmed_range.end {
        return None;
    }
    let slice = text.get(trimmed_range.clone())?;
    if let Some((src_start, src_end)) = same_text::quotation(haystack, slice) {
        if is_word_bounded(haystack, src_start, src_end) {
            if let Some(exact) = haystack.get(src_start..src_end) {
                return Some((trimmed_range, exact.to_owned()));
            }
        }
    }

    let words = word_spans(text, trimmed_range.start, trimmed_range.end);
    if words.len() < same_text::MIN_QUOTE_WORDS {
        return None;
    }

    let mut best: Option<(Range<usize>, String)> = None;
    for start_idx in 0..=words.len() - same_text::MIN_QUOTE_WORDS {
        let Some(&(w_start, _)) = words.get(start_idx) else {
            continue;
        };
        let Some(&(_, min_end)) = words.get(start_idx + same_text::MIN_QUOTE_WORDS - 1) else {
            continue;
        };
        let Some(min_slice) = text.get(w_start..min_end) else {
            continue;
        };
        let min_clean_end = strip_trailing_clause_punct(text, w_start, min_end);
        let min_folded = same_text::fold(text.get(w_start..min_clean_end).unwrap_or(min_slice));
        if min_folded.is_empty() || !hay_folded.contains(&min_folded) {
            continue;
        }

        let mut end_idx = start_idx + same_text::MIN_QUOTE_WORDS - 1;
        while end_idx + 1 < words.len() {
            let next_end = words.get(end_idx + 1).map_or(min_end, |&(_, end)| end);
            let clean_next = strip_trailing_clause_punct(text, w_start, next_end);
            let next_folded = same_text::fold(text.get(w_start..clean_next).unwrap_or_default());
            if next_folded.is_empty() || !hay_folded.contains(&next_folded) {
                break;
            }
            end_idx += 1;
        }

        let Some(&(_, raw_end)) = words.get(end_idx) else {
            continue;
        };
        let clean_end = strip_trailing_clause_punct(text, w_start, raw_end);
        let picked = if text
            .get(w_start..raw_end)
            .and_then(|cand| same_text::quotation(haystack, cand))
            .is_some()
        {
            raw_end
        } else {
            clean_end
        };

        let Some(cand_slice) = text.get(w_start..picked) else {
            continue;
        };
        if let Some((src_start, src_end)) = same_text::quotation(haystack, cand_slice) {
            if is_word_bounded(haystack, src_start, src_end) {
                if let Some(exact) = haystack.get(src_start..src_end) {
                    let candidate_len = picked.saturating_sub(w_start);
                    if best
                        .as_ref()
                        .is_none_or(|(prev, _)| candidate_len > prev.len())
                    {
                        best = Some((w_start..picked, exact.to_owned()));
                    }
                }
            }
        }
    }
    best
}

fn is_word_bounded(text: &str, start: usize, end: usize) -> bool {
    let left_ok = text
        .get(..start)
        .and_then(|head| head.chars().next_back())
        .is_none_or(|ch| !ch.is_alphanumeric());
    let right_ok = text
        .get(end..)
        .and_then(|tail| tail.chars().next())
        .is_none_or(|ch| !ch.is_alphanumeric());
    left_ok && right_ok
}

fn trim_markdown_prose_prefix(text: &str, mut range: Range<usize>) -> Range<usize> {
    let slice = text.get(range.clone()).unwrap_or_default();
    let mut lead = 0usize;
    for (idx, ch) in slice.char_indices() {
        if ch.is_whitespace() || matches!(ch, '*' | '_' | '`' | '>' | '(') {
            lead = idx + ch.len_utf8();
        } else {
            break;
        }
    }
    let rest = slice.get(lead..).unwrap_or_default();
    let list_skip = if ["- ", "* ", "+ "]
        .iter()
        .any(|bullet| rest.starts_with(bullet))
    {
        2
    } else {
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        if digits > 0
            && rest
                .get(digits..)
                .is_some_and(|after| after.starts_with(". ") || after.starts_with(") "))
        {
            digits + 2
        } else {
            0
        }
    };
    range.start += lead + list_skip;

    let inner = text.get(range.clone()).unwrap_or_default();
    let mut trail = inner.len();
    for (idx, ch) in inner.char_indices().rev() {
        if ch.is_whitespace() || matches!(ch, '*' | '_' | '`') {
            trail = idx;
        } else {
            break;
        }
    }
    range.end = range.start + trail;
    range
}

fn word_spans(text: &str, start: usize, end: usize) -> Vec<(usize, usize)> {
    let Some(slice) = text.get(start..end) else {
        return Vec::new();
    };
    let mut spans = Vec::new();
    let mut word_start: Option<usize> = None;
    for (idx, ch) in slice.char_indices() {
        if ch.is_whitespace() {
            if let Some(ws) = word_start.take() {
                let raw_end = start + idx;
                if let Some(trimmed) = trim_token_emphasis(text, ws, raw_end) {
                    spans.push(trimmed);
                }
            }
        } else if word_start.is_none() {
            word_start = Some(start + idx);
        }
    }
    if let Some(ws) = word_start {
        if let Some(trimmed) = trim_token_emphasis(text, ws, end) {
            spans.push(trimmed);
        }
    }
    spans
}

fn trim_token_emphasis(text: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    let slice = text.get(start..end)?;
    let mut lead = 0usize;
    for (idx, ch) in slice.char_indices() {
        if matches!(ch, '*' | '_' | '`' | '(') {
            lead = idx + ch.len_utf8();
        } else {
            break;
        }
    }
    let mut trail = slice.len();
    for (idx, ch) in slice.char_indices().rev() {
        if idx < lead {
            break;
        }
        if matches!(ch, '*' | '_' | '`' | ')') {
            trail = idx;
        } else {
            break;
        }
    }
    (lead < trail).then_some((start + lead, start + trail))
}

fn strip_trailing_clause_punct(text: &str, start: usize, mut end: usize) -> usize {
    while end > start {
        let Some(slice) = text.get(start..end) else {
            break;
        };
        let Some(last) = slice.chars().next_back() else {
            break;
        };
        if matches!(last, ',' | ';' | ':' | '.' | '!' | '?') {
            end -= last.len_utf8();
        } else {
            break;
        }
    }
    end
}
