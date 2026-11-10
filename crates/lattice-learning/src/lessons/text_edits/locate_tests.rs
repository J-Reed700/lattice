//! An edit's `before` and a citation's sentence are located in the lesson
//! under the crate's one text identity, and a refusal gives the lesson's own
//! wording to copy.
use super::*;

const MARKED: &str = "The **Tool** prints the value.";
const PLAIN: &str = "The Tool prints the value.";

fn body() -> Value {
    json!({"blocks":[
        {"kind":"explanation","body":format!("Alpha is first. {MARKED} Omega ends it."),"citations":[]},
        {"kind":"explanation","body":"Gamma stands here. The **second** part sits in this body.","citations":[]}
    ],"questions":[]})
}

fn refused(candidate: &Value, edits: Value) -> String {
    apply(candidate, &json!({ "edits": edits }).to_string())
        .unwrap_err()
        .to_string()
}

fn cite(path: &str, sentence: &str) -> String {
    json!({"edits":[],"citations":[{"path":path,"sentence":sentence,"sourceIndex":0,"quote":"The source states the value is printed."}]}).to_string()
}

#[test]
fn an_edit_that_drops_the_emphasis_markers_is_refused_with_the_exact_sentence() {
    let reason = refused(
        &body(),
        json!([{"path":"/blocks/0/body","before":PLAIN,"after":"The Tool shows the value."}]),
    );
    assert!(reason.contains(&format!("\"{MARKED}\"")), "{reason}");
}

#[test]
fn an_edit_naming_one_field_whose_words_stand_in_another_is_refused_naming_that_field() {
    let reason = refused(
        &body(),
        json!([{"path":"/blocks/0/body","before":"The second part sits in this body.","after":"The part sits here."}]),
    );
    assert!(reason.contains("/blocks/1/body"), "{reason}");
    assert!(
        reason.contains("\"Gamma stands here. The **second** part sits in this body.\"")
            || reason.contains("\"The **second** part sits in this body.\""),
        "{reason}"
    );
}

#[test]
fn a_citation_with_a_straight_apostrophe_cites_the_lessons_own_sentence() {
    let mut original = body();
    original["blocks"][0]["body"] =
        json!("Alpha is first. The tool\u{2019}s timeout is 30 seconds. Omega ends it.");
    let revised = apply(
        &original,
        &cite("/blocks/0", "The tool's timeout is 30 seconds."),
    );
    assert!(revised.is_ok(), "{revised:?}");
    assert_eq!(
        revised.unwrap()["blocks"][0]["citations"][0]["sentence"],
        "The tool\u{2019}s timeout is 30 seconds."
    );
}

#[test]
fn a_citation_without_the_emphasis_markers_cites_the_lessons_own_sentence() {
    let revised = apply(&body(), &cite("/blocks/0", &PLAIN.to_lowercase()));
    assert!(revised.is_ok(), "{revised:?}");
    assert_eq!(
        revised.unwrap()["blocks"][0]["citations"][0]["sentence"],
        MARKED
    );
}

#[test]
fn an_exact_unique_before_applies_unchanged() {
    let revised = apply(
        &body(),
        &json!({"edits":[{"path":"/blocks/0/body","before":MARKED,"after":"The Tool shows the value."}]}).to_string(),
    );
    assert!(revised.is_ok(), "{revised:?}");
    assert_eq!(
        revised.unwrap()["blocks"][0]["body"],
        "Alpha is first. The Tool shows the value. Omega ends it."
    );
}

#[test]
fn a_before_whose_words_are_nowhere_keeps_the_plain_reason() {
    let reason = refused(
        &body(),
        json!([{"path":"/blocks/0/body","before":"Nothing like this.","after":"Something."}]),
    );
    assert!(
        reason.contains("the before text of edit 0 does not occur in /blocks/0/body.")
            && !reason.contains("stand"),
        "{reason}"
    );
}

#[test]
fn a_citation_of_a_sentence_the_part_lacks_keeps_its_error() {
    let reason = apply(&body(), &cite("/blocks/0", "A sentence nobody wrote."))
        .unwrap_err()
        .to_string();
    assert!(
        reason.contains("names a sentence that is not in the body of /blocks/0; copy it exactly."),
        "{reason}"
    );
}
