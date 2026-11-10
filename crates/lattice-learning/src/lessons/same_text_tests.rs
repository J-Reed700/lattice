#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;

#[test]
fn text_is_folded_without_markers_case_or_extra_space() {
    assert_eq!(
        fold("  The **Tool**\n\n prints `Two_Words`.  "),
        "the tool prints twowords."
    );
    assert_eq!(
        fold("It is \u{201C}boiling\u{201D} \u{2014} at 100 \u{00B0}C."),
        "it is \"boiling\" - at 100 \u{00B0}c."
    );
    assert_eq!(fold("** _ `"), "");
}

#[test]
fn a_match_maps_back_to_the_bytes_it_came_from() {
    let text = "Say **Hello,\n  World** again.";
    let (start, end) = find(text, "hello, world").unwrap();
    assert_eq!(&text[start..end], "Hello,\n  World");
    assert_eq!(find(text, "   "), None);
    assert_eq!(find(text, "goodbye"), None);
}

#[test]
fn overlap_is_either_text_holding_the_other() {
    assert!(overlap("Run `ls` now.", "run ls now."));
    assert!(overlap("Run ls now. Then stop.", "RUN LS NOW."));
    assert!(!overlap("Run ls now.", "Stop."));
    assert!(!overlap("", "Stop."));
}

#[test]
fn a_whole_sentence_is_held_whatever_its_emphasis_apostrophe_or_case() {
    let text = "It is the tool\u{2019}s name. Then stop.";
    assert!(holds_sentence(text, "It is the **tool's** name."));
    assert!(holds_sentence(text, "THEN STOP."));
    assert!(holds_sentence(text, "Other words here. then stop."));
}

#[test]
fn a_sentence_inside_a_longer_one_is_not_held() {
    assert!(!holds_sentence("There is no.", "No."));
    assert!(!holds_sentence("There is no.", "is no."));
    assert!(!holds_sentence("Some text.", "  "));
}
