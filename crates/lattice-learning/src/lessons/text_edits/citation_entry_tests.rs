//! One rule makes a citation entry.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use serde_json::{json, Value};

const QUOTE: &str = "Alpha quote text.";

fn block() -> Value {
    json!({"kind":"explanation","title":"Alpha title.","body":"Alpha holds the line. Beta follows it."})
}

fn question() -> Value {
    json!({"kind":"quiz","prompt":"Which?","explanation":"Alpha holds the line.",
        "rationales":["Beta follows it.",""]})
}

#[test]
fn an_entry_holds_the_exact_sentence_of_the_part() {
    let entry = citation_entry(&block(), false, "Alpha holds the line.", json!(0), QUOTE);
    assert_eq!(
        entry,
        Some(json!({"sentence":"Alpha holds the line.","sourceIndex":0,"quote":QUOTE}))
    );
    let inside = citation_entry(&block(), false, "holds the line", json!(1), QUOTE);
    assert_eq!(
        inside,
        Some(json!({"sentence":"Alpha holds the line.","sourceIndex":1,"quote":QUOTE}))
    );
}

#[test]
fn no_entry_is_made_for_a_blank_sentence() {
    for blank in ["", " ", "\n\t "] {
        assert_eq!(
            citation_entry(&block(), false, blank, json!(0), QUOTE),
            None,
            "{blank:?}"
        );
    }
}

#[test]
fn no_entry_is_made_for_a_sentence_outside_the_part_s_citable_texts() {
    assert_eq!(
        citation_entry(&block(), false, "Alpha title.", json!(0), QUOTE),
        None
    );
    assert_eq!(
        citation_entry(&block(), false, "Gamma stands apart.", json!(0), QUOTE),
        None
    );
    assert!(citation_entry(&question(), true, "Beta follows it.", json!(0), QUOTE).is_some());
    assert_eq!(
        citation_entry(&question(), false, "Beta follows it.", json!(0), QUOTE),
        None
    );
}
