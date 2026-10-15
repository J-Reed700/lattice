//! The span of a retrieved passage an outline item may cite: whole sentences
//! of the saved page's own prose, within the quote limit.
use crate::features::learning::{generation, reference_collection::ReferencePassage};
use std::collections::HashSet;

/// The byte just past the last sentence end in `text`, including any closing
/// quotation mark or bracket that follows the terminal punctuation.
pub(super) fn last_sentence_end(text: &str) -> Option<usize> {
    sentence_ends(text).last()
}

pub(super) fn sentence_ends(text: &str) -> impl Iterator<Item = usize> + '_ {
    text.char_indices().filter_map(move |(i, c)| {
        if !matches!(c, '.' | '!' | '?') {
            return None;
        }
        let mut end = i + c.len_utf8();
        for next in text[end..].chars() {
            if matches!(next, '"' | '\u{201D}' | '\'' | '\u{2019}' | ')' | ']') {
                end += next.len_utf8();
            } else if next.is_whitespace() {
                return Some(end);
            } else {
                return None;
            }
        }
        Some(end)
    })
}

/// A line's text as a reader sees it: markdown link targets and bracket
/// markup removed, link text kept.
fn visible_text(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == ']' && chars.peek() == Some(&'(') {
            let mut depth = 0;
            for next in chars.by_ref() {
                match next {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
            }
            continue;
        }
        if c == '[' || c == ']' || (c == '!' && chars.peek() == Some(&'[')) {
            continue;
        }
        out.push(c);
    }
    out
}

/// Running text a learner can read as evidence: at least one full sentence of
/// five or more words (the quote rule's minimum), mostly letters. Headings, table rows, navigation,
/// link lists and code are not prose.
pub(super) fn is_prose(line: &str) -> bool {
    let line = line.trim();
    if line.is_empty()
        || line.starts_with('#')
        || line.starts_with('|')
        || line.starts_with("```")
        || line.matches('|').count() >= 2
    {
        return false;
    }
    let visible = visible_text(line);
    let words = visible
        .split_whitespace()
        .filter(|word| word.chars().any(char::is_alphabetic))
        .count();
    let letters = visible.chars().filter(|c| c.is_alphabetic()).count();
    let printable = visible.chars().filter(|c| !c.is_whitespace()).count();
    words >= 5 && letters * 10 >= printable * 7 && sentence_ends(visible.trim()).next().is_some()
}

fn terms(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.chars().count() >= 4)
        .map(str::to_lowercase)
        .collect()
}

/// The span of a retrieved passage that becomes the quote: the contiguous run
/// of prose lines that shares the most terms with the item, as whole sentences
/// within the quote length limit. A passage cut mid-sentence loses the partial
/// sentence. Returns nothing when no prose span satisfies the quote rules.
pub(super) fn citable_span(
    passage: &ReferencePassage,
    excerpt: &str,
    query: &str,
) -> Option<String> {
    let mut text = passage.text.as_str();
    let clean_start = passage.start_byte == 0
        || excerpt
            .get(..passage.start_byte)
            .is_some_and(|before| before.ends_with('\n'));
    if !clean_start {
        if let Some(cut) = sentence_ends(text).next() {
            text = &text[cut..];
        }
    }
    let clean_end = passage.end_byte >= excerpt.len()
        || excerpt
            .get(passage.end_byte..)
            .is_some_and(|after| after.starts_with('\n'));
    if !clean_end {
        if let Some(cut) = last_sentence_end(text) {
            text = &text[..cut];
        }
    }
    // Runs of prose lines (blank lines may separate paragraphs inside a run).
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut current: Option<(usize, usize)> = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        if is_prose(line) {
            let end = start + line.trim_end().len();
            current = Some(current.map_or((start, end), |(first, _)| (first, end)));
        } else if !line.trim().is_empty() {
            runs.extend(current.take());
        }
    }
    runs.extend(current);
    let wanted = terms(query);
    let (start, end) = runs
        .into_iter()
        .enumerate()
        .max_by_key(|(order, (start, end))| {
            let shared = terms(&text[*start..*end]).intersection(&wanted).count();
            (shared, std::cmp::Reverse(*order))
        })
        .map(|(_, run)| run)?;
    text = text[start..end].trim();
    while text.chars().count() > generation::MAX_QUOTE_CHARS {
        let limit = text
            .char_indices()
            .nth(generation::MAX_QUOTE_CHARS)
            .map_or(text.len(), |(i, _)| i);
        let head = &text[..limit];
        let cut = last_sentence_end(head).or_else(|| head.rfind(char::is_whitespace))?;
        text = text[..cut].trim_end();
    }
    generation::validate_excerpt_quote(text, excerpt)
        .ok()
        .map(|_| text.to_owned())
}
