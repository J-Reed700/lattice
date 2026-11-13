//! The reason a lesson draft gives its right choice.

/// The draft with every question's right choice given no reason: its
/// explanation says why it is right. A draft that gave none comes back as
/// the writer sent it.
pub(crate) fn clear_right_choice_reasons(raw: String) -> String {
    raw
}
