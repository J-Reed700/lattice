//! A sentence written into or out of a field's text, the way a put back and
//! its undo do it.

use lattice_core::domain::learning::claim_review::{span_of, Anchor, CutPlace};

/// `text` with `sentence` written in at `anchor`: one space between sentences
/// of a paragraph, paragraph breaks kept. `None` when the anchor's sentence is
/// not in `text` exactly once.
pub(crate) fn insert_sentence(text: &str, anchor: &Anchor, sentence: &str) -> Option<String> {
    let mut out = text.to_owned();
    match anchor {
        Anchor::After(other) => {
            let (_, end) = span_of(text, other)?;
            out.insert_str(end, &format!(" {sentence}"));
        }
        Anchor::Before(other) => {
            let (start, _) = span_of(text, other)?;
            out.insert_str(start, &format!("{sentence} "));
        }
        Anchor::End if text.trim().is_empty() => out = sentence.to_owned(),
        Anchor::End => out.insert_str(text.trim_end().len(), &format!(" {sentence}")),
    }
    Some(out)
}

/// `text` without the one occurrence of `sentence` and a space beside it;
/// `None` when it is not in `text` exactly once.
pub(crate) fn remove_sentence(text: &str, sentence: &str) -> Option<String> {
    span_of(text, sentence)?;
    for pattern in [format!(" {sentence}"), format!("{sentence} ")] {
        if text.matches(pattern.as_str()).count() == 1 {
            return Some(text.replacen(pattern.as_str(), "", 1));
        }
    }
    Some(text.replacen(sentence, "", 1))
}

/// Where the placing of a sentence a machine cut ended: a place, no place the
/// records name, or places the records and the text cannot tell apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Placing {
    At(Anchor),
    Unknown,
    Ambiguous,
}

pub(crate) fn put_place(_text: &str, _sentence: &str, _cuts: &[&CutPlace]) -> Placing {
    Placing::Unknown
}

#[cfg(test)]
mod put_place_tests;
