#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;

const COMMAND: &str = "tool sync --all --prune --keep 3 --quiet";

fn command_wording() -> Wording {
    Wording::of_excerpts([COMMAND])
}

fn copied(text: &str, wording: &Wording) -> Vec<Copied> {
    copies(text, &Wording::default(), wording)
}

#[test]
fn a_command_in_inline_code_is_not_the_lessons_wording() {
    let found = copied(
        &format!("Start the sync by typing `{COMMAND}`."),
        &command_wording(),
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn the_same_command_in_prose_is_a_copy() {
    let found = copied(
        &format!("Start the sync by typing {COMMAND}."),
        &command_wording(),
    );
    assert_eq!(found.len(), 1, "{found:?}");
}

#[test]
fn prose_beside_a_code_span_that_is_a_quotable_span_of_the_source_still_copies() {
    let source = Wording::of_excerpts([
        "Start the nightly sync by typing the tool name and pressing enter.",
    ]);
    let sentence = "Start the nightly sync by typing the `tool` name and pressing enter.";
    let found = copied(sentence, &source);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].sentence, sentence);
    assert_eq!(found[0].words, "Start the nightly sync by typing the");
}

#[test]
fn a_few_words_of_prose_around_code_spans_are_no_copy() {
    let source = Wording::of_excerpts(["Run tool sync --all then tool prune --keep 3 today."]);
    let found = copied(
        "Run `tool sync --all` then `tool prune --keep 3` today.",
        &source,
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_span_written_with_double_backticks_holds_a_single_backtick() {
    let found = copied(
        "Then type ``tool `sync` --all --prune --keep 3`` and wait.",
        &Wording::of_excerpts(["tool sync --all --prune --keep 3"]),
    );
    assert!(found.is_empty(), "{found:?}");
}
