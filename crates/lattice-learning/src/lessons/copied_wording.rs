//! Source wording stands only in double quotation marks. A lesson states a
//! fact in its own sentence; the words of the source are the citation's
//! quote. A teaching sentence that holds one of its part's quotes outside
//! quotation marks, or sits inside one, has copied the source.
use super::same_text::fold;
use super::text_edits::citable_texts;
use crate::generation::{long_enough_quote, teaching_sentences};
use crate::review_findings::{FindingKind, FindingSite, ReviewFinding};
use serde_json::Value;

/// The quotes among `quotes` that are long enough to be a quotation; a copy
/// of a shorter span is no copy of the source's wording.
pub(crate) fn kept<'a>(quotes: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    quotes
        .into_iter()
        .filter(|quote| long_enough_quote(quote))
        .map(str::to_owned)
        .collect()
}

/// The quotes of `part`, a block or question: its own and its citations'.
pub(crate) fn quotes_of(part: &Value) -> Vec<String> {
    let cited = part
        .get("citations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("quote"));
    kept(
        part.get("quote")
            .into_iter()
            .chain(cited)
            .filter_map(Value::as_str),
    )
}

/// `text` without its double-quoted spans, each left as a space. A quotation
/// mark that is never closed hides the rest of its text.
fn outside_quotes(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find(['"', '\u{201C}']) {
        out.push_str(rest.get(..open).unwrap_or_default());
        let mut chars = rest.get(open..).unwrap_or_default().chars();
        let closer = if chars.next() == Some('\u{201C}') {
            '\u{201D}'
        } else {
            '"'
        };
        let body = chars.as_str();
        let Some(close) = body.find(closer) else {
            return out;
        };
        out.push(' ');
        rest = body.get(close + closer.len_utf8()..).unwrap_or_default();
    }
    out.push_str(rest);
    out
}

/// The words of `text` as sentences are compared here: folded, without the
/// punctuation at either end of a word.
fn words(text: &str) -> Vec<String> {
    fold(text)
        .split_whitespace()
        .map(|word| word.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Whether `hay` holds the run of words `needle`.
fn holds(hay: &[String], needle: &[String]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|run| run == needle)
}

/// The teaching sentences of `text` that copy one of `quotes` outside double
/// quotation marks: they hold its words, or are a span of them long enough
/// to be quoted.
pub(crate) fn copies(text: &str, quotes: &[String]) -> Vec<String> {
    let quotes: Vec<Vec<String>> = quotes.iter().map(|quote| words(quote)).collect();
    if quotes.iter().all(Vec::is_empty) {
        return Vec::new();
    }
    teaching_sentences(text)
        .into_iter()
        .filter(|sentence| {
            let own = words(&outside_quotes(sentence));
            quotes.iter().any(|quote| {
                holds(&own, quote) || (holds(quote, &own) && long_enough_quote(&own.join(" ")))
            })
        })
        .collect()
}

/// Whether `path` names a text that teaches in a block or question: a
/// block's body, a question's explanation or one of its choice reasons.
pub(crate) fn is_taught_text(path: &str) -> bool {
    path.ends_with("/body") || path.ends_with("/explanation") || path.contains("/rationales/")
}

/// The first sentence `after` copies from `quotes` that `before` did not.
pub(crate) fn gained(before: &str, after: &str, quotes: &[String]) -> Option<String> {
    let had = copies(before, quotes);
    copies(after, quotes)
        .into_iter()
        .find(|sentence| !had.contains(sentence))
}

/// What a repair is told about a sentence that copies a source.
pub(crate) const SAY_IT_YOURSELF: &str =
    "Say it in the lesson's own words, or put the source's words in double quotation marks.";

/// A finding for every sentence of `candidate`'s blocks and questions that
/// copies the source's wording.
pub(crate) fn findings(candidate: &Value) -> Vec<ReviewFinding> {
    let mut found = Vec::new();
    for (list, question) in [("blocks", false), ("questions", true)] {
        let parts = candidate.get(list).and_then(Value::as_array);
        for (index, part) in parts.into_iter().flatten().enumerate() {
            let quotes = quotes_of(part);
            let (site, label, plain) = if question {
                let name = format!("Question {}", index + 1);
                (FindingSite::Question(index), name.clone(), name)
            } else {
                (
                    FindingSite::Section(index),
                    format!("Section {} (candidate blocks-{index})", index + 1),
                    format!("Section {}", index + 1),
                )
            };
            for text in citable_texts(part, question) {
                for sentence in copies(text, &quotes) {
                    let shown: String = sentence.chars().take(90).collect();
                    found.push(
                        ReviewFinding::new(
                            FindingKind::Teaching,
                            Some(site),
                            format!("{label} copies the source's wording in \"{shown}\" outside quotation marks. {SAY_IT_YOURSELF}"),
                        )
                        .with_summary(format!("{plain} copies a source's wording without quotation marks: \u{201c}{sentence}\u{201d}"))
                        .with_passage(&sentence),
                    );
                }
            }
        }
    }
    found
}
