//! What a section reads like once a sentence is taken out of it.
use super::published_remove::{removal_text, without_sentence};

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

/// What taking `sentence` out of `body` records.
fn recorded(body: &str, sentence: &str) -> (Option<String>, Option<String>) {
    let new = without(body, sentence);
    let (_, follows, precedes) = removal_text(body, &new).unwrap();
    (follows, precedes)
}

const FIVE: &str = "Alpha comes first. Bravo comes second. Charlie comes third. Delta comes fourth. Echo comes last.";

#[test]
fn a_removal_records_the_whole_sentences_either_side_of_it() {
    assert_eq!(
        recorded(FIVE, "Bravo comes second."),
        (
            Some("Alpha comes first. ".into()),
            Some("Charlie comes third.".into())
        )
    );
}

#[test]
fn the_first_sentence_has_no_context_before_it_and_the_whole_next_sentence_after() {
    assert_eq!(
        recorded(FIVE, "Alpha comes first."),
        (None, Some("Bravo comes second.".into()))
    );
}

#[test]
fn the_last_sentence_has_no_context_after_it_and_the_whole_sentence_before() {
    assert_eq!(
        recorded(FIVE, "Echo comes last."),
        (Some("Delta comes fourth.".into()), None)
    );
}

#[test]
fn a_pair_of_neighbours_that_occurs_twice_is_widened_by_a_sentence_each_way() {
    let body = "Same here. Gone now. Other one. Same here. Other one.";
    assert_eq!(
        recorded(body, "Gone now."),
        (
            Some("Same here. ".into()),
            Some("Other one. Same here.".into())
        )
    );
}
