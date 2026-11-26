//! A quotation is settled without a model: the words are in the passage it
//! cites or they are not. The judge reads a made-up sentence beside a passage
//! about the same thing and can only hesitate, which shows the reader nothing,
//! so a cited quotation is looked for in the text itself.
//!
//! Only a quotation with a marker right behind it counts. One cited further
//! along its sentence is as often a document's title, which no passage holds.

use std::collections::{BTreeSet, HashSet};

use lattice_core::application::contracts::sources::SourceDto;
use lattice_core::shared::text::same_text::{self, long_enough_quote, quotation};

use super::lexical::{passage_text, prose_lines, source_index, split_sentences, LexicalClaim};
use super::marked;
use crate::features::conversation::chat::reply_text::ReplyText;

const QUOTE_MARKS: &[char] = &['"', '\u{201c}', '\u{201d}'];
const ELLIPSES: &[&str] = &["...", "\u{2026}"];

struct Quotation<'a> {
    text: &'a str,
    /// The markers behind it, and any right before it: some answers label a
    /// quotation first, so the marker behind it may be the next one's.
    cited: BTreeSet<u32>,
}

/// The claims that quote words the passages they cite do not hold.
pub(super) fn misquoted_claims(
    reply: &ReplyText,
    sources: &[SourceDto],
    claims: &[LexicalClaim],
) -> HashSet<usize> {
    let mut misquoted = HashSet::new();
    for line in prose_lines(&marked::marked(reply)) {
        for quoted in quotations(&line) {
            let Some(passages) = cited_passages(&quoted.cited, sources) else {
                continue;
            };
            for fragment in quoted_sentences(quoted.text) {
                if passages
                    .iter()
                    .any(|passage| quotation(passage, &fragment).is_some())
                {
                    continue;
                }
                misquoted.extend(claims.iter().enumerate().filter_map(|(index, claim)| {
                    let cites = claim
                        .citation_ids
                        .iter()
                        .any(|id| quoted.cited.contains(id));
                    let claim_text = unwrap_case_edits(&claim.claim_text);
                    (cites && same_text::find(&claim_text, &fragment).is_some()).then_some(index)
                }));
            }
        }
    }
    misquoted
}

/// Matching spans `(start, end)` in `source_text` for explicit quotations found
/// inside a cited prose slice `prose` (excluding misquoted fragments not held by
/// `source_text`).
pub(in crate::features::conversation::chat) fn matched_quotations(
    prose: &str,
    source_text: &str,
) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let marks: Vec<usize> = prose.match_indices(QUOTE_MARKS).map(|(at, _)| at).collect();
    let mut index = 0;
    while let (Some(&open), Some(&close)) = (marks.get(index), marks.get(index + 1)) {
        let before = prose.get(..open).unwrap_or_default();
        let inner = prose.get(open..close).map(past_mark).unwrap_or_default();
        if !opens_quotation(before, inner) {
            index += 1;
            continue;
        }
        index += 2;
        for fragment in quoted_sentences(inner) {
            if let Some(span) = quotation(source_text, &fragment) {
                spans.push(span);
            }
        }
    }
    spans
}

/// The cited quotations of one line. Claims are cut at a full stop even inside
/// a quotation, so these are read from the line, not from its sentences.
fn quotations(line: &str) -> Vec<Quotation<'_>> {
    let marks: Vec<usize> = line.match_indices(QUOTE_MARKS).map(|(at, _)| at).collect();
    let mut found = Vec::new();
    let mut index = 0;
    while let (Some(&open), Some(&close)) = (marks.get(index), marks.get(index + 1)) {
        let before = line.get(..open).unwrap_or_default();
        let text = line.get(open..close).map(past_mark).unwrap_or_default();
        if !opens_quotation(before, text) {
            index += 1;
            continue;
        }
        index += 2;
        let behind = markers_behind(line.get(close..).map(past_mark).unwrap_or_default());
        if !behind.is_empty() {
            found.push(Quotation {
                text,
                cited: behind.union(&markers_before(before)).copied().collect(),
            });
        }
    }
    found
}

fn past_mark(text: &str) -> &str {
    let mut chars = text.chars();
    chars.next();
    chars.as_str()
}

/// An opening mark follows a space or bracket and comes before a word, which
/// tells it from the inch mark in `a 6" pipe`.
fn opens_quotation(before: &str, text: &str) -> bool {
    !before
        .chars()
        .next_back()
        .is_some_and(char::is_alphanumeric)
        && text.chars().next().is_some_and(|c| !c.is_whitespace())
}

