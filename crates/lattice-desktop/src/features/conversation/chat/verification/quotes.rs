//! A quotation is settled without a model: the words are in the passage it
//! cites or they are not. The judge reads a made-up sentence beside a passage
//! about the same thing and can only hesitate, which shows the reader nothing,
//! so a cited quotation is looked for in the text itself.
//!
//! Only a quotation with a marker right behind it counts. One cited further
//! along its sentence is as often a document's title, which no passage holds.

use std::collections::{BTreeSet, HashSet};

use lattice_core::application::contracts::sources::SourceDto;
use unicode_normalization::UnicodeNormalization;

use super::lexical::{
    cited_labels, lowercase_words, passage_text, prose_lines, source_index, split_sentences,
    LexicalClaim,
};

/// Below this a quoted stretch is more often a term than a quotation.
const MIN_QUOTE_WORDS: usize = 5;
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
    response: &str,
    sources: &[SourceDto],
    claims: &[LexicalClaim],
) -> HashSet<usize> {
    let mut misquoted = HashSet::new();
    for line in prose_lines(response) {
        for quotation in quotations(&line) {
            let Some(passages) = cited_passages(&quotation.cited, sources) else {
                continue;
            };
            for sentence in quoted_sentences(quotation.text) {
                let words = letters(&sentence);
                if passages.iter().any(|passage| passage.contains(&words)) {
                    continue;
                }
                misquoted.extend(claims.iter().enumerate().filter_map(|(index, claim)| {
                    let cites = claim
                        .citation_ids
                        .iter()
                        .any(|id| quotation.cited.contains(id));
                    (cites && letters(&claim.claim_text).contains(&words)).then_some(index)
                }));
            }
        }
    }
    misquoted
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
    while let Some((label, tail)) = rest.strip_prefix('[').and_then(|r| r.split_once(']')) {
        let marker = cited_labels(&format!("[{label}]"));
        if marker.is_empty() {
            break;
        }
        cited.extend(marker);
        rest = tail.trim_start();
    }
    cited
}

fn markers_before(text: &str) -> BTreeSet<u32> {
    let mut rest = text.trim_end();
    let mut cited = BTreeSet::new();
    while let Some((head, label)) = rest.strip_suffix(']').and_then(|r| r.rsplit_once('[')) {
        let marker = cited_labels(&format!("[{label}]"));
        if marker.is_empty() {
            break;
        }
        cited.extend(marker);
        rest = head.trim_end();
    }
    cited
}

/// Each cited passage's text, or None when one cannot be read: a quotation is
/// not missing from a page nobody has the text of.
fn cited_passages(cited: &BTreeSet<u32>, sources: &[SourceDto]) -> Option<Vec<String>> {
    cited
        .iter()
        .map(|id| {
            let text = passage_text(sources.get(source_index(*id, sources)?)?);
            (!text.trim().is_empty()).then(|| letters(text))
        })
        .collect()
}

/// What a quotation gives word for word, a sentence at a time, so that where
/// it runs over a sentence's end only the sentence that is wrong is marked.
fn quoted_sentences(text: &str) -> Vec<String> {
    let mut parts = vec![text];
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
        .filter(|sentence| sentence.split_whitespace().count() >= MIN_QUOTE_WORDS)
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

/// `text` without what a line break, hyphenation, typography or letter case
/// changes, so only a different word can fail a comparison.
fn letters(text: &str) -> String {
    lowercase_words(&text.nfkc().collect::<String>()).concat()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
mod tests;
