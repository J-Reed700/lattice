//! One rule makes a citation entry, and no edit leaves a citation's sentence blank.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;

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

fn cited() -> Value {
    json!({"blocks":[{"kind":"explanation","body":"Alpha holds the line.","citations":[
        {"sentence":"Alpha holds the line.","sourceIndex":0,"quote":QUOTE}]}],"questions":[]})
}

#[test]
fn an_edit_may_not_blank_a_citation_s_sentence() {
    let edit = json!({"path":"/blocks/0/citations/0/sentence","before":"Alpha holds the line.","after":""});
    let refused = apply(&cited(), &json!({"edits":[edit]}).to_string());
    assert!(refused.is_err(), "{refused:?}");
    let message = refused.unwrap_err().to_string();
    assert!(message.contains("uncite"), "{message}");
}

#[test]
fn the_remedy_for_copied_wording_does_not_offer_quotation_marks() {
    assert_eq!(SAY_IT_YOURSELF, "Say it in the lesson's own words.");
}
