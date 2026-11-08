//! A sentence written into or out of a field's text, the way a put back and
//! its undo do it.

use lattice_core::domain::learning::claim_review::{span_of, Anchor};

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
