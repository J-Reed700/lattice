#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
//! What a section reads like once a sentence is taken out of it.
use super::published_remove::without_sentence;

/// `body` without its only occurrence of `sentence`.
fn without(body: &str, sentence: &str) -> String {
    let start = body.find(sentence).unwrap();
    without_sentence(body, (start, start + sentence.len()))
}

#[test]
fn the_first_sentence_goes_with_the_space_after_it() {
    assert_eq!(
        without("A one. B two. C three.", "A one."),
        "B two. C three."
    );
}

#[test]
fn a_middle_sentence_leaves_one_space() {
    assert_eq!(
        without("A one. B two. C three.", "B two."),
        "A one. C three."
    );
}

#[test]
fn the_last_sentence_goes_with_the_space_before_it() {
    assert_eq!(
        without("A one. B two. C three.", "C three."),
        "A one. B two."
    );
}

#[test]
fn a_whole_paragraph_goes_with_one_blank_line() {
    assert_eq!(
        without("A one.\n\nB two.\n\nC three.", "B two."),
        "A one.\n\nC three."
    );
}

#[test]
fn a_sentence_that_starts_a_paragraph_keeps_the_paragraph_break() {
    assert_eq!(
        without("A one.\n\nB two. C three.", "B two."),
        "A one.\n\nC three."
    );
}

#[test]
fn a_sentence_that_ends_a_paragraph_keeps_the_paragraph_break() {
    assert_eq!(
        without("A one. B two.\n\nC three.", "B two."),
        "A one.\n\nC three."
    );
}

#[test]
fn a_list_item_goes_whole_with_its_marker() {
    assert_eq!(
        without("- A one.\n- B two.\n- C three.", "B two."),
        "- A one.\n- C three."
    );
}

#[test]
fn the_last_list_item_goes_with_its_line_break() {
    assert_eq!(without("- A one.\n- B two.", "B two."), "- A one.");
}

#[test]
fn the_last_paragraph_goes_with_the_blank_line_before_it() {
    assert_eq!(without("A one.\n\nB two.", "B two."), "A one.");
}

#[test]
fn the_first_paragraph_goes_with_the_blank_line_after_it() {
    assert_eq!(without("A one.\n\nB two.", "A one."), "B two.");
}

#[test]
fn the_only_sentence_leaves_an_empty_section() {
    assert_eq!(without("A one.", "A one."), "");
}

#[test]
fn text_with_windows_line_breaks_keeps_its_paragraph_break() {
    assert_eq!(
        without("A one.\r\n\r\nB two.\r\n\r\nC three.", "B two."),
        "A one.\r\n\r\nC three."
    );
}
