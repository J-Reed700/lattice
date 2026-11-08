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

/// Whether a note is about a part's own quotation, not a sentence's.
fn names_a_part(note: &str) -> bool {
    note.starts_with("The source citation at ") || note.starts_with("The source quotation at ")
}

/// What the passage of a finding with no summary is. Before findings had a
/// summary a quotation finding on a section was given the section's quote:
/// the citation in question for a part, the wrong text for a sentence.
pub(crate) fn passage_shown(
    kind: FindingKind,
    note: &str,
    stored: Option<String>,
) -> Option<String> {
    match kind {
        FindingKind::Quotation if !names_a_part(note) => None,
        _ => stored,
    }
}

/// A stored finding with no summary, and the text it is about.
pub(crate) struct Stored<'a> {
    pub kind: FindingKind,
    pub site: FindingSite,
    pub note: &'a str,
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
    for sentence in row.section.into_iter().flat_map(prose_sentences) {
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

/// What a sentence-level quotation note says, and the whole sentence it is
/// about when that is known; none when the note is not about a sentence.
fn about_a_sentence(row: &Stored) -> Option<(String, Option<String>)> {
    let note = row.note;
    if let Some(shown) = between(note, "states a specific in \"", "\" without citing it.") {
        return Some((
            "No source is cited for this sentence".to_owned(),
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
        return Some((
            format!("The quotation cited for this sentence does not contain {missing}"),
            whole_sentence(shown, UNCITED_CUT, row),
        ));
    }
    let shown = between(
        note,
        "The source citation of the sentence \"",
        "\" could not be confirmed: ",
    )?;
    Some((
        "The quotation cited for this sentence could not be found in its source".to_owned(),
        whole_sentence(shown, CITED_CUT, row),
    ))
}

/// The whole sentence a finding with no stored sentence is about, when its
/// note names one and the text completes it.
pub(crate) fn sentence_of(row: &Stored) -> Option<String> {
    match row.kind {
        FindingKind::Quotation => about_a_sentence(row).and_then(|(_, sentence)| sentence),
        _ => None,
    }
}

fn quotation(row: &Stored) -> Option<String> {
    let note = row.note;
    if let Some((lead, sentence)) = about_a_sentence(row) {
        return Some(about(&lead, sentence));
    }
    names_a_part(note).then(|| {
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
            [only] => format!(
                "A statement here was never checked against a source. The checker read it as: {only}."
            ),
            _ => format!(
                "Some statements here were never checked against a source. The checker read them as: {}.",
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
