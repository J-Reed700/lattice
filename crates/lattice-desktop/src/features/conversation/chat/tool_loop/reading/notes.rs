//! What a reading's lines say, and which passage each note was copied from.
//!
//! A note counts only when it is copied word for word, and it belongs to the
//! passage it was copied from, whatever label the model wrote before it. Both
//! are decided on exact text, so they hold for any language and need no
//! threshold.

use super::Passages;
use lattice_core::shared::text::normalize_whitespace;
use std::collections::{BTreeMap, BTreeSet};
use unicode_segmentation::UnicodeSegmentation;

/// The notes as `(label, note)`, in the order written: a line that starts
/// with a label, bracketed or bare, then an optional `:`, `.` or dash, in
/// their full-width forms too. A bare number before a bracketed one numbers
/// the list (`1. [1] -`), and the bracketed one is the label. A note with no
/// letter or digit in it — a hyphen — is `None`. Lines without a label are
/// not notes.
pub(super) fn parse_notes(text: &str) -> Vec<(u32, Option<String>)> {
    text.lines().filter_map(parse_line).collect()
}

fn parse_line(line: &str) -> Option<(u32, Option<String>)> {
    let (label, bracketed, rest) = leading_label(line)?;
    let (label, rest) = match leading_label(rest) {
        Some((listed, true, after)) if !bracketed => (listed, after),
        _ => (label, rest),
    };
    let note = rest.trim();
    let says_something = note.chars().any(|c| c.is_alphanumeric() || c == '_');
    Some((label, says_something.then(|| note.to_string())))
}

/// The label `text` opens with, whether it stood in brackets, and what
/// follows it past an optional `:`, `.` or dash.
fn leading_label(text: &str) -> Option<(u32, bool, &str)> {
    let rest = text.trim_start();
    let opened = rest.strip_prefix(|c: char| narrow(c) == '[');
    let rest = opened.unwrap_or(rest);
    let digits = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    let label: u32 = rest
        .get(..digits)
        .filter(|digits| !digits.is_empty())?
        .parse()
        .ok()?;
    let rest = &rest[digits..];
    let closed = rest.strip_prefix(|c: char| narrow(c) == ']');
    let bracketed = opened.is_some() && closed.is_some();
    let rest = closed.unwrap_or(rest).trim_start();
    let rest = rest
        .strip_prefix(|c: char| matches!(narrow(c), ':' | '.' | '-' | '–' | '—'))
        .unwrap_or(rest);
    Some((label, bracketed, rest))
}

/// A full-width form (`：`, `．`, `［` …) read as the ASCII character it is
/// the wide form of, so a label is set off the same way in any script.
fn narrow(c: char) -> char {
    match c {
        '\u{FF01}'..='\u{FF5E}' => char::from_u32(u32::from(c) - 0xFEE0).unwrap_or(c),
        _ => c,
    }
}

/// The passages a note is copied from: each that holds every one of its
/// sentences, with whitespace runs read as one space and any quotation marks
/// around it dropped, as written. Empty for a note with nothing in it to
/// copy.
pub(super) fn locate(note: &str, passages: &Passages) -> Vec<u32> {
    let sentences: Vec<String> = sentences(note)
        .into_iter()
        .map(|sentence| {
            normalize_whitespace(sentence)
                .trim_matches(['"', '“', '”', '„', '«', '»'])
                .to_string()
        })
        .filter(|sentence| !sentence.is_empty())
        .collect();
    if sentences.is_empty() {
        return Vec::new();
    }
    passages
        .0
        .iter()
        .filter(|(_, passage)| {
            sentences
                .iter()
                .all(|sentence| passage.normalized.contains(sentence.as_str()))
        })
        .map(|(label, _)| *label)
        .collect()
}

/// A note split into its sentences where Unicode puts their boundaries
/// (UAX #29: after `.`, `!` or `?` and a space, after `。`, `！`, `？` and
/// every other script's terminators), so the sentences it copied need not
/// have stood side by side in the passage.
fn sentences(note: &str) -> Vec<&str> {
    note.split_sentence_bounds().collect()
}

/// Put each note under the passage it was copied from. Returns the labels
/// whose note is now a copy of their own passage, and each note moved, as
/// `(label written, label copied from)`.
///
/// A note copied from its own passage stays. One copied from exactly one
/// other passage is that passage's note: it takes the place of what was
/// written there — a hyphen, a mistaken note or nothing — unless that
/// passage holds a copy of its own, and the label written keeps its line as
/// a mistaken note. A note found in no passage, or in several (documents
/// repeat their boilerplate), stays where it was written, mistaken: nothing
/// is guessed. Notes move in label order, so the first copy to reach a
/// passage is the one it keeps.
pub(super) fn attribute(
    lines: &mut BTreeMap<u32, Option<String>>,
    passages: &Passages,
) -> (BTreeSet<u32>, Vec<(u32, u32)>) {
    let found: Vec<(u32, String, Vec<u32>)> = lines
        .iter()
        .filter_map(|(label, note)| {
            let note = note.as_ref()?;
            Some((*label, note.clone(), locate(note, passages)))
        })
        .collect();
    let mut noted: BTreeSet<u32> = found
        .iter()
        .filter(|(label, _, found_in)| found_in.contains(label))
        .map(|(label, ..)| *label)
        .collect();
    let mut moved = Vec::new();
    for (label, note, found_in) in found {
        let [from] = found_in[..] else {
            continue;
        };
        if noted.contains(&from) {
            continue;
        }
        lines.insert(from, Some(note));
        noted.insert(from);
        moved.push((label, from));
    }
    (noted, moved)
}

/// The notes to give the writing round when they are not what the model
/// wrote: the lines as kept, one per label in label order, as `[label] note`.
///
/// `None` when every line of the model's text is a note kept as written and
/// none moved: that text is replayed byte for byte, so the writing round's
/// prompt runs on from the one the reading was cached under. A paragraph
/// after the lines, a line the reading did not keep or a note moved, and the
/// text is rebuilt: replayed as the model's own words, it is what the writer
/// would repeat and cite.
pub(super) fn corrected(
    text: &str,
    lines: &BTreeMap<u32, Option<String>>,
    moved: &[(u32, u32)],
) -> Option<String> {
    let as_written = moved.is_empty()
        && text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .all(|line| {
                parse_line(line).is_some_and(|(label, note)| lines.get(&label) == Some(&note))
            });
    (!as_written).then(|| as_kept(lines))
}

/// The lines as kept, one per label in label order, as `[label] note` or
/// `[label] -`.
pub(super) fn as_kept(lines: &BTreeMap<u32, Option<String>>) -> String {
    lines
        .iter()
        .map(|(label, note)| format!("[{label}] {}", note.as_deref().unwrap_or("-")))
        .collect::<Vec<_>>()
        .join("\n")
}
