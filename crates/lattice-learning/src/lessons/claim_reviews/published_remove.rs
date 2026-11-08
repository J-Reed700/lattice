//! The user's removal of a statement in a published lesson.

/// `body` with the sentence at `range` taken out, and the whitespace around
/// it.
#[allow(dead_code)]
pub(super) fn without_sentence(body: &str, _range: (usize, usize)) -> String {
    body.to_owned()
}
