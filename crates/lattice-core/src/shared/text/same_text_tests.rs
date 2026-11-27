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
fn every_place_a_needle_stands_is_found() {
    let text = "Say **Hello** and HELLO.";
    let hits = find_all(text, "hello");
    assert_eq!(hits.len(), 2);
    assert_eq!(&text[hits[0].0..hits[0].1], "Hello");
    assert_eq!(&text[hits[1].0..hits[1].1], "HELLO");
    assert!(find_all(text, "goodbye").is_empty());
    assert!(find_all(text, "  ").is_empty());
}

#[test]
fn the_sentences_a_span_lies_in_are_whole_and_a_span_in_none_stands_for_itself() {
    let text = "Alpha is first. Beta **is** second. Gamma ends it.";
    let span = find(text, "is second").unwrap();
    assert_eq!(exact_sentences(text, span), Some("Beta **is** second."));
    let across = find(text, "first. beta").unwrap();
    assert_eq!(
        exact_sentences(text, across),
        Some("Alpha is first. Beta **is** second.")
    );
    assert_eq!(sentences_over("", (0, 0)), vec![(0, 0)]);
}

#[test]
fn a_whole_sentence_is_held_whatever_its_emphasis_apostrophe_or_case() {
    let text = "It is the tool\u{2019}s name. Then stop.";
    assert!(holds_sentence(text, "It is the **tool's** name."));
    assert!(holds_sentence(text, "THEN STOP."));
    assert!(holds_sentence(text, "Other words here. THEN stop."));
}

#[test]
fn a_sentence_inside_a_longer_one_is_not_held() {
    assert!(!holds_sentence("There is no.", "No."));
    assert!(!holds_sentence("There is no.", "is no."));
    assert!(!holds_sentence("Some text.", "  "));
}

#[test]
fn recorded_text_stands_byte_for_byte_and_a_blank_quote_nowhere() {
    assert!(stands("It is **bold**.", "**bold**"));
    assert!(!stands("It is **bold**.", "bold!"));
    assert!(!stands("It is bold.", "It Is"));
    assert!(!stands("It is bold.", "   "));
    assert!(!stands("It is bold.", ""));
}

#[test]
fn spans_overlap_only_when_they_share_a_byte() {
    assert!(overlaps((0, 5), (4, 9)));
    assert!(overlaps((2, 3), (0, 9)));
    assert!(!overlaps((0, 5), (5, 9)));
    assert!(!overlaps((3, 3), (0, 9)));
}

#[test]
fn a_common_prefix_ends_on_a_character_boundary() {
    assert_eq!(common_prefix("abcd", "abxy"), 2);
    assert_eq!(common_prefix("日本語", "日本人"), 6);
    assert_eq!(common_prefix("abc", "abc"), 3);
    assert_eq!(common_prefix("abc", ""), 0);
}

#[test]
fn a_span_is_whole_sentences_only_from_a_start_to_an_end() {
    let text = "Alpha is first. Beta is second. Gamma.";
    assert!(whole_sentences(text, (0, 15)));
    assert!(whole_sentences(text, (0, 31)));
    assert!(whole_sentences(text, (16, 31)));
    assert!(!whole_sentences(text, (6, 15)));
    assert!(!whole_sentences(text, (0, 20)));
    assert!(!whole_sentences("", (0, 0)));
}

#[test]
fn a_quotation_needs_twenty_five_characters() {
    let text = "Say: Show the folder name no. Then: Show the folder name now. Done.";
    assert_eq!(quotation(text, "Show the folder name no."), None);
    let (start, end) = quotation(text, "Show the folder name now.").unwrap();
    assert_eq!(&text[start..end], "Show the folder name now.");
}

#[test]
fn a_quotation_needs_five_words() {
    let text = "Then Extraordinarily long words only, and more words follow.";
    assert_eq!(quotation(text, "Extraordinarily long words only"), None);
    let (start, end) = quotation(text, "Extraordinarily long words only, and").unwrap();
    assert_eq!(&text[start..end], "Extraordinarily long words only, and");
}

#[test]
fn a_quotation_is_found_apart_from_case_and_curly_quotes() {
    let text = "He said, \u{201C}Show the **Folder** name now.\u{201D} and left.";
    let (start, end) = quotation(text, "  show the folder name now.  ").unwrap();
    assert_eq!(&text[start..end], "Show the **Folder** name now.");
    let curly = "It is the writer\u{2019}s own long sentence here.";
    assert!(quotation(curly, "the writer's own long sentence here").is_some());
}

#[test]
fn a_long_enough_quote_that_is_not_there_is_not_found() {
    let text = "Show the folder name now, then leave.";
    assert_eq!(quotation(text, "Open the folder name now please."), None);
}

#[test]
fn a_quotation_joins_hyphenated_line_breaks_and_normalizes_ligatures() {
    let text =
        "The supplier\nshall main-\ntain the \u{201C}\u{FB01}nal\u{201D} register at all sites.";
    let (start, end) =
        quotation(text, "The supplier shall maintain the \"final\" register").unwrap();
    assert_eq!(
        &text[start..end],
        "The supplier\nshall main-\ntain the \u{201C}\u{FB01}nal\u{201D} register"
    );
}
