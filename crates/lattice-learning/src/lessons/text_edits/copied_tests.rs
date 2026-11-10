//! A repair may not write a source's words into the lesson's own sentence.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;

const QUOTE: &str = "All it does is print out the current folder name.";

fn cited() -> Value {
    json!({"blocks":[{"kind":"explanation","body":"Alpha is first. The result is stated here.",
        "citations":[{"sentence":"Alpha is first.","sourceIndex":0,"quote":QUOTE}]}],"questions":[]})
}

fn rewrite(after: &str) -> Result<Value> {
    let edit = json!({"path":"/blocks/0/body","before":"The result is stated here.","after":after});
    apply(&cited(), &json!({"edits":[edit]}).to_string())
}

#[test]
fn an_edit_adding_a_sentence_equal_to_its_citations_quote_is_refused() {
    let refused = rewrite(QUOTE);
    assert!(refused.is_err(), "{refused:?}");
    let message = refused.unwrap_err().to_string();
    assert!(message.contains(QUOTE), "{message}");
    assert!(message.contains("double quotation marks"), "{message}");
}

#[test]
fn an_edit_adding_the_quote_after_other_words_is_refused() {
    assert!(rewrite(&format!("The command prints that location: {QUOTE}")).is_err());
}

#[test]
fn an_edit_in_the_lessons_own_words_or_with_the_quote_marked_is_applied() {
    let own = rewrite("The command shows the name of the folder you are in.").unwrap();
    assert_eq!(
        own["blocks"][0]["body"],
        "Alpha is first. The command shows the name of the folder you are in."
    );
    let quoted = rewrite(&format!("The manual says \"{QUOTE}\"")).unwrap();
    assert!(quoted["blocks"][0]["body"]
        .as_str()
        .unwrap()
        .contains(QUOTE));
}

#[test]
fn a_copy_the_field_already_held_does_not_refuse_an_edit_beside_it() {
    let mut candidate = cited();
    candidate["blocks"][0]["body"] = json!(format!("{QUOTE} The result is stated here."));
    let edit = json!({"path":"/blocks/0/body","before":"stated here","after":"shown here"});
    let applied = apply(&candidate, &json!({"edits":[edit]}).to_string()).unwrap();
    assert!(applied["blocks"][0]["body"]
        .as_str()
        .unwrap()
        .ends_with("The result is shown here."));
}
