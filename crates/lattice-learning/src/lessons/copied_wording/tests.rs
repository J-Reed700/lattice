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

#[test]
fn a_quotation_that_holds_inline_code_stays_a_quotation() {
    let source = Wording::of_excerpts([
        "Start the nightly sync by typing tool sync and wait for the summary line to appear.",
    ]);
    let found = copied(
        "The guide says \"start the nightly sync by typing `tool sync` and wait for the summary line to appear\".",
        &source,
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_quotation_mark_inside_inline_code_opens_no_quotation() {
    let source = Wording::of_excerpts([
        "Start the nightly sync by typing the tool name and pressing enter.",
    ]);
    let found = copied(
        "Type `say \"` first, then start the nightly sync by typing the tool name and pressing enter.",
        &source,
    );
    assert_eq!(found.len(), 1, "{found:?}");
}

#[test]
fn a_quoted_span_beside_a_code_span_leaves_only_the_own_words() {
    let source = Wording::of_excerpts([
        "Start the nightly sync by typing the tool name and pressing enter.",
    ]);
    let found = copied(
        "The guide says \"start the nightly sync by typing the tool name and pressing enter\" then run `tool sync` to see it.",
        &source,
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_source_line_with_too_few_words_to_quote_is_no_wording() {
    // A table row: long enough as written, two words once its marks are set aside.
    let source = Wording::of_excerpts(["| ^ | - | upper shelf | - |"]);
    let found = copied(
        "Put the jar back on the upper shelf when you are done.",
        &source,
    );
    assert!(found.is_empty(), "{found:?}");
}
