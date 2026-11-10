//! What counts as a sentence the lesson says twice, and as source wording the
//! lesson copies.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use crate::generation::lesson_rules;
use crate::review_findings::{FindingKind, FindingSite, ReviewFinding};
use serde_json::{json, Value};

const QUOTE: &str = "All it does is print out the current folder name.";

fn section(title: &str, body: &str) -> Value {
    json!({"kind":"explanation","title":title,"body":body,"quote":"","citations":[]})
}

fn lesson(blocks: Vec<Value>) -> Value {
    json!({"blocks":blocks,"questions":[]})
}

fn teaching(candidate: &Value) -> Vec<ReviewFinding> {
    lesson_rules(candidate)
        .into_iter()
        .filter(|finding| finding.kind == FindingKind::Teaching)
        .collect()
}

fn cites(quote: &str) -> Value {
    json!([{"sentence":"A sentence of the lesson.","sourceIndex":0,"quote":quote}])
}

#[test]
fn a_command_on_a_line_of_its_own_is_no_sentence_the_lesson_repeats() {
    let candidate = lesson(vec![
        section("First step", "Look at the folder.\n\n`ls`"),
        section("Second step", "Look again.\n\n`ls`"),
    ]);
    let found = teaching(&candidate);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_prose_sentence_holding_code_is_still_a_repeat() {
    const SHARED: &str = "Run `ls` to list the folder.";
    let candidate = lesson(vec![
        section("First step", &format!("Begin. {SHARED}")),
        section("Second step", &format!("Again. {SHARED}")),
    ]);
    let found = teaching(&candidate);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].key().site, FindingSite::Section(1));
    assert_eq!(found[0].passage.as_deref(), Some(SHARED));
}

#[test]
fn a_sentence_equal_to_its_citations_quote_copies_the_source() {
    let mut block = section("Printing", QUOTE);
    block["citations"] = cites(QUOTE);
    let found = teaching(&lesson(vec![block]));
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].key().site, FindingSite::Section(0));
    assert!(
        found[0].note.contains("double quotation marks"),
        "{}",
        found[0].note
    );
}

#[test]
fn a_sentence_holding_the_quote_after_other_words_copies_the_source() {
    let body = format!("The command prints that location: {QUOTE}");
    let mut block = section("Printing", &body);
    block["citations"] = cites(QUOTE);
    let found = teaching(&lesson(vec![block]));
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].key().site, FindingSite::Section(0));
}

#[test]
fn a_sentence_inside_a_longer_quote_copies_the_source() {
    let mut block = section("Printing", "It prints the current folder name.");
    block["citations"] = cites("It prints the current folder name and then exits.");
    let found = teaching(&lesson(vec![block]));
    assert_eq!(found.len(), 1, "{found:?}");
}

#[test]
fn a_sections_own_quote_is_held_to_the_same_rule() {
    let mut block = section("Printing", QUOTE);
    block["quote"] = json!(QUOTE);
    let found = teaching(&lesson(vec![block]));
    assert_eq!(found.len(), 1, "{found:?}");
}

#[test]
fn the_sources_words_in_double_quotation_marks_are_no_copy() {
    let body = format!(
        "The manual puts it this way: \"{QUOTE}\" The lesson says it again in its own words."
    );
    let mut block = section("Printing", &body);
    block["citations"] = cites(QUOTE);
    let found = teaching(&lesson(vec![block]));
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_sentence_that_says_it_differently_is_no_copy() {
    let mut block = section(
        "Printing",
        "The command shows the name of the folder you are in.",
    );
    block["citations"] = cites(QUOTE);
    let found = teaching(&lesson(vec![block]));
    assert!(found.is_empty(), "{found:?}");
}

fn question(explanation: &str, reasons: [&str; 4]) -> Value {
    json!({"kind":"practice","prompt":"Which choice prints the folder name?",
        "options":["One","Two","Three","Four"],"correctIndex":0,
        "rationales":reasons,"explanation":explanation,"quote":"","citations":cites(QUOTE)})
}

#[test]
fn a_question_explanation_or_choice_reason_copying_the_quote_is_a_finding_at_the_question() {
    for (explanation, reasons) in [
        (
            QUOTE,
            ["", "Two is wrong.", "Three is wrong.", "Four is wrong."],
        ),
        ("Because.", ["", QUOTE, "Three is wrong.", "Four is wrong."]),
    ] {
        let mut candidate = lesson(vec![section("Printing", "Body of printing.")]);
        candidate["questions"] = json!([question(explanation, reasons)]);
        let found = teaching(&candidate);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].key().site, FindingSite::Question(0));
    }
}