fn markers_behind(text: &str) -> BTreeSet<u32> {
    let mut rest = text
        .strip_prefix(['.', ',', ';', ':'])
        .unwrap_or(text)
        .trim_start();
    let mut cited = BTreeSet::new();
    while let Some((number, tail)) = marked::leading(rest) {
        cited.insert(number);
        rest = tail.trim_start();
    }
    cited
}

fn markers_before(text: &str) -> BTreeSet<u32> {
    let mut rest = text.trim_end();
    let mut cited = BTreeSet::new();
    while let Some((head, number)) = marked::trailing(rest) {
        cited.insert(number);
        rest = head.trim_end();
    }
    cited
}

/// Each cited passage's text, or None when one cannot be read: a quotation is
/// not missing from a page nobody has the text of.
fn cited_passages<'a>(cited: &BTreeSet<u32>, sources: &'a [SourceDto]) -> Option<Vec<&'a str>> {
    cited
        .iter()
        .map(|id| {
            let text = passage_text(sources.get(source_index(*id, sources)?)?);
            (!text.trim().is_empty()).then_some(text)
        })
        .collect()
}

/// What a quotation gives word for word, a sentence at a time, so that where
/// it runs over a sentence's end only the sentence that is wrong is marked.
fn quoted_sentences(text: &str) -> Vec<String> {
    let text = marked::without(text);
    let mut parts = vec![text.as_str()];
    for ellipsis in ELLIPSES {
        parts = parts
            .into_iter()
            .flat_map(|part| part.split(ellipsis))
            .collect();
    }
    parts
        .into_iter()
        .flat_map(between_edits)
        .flat_map(split_sentences)
        .map(|sentence| unwrap_case_edits(sentence.trim()))
        .filter(|sentence| long_enough_quote(sentence))
        .collect()
}

/// `text` either side of each bracketed edit, such as "[must]". "[t]he" only
/// changes a letter's case, which the comparison ignores.
fn between_edits(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    for (open, _) in text.match_indices('[') {
        let Some((inside, _)) = text.get(open + 1..).and_then(|r| r.split_once(']')) else {
            break;
        };
        if inside.chars().count() == 1 && inside.chars().all(char::is_alphabetic) {
            continue;
        }
        parts.push(text.get(start..open).unwrap_or_default());
        start = open + inside.len() + 2;
    }
    parts.push(text.get(start..).unwrap_or_default());
    parts
}

/// Unwraps single-letter case edits like `[t]he` into `the` so `same_text` can
/// match them without ignoring square brackets elsewhere.
fn unwrap_case_edits(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    while let Some(rel_open) = text.get(cursor..).and_then(|rest| rest.find('[')) {
        let open = cursor + rel_open;
        let Some((inside, _)) = text.get(open + 1..).and_then(|rest| rest.split_once(']')) else {
            break;
        };
        if inside.chars().count() == 1 && inside.chars().all(char::is_alphabetic) {
            out.push_str(text.get(cursor..open).unwrap_or_default());
            out.push_str(inside);
            cursor = open + inside.len() + 2;
        } else {
            let end = open + 1;
            out.push_str(text.get(cursor..end).unwrap_or_default());
            cursor = end;
        }
    }
    out.push_str(text.get(cursor..).unwrap_or_default());
    out
}

/// Quoted stretches inside `prose` (character spans `(open_quote, close_quote_end)`),
/// used to tell already-quoted prose from unquoted prose.
pub(in crate::features::conversation::chat) fn quoted_ranges(prose: &str) -> Vec<(usize, usize)> {
    let marks: Vec<usize> = prose.match_indices(QUOTE_MARKS).map(|(at, _)| at).collect();
    let mut ranges = Vec::new();
    let mut index = 0;
    while let (Some(&open), Some(&close)) = (marks.get(index), marks.get(index + 1)) {
        let before = prose.get(..open).unwrap_or_default();
        let inner = prose.get(open..close).map(past_mark).unwrap_or_default();
        if !opens_quotation(before, inner) {
            index += 1;
            continue;
        }
        index += 2;
        let close_end = prose
            .get(close..)
            .and_then(|rest| rest.chars().next().map(|ch| close + ch.len_utf8()))
            .unwrap_or(close);
        ranges.push((open, close_end));
    }
    ranges
}

#[cfg(test)]
mod tests;
