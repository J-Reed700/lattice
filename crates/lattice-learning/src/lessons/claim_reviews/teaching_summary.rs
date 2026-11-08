//! The plain summary of a teaching finding recorded before findings kept one:
//! rebuilt from the note the repair was given, and from the text the finding
//! is about. A sentence the note cut short is completed only when exactly one
//! sentence of that text begins with it, and left out otherwise.
use crate::generation::prose_sentences;
use crate::review_findings::{plain_part, FindingKind, FindingSite};

/// How long the notes of the old rows let a sentence run before cutting it.
const UNCITED_CUT: usize = 90;
const CITED_CUT: usize = 80;

const AUDIT_NOTE: &str = "The claim audit found assertions here that were not extracted for checking, so no source has confirmed them: ";

/// A stored finding with no summary, and the text it is about.
pub(crate) struct Stored<'a> {
    pub kind: FindingKind,
    pub site: FindingSite,
    pub note: &'a str,
    pub passage: Option<&'a str>,
    /// The published section the finding is about, for a section's finding.
    pub section: Option<&'a str>,
}

/// What a finding says when its note is in no format this knows.
pub(crate) fn fallback(kind: FindingKind) -> &'static str {
    match kind {
        FindingKind::Teaching => {
            "The teaching review found a problem here that its repairs did not fix."
        }
        FindingKind::AnswerKey => "A second, independent solution disagreed with the answer key.",
        FindingKind::Structure => "Part of this lesson\u{2019}s shape did not pass its check.",
        FindingKind::Quotation => "A quotation here could not be matched to a saved source.",
    }
}

pub(crate) fn derive(row: &Stored) -> String {
    let said = match row.kind {
        FindingKind::Quotation => quotation(row),
        FindingKind::Teaching => teaching(row.note),
        FindingKind::AnswerKey | FindingKind::Structure => None,
    };
    said.unwrap_or_else(|| fallback(row.kind).to_owned())
}

fn quoted(text: &str) -> String {
    format!("\u{201c}{text}\u{201d}")
}

fn between<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = text.find(open)? + open.len();
    let end = text.get(start..)?.find(close)? + start;
    text.get(start..end)
}

/// `shown` as the whole sentence it was cut from, when that is known.
fn whole_sentence(shown: &str, cut: usize, row: &Stored) -> Option<String> {
    if shown.chars().count() < cut {
        return Some(shown.to_owned());
    }
    let mut found: Vec<String> = Vec::new();
    for sentence in row
        .passage
        .into_iter()
        .chain(row.section)
        .flat_map(prose_sentences)
    {
        if sentence.starts_with(shown) && !found.contains(&sentence) {
            found.push(sentence);
        }
    }
    match found.as_slice() {
        [only] => Some(only.clone()),
        _ => None,
    }
}

/// "{lead}: “sentence”", or "{lead}." when the sentence is not known whole.
fn about(lead: &str, sentence: Option<String>) -> String {
    match sentence {
        Some(sentence) => format!("{lead}: {}", quoted(&sentence)),
        None => format!("{lead}."),
    }
}

fn quotation(row: &Stored) -> Option<String> {
    let note = row.note;
    if let Some(shown) = between(note, "states a specific in \"", "\" without citing it.") {
        return Some(about(
            "No source is cited for this sentence",
            whole_sentence(shown, UNCITED_CUT, row),
        ));
    }
    if let (Some(shown), Some(missing)) = (
        between(note, "the quote cited for \"", "\" does not contain "),
        between(
            note,
            "\" does not contain ",
            ". Cite a quote of a supplied excerpt",
        ),
    ) {
        return Some(about(
            &format!("The quotation cited for this sentence does not contain {missing}"),
            whole_sentence(shown, UNCITED_CUT, row),
        ));
    }
    if let Some(shown) = between(
        note,
        "The source citation of the sentence \"",
        "\" could not be confirmed: ",
    ) {
        return Some(about(
            "The quotation cited for this sentence could not be found in its source",
            whole_sentence(shown, CITED_CUT, row),
        ));
    }
    (note.starts_with("The source citation at ") || note.starts_with("The source quotation at "))
        .then(|| {
            format!(
                "The quotation cited in {} could not be found in its source.",
                plain_part(row.site)
            )
        })
}

fn teaching(note: &str) -> Option<String> {
    if let Some(list) = note.strip_prefix(AUDIT_NOTE) {
        let statements: Vec<String> = list.split("; ").map(quoted).collect();
        return Some(match statements.as_slice() {
            [only] => format!("This statement was never checked against a source: {only}."),
            _ => format!(
                "These statements were never checked against a source: {}.",
                statements.join(", ")
            ),
        });
    }
    let (place, critique) = note.split_once("): ")?;
    let (part, _) = place.split_once(" (candidate ")?;
    let (word, number) = part.split_once(' ')?;
    (["Section", "Question"].contains(&word)
        && !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit())
        && !critique.trim().is_empty())
    .then(|| critique.trim().to_owned())
}
