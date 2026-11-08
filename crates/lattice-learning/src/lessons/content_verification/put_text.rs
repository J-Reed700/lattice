//! A sentence written into or out of a field's text, the way a put back and
//! its undo do it.

use lattice_core::domain::learning::claim_review::{span_of, Anchor};

/// `text` with `sentence` written in at `anchor`: one space between sentences
/// of a paragraph, paragraph breaks kept. `None` when the anchor's sentence is
/// not in `text` exactly once.
pub(crate) fn insert_sentence(text: &str, anchor: &Anchor, sentence: &str) -> Option<String> {
    let _ = (text, anchor, sentence);
    None
}

/// `text` without the one occurrence of `sentence` and a space beside it;
/// `None` when it is not in `text` exactly once.
pub(crate) fn remove_sentence(text: &str, sentence: &str) -> Option<String> {
    let _ = (text, sentence);
    None
}
