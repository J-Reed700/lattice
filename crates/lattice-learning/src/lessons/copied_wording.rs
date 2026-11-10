//! Source wording stands only in double quotation marks. A lesson states a
//! fact in its own sentence; the words of the source are the citation's
//! quote. A teaching sentence that holds a source's wording outside
//! quotation marks, or sits inside one, has copied the source.
use super::same_text::fold;
use super::text_edits::citable_texts;
use crate::generation::{long_enough_quote, teaching_sentences};
use crate::reference_collection::ReferenceCollection;
use crate::review_findings::{FindingKind, FindingSite, ReviewFinding, Rule};
use lattice_core::shared::text::sentences;
use serde_json::Value;

/// Source wording as folded runs of words, each long enough to be quoted: a
/// copy of a shorter span is no copy of the source. Folded once, when built.
#[derive(Clone, Debug, Default)]
pub(crate) struct Wording(Vec<Vec<String>>);

impl Wording {
    /// The wording of `quotes`, those long enough to be a quotation.
    pub(crate) fn of_quotes<'a>(quotes: impl IntoIterator<Item = &'a str>) -> Self {
        let mut wording = Self::default();
        wording.add(quotes);
        wording
    }

    /// The quotes of `part`, a block or question: its own and its citations',
    /// and those of citations a patch appends.
    pub(crate) fn of_part<'a>(
        part: &'a Value,
        appended: impl IntoIterator<Item = &'a str>,
    ) -> Self {
        let cited = part
            .get("citations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.get("quote"));
        Self::of_quotes(
            part.get("quote")
                .into_iter()
                .chain(cited)
                .filter_map(Value::as_str)
                .chain(appended),
        )
    }

    /// The saved excerpts: each whole, and each of its sentences.
    pub(crate) fn of_excerpts<'a>(excerpts: impl IntoIterator<Item = &'a str>) -> Self {
        let mut wording = Self::default();
        for excerpt in excerpts {
            wording.add([excerpt]);
            wording.add(
                sentences(excerpt)
                    .into_iter()
                    .filter_map(|(from, to)| excerpt.get(from..to)),
            );
        }
        wording
    }

    /// The excerpts of a lesson's reference collection.
    pub(crate) fn of(references: &ReferenceCollection<'_>) -> Self {
        Self::of_excerpts(
            references
                .sources
                .iter()
                .map(|source| source.excerpt.as_str()),
        )
    }

    fn add<'a>(&mut self, texts: impl IntoIterator<Item = &'a str>) {
        self.0.extend(
            texts
                .into_iter()
                .filter(|text| long_enough_quote(text))
                .map(words)
                .filter(|run| !run.is_empty()),
        );
    }
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

/// The teaching sentences of `text` that copy the wording of the part or the
/// lesson outside double quotation marks: they hold a run of it, or are a
/// span of one long enough to be quoted.
pub(crate) fn copies(text: &str, part: &Wording, lesson: &Wording) -> Vec<String> {
    if part.0.is_empty() && lesson.0.is_empty() {
        return Vec::new();
    }
    teaching_sentences(text)
        .into_iter()
        .filter(|sentence| {
            let own = words(&outside_quotes(sentence));
            part.0.iter().chain(&lesson.0).any(|run| {
                holds(&own, run) || (holds(run, &own) && long_enough_quote(&own.join(" ")))
            })
        })
        .collect()
}

/// Whether `path` names a text that teaches in a block or question: a
/// block's body, a question's explanation or one of its choice reasons.
pub(crate) fn is_taught_text(path: &str) -> bool {
    path.ends_with("/body") || path.ends_with("/explanation") || path.contains("/rationales/")
}

/// The first sentence `after` copies that `before` did not.
pub(crate) fn gained(
    before: &str,
    after: &str,
    part: &Wording,
    lesson: &Wording,
) -> Option<String> {
    let had = copies(before, part, lesson);
    copies(after, part, lesson)
        .into_iter()
        .find(|sentence| !had.contains(sentence))
}

/// What a repair is told about a sentence that copies a source.
pub(crate) const SAY_IT_YOURSELF: &str =
    "Say it in the lesson's own words, or put the source's words in double quotation marks.";

/// A finding for every teaching sentence of `candidate`'s blocks and
/// questions that copies a quote its part cites or the lesson's `wording`,
/// each sentence once.
pub(crate) fn findings(candidate: &Value, wording: &Wording) -> Vec<ReviewFinding> {
    let mut found = Vec::new();
    for (list, question) in [("blocks", false), ("questions", true)] {
        let parts = candidate.get(list).and_then(Value::as_array);
        for (index, part) in parts.into_iter().flatten().enumerate() {
            let quotes = Wording::of_part(part, []);
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
                for sentence in copies(text, &quotes, wording) {
                    found.push(
                        ReviewFinding::rule_about(
                            Rule::CopiedWording,
                            FindingKind::Teaching,
                            site,
                            &sentence,
                            format!("{label} copies the source's wording in \"{sentence}\" outside quotation marks. {SAY_IT_YOURSELF}"),
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
