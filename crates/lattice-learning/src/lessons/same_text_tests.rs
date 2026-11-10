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
