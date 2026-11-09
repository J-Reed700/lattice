//! A finding about a question or a check is repaired as a whole: the key,
//! the options, the explanation and the citations change together.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::lessons::content_verification::Protected;

const PROMPT: &str = "Which command lists the files in the projects folder?";
const PROTECTED: &str = "The projects folder holds notes.txt and nothing else.";

fn question_lesson() -> Value {
    json!({"blocks":[],"questions":[{
        "prompt": PROMPT,
        "options": ["cd projects", "cat projects", "ls projects", "rm projects"],
        "correctIndex": 0,
        "explanation": format!("The first option changes directory and lists nothing. {PROTECTED}"),
        "citations": [{"sentence":"Listing a folder shows its files.","sourceIndex":0,"quote":"ls shows the files."}]
    }]})
}

fn check_lesson() -> Value {
    json!({"blocks":[{"kind":"check","title":"Check","body":"What does ls print for the projects folder?\n\n### Check your thinking\nIt prints nothing. The folder is hidden.","rubric":[],"sourceIndex":0,"quote":"ls shows the files."}],"questions":[]})
}

fn finding(site: FindingSite, passage: &str) -> ReviewFinding {
    ReviewFinding::new(FindingKind::Teaching, Some(site), "the key is wrong").with_passage(passage)
}

fn kept(
    previous: &Value,
    repaired: &Value,
    finding: ReviewFinding,
    protected: &[Protected],
) -> Value {
    let (text, _) =
        protected::confine_review(previous, &repaired.to_string(), &[finding], protected).unwrap();
    assert!(text.is_some(), "the confined repair kept nothing");
    serde_json::from_str(&text.unwrap()).unwrap()
}

fn whole_question_repair(previous: &Value) -> Value {
    let mut repaired = previous.clone();
    repaired["questions"][0]["correctIndex"] = json!(2);
    repaired["questions"][0]["options"][2] = json!("ls projects/");
    repaired["questions"][0]["explanation"] = json!(format!("Only ls lists a folder. {PROTECTED}"));
    repaired
}

#[test]
fn a_question_finding_on_the_prompt_keeps_a_changed_key_option_and_explanation() {
    let previous = question_lesson();
    let repaired = whole_question_repair(&previous);
    let out = kept(
        &previous,
        &repaired,
        finding(FindingSite::Question(0), PROMPT),
        &[],
    );
    assert_eq!(out["questions"][0], repaired["questions"][0]);
}

#[test]
fn a_question_repair_keeps_its_changes_and_leaves_a_protected_sentence_byte_identical() {
    let previous = question_lesson();
    let repaired = whole_question_repair(&previous);
    let protected = vec![Protected {
        unit: 0,
        field: "explanation".into(),
        quote: PROTECTED.into(),
        released: false,
    }];
    let out = kept(
        &previous,
        &repaired,
        finding(FindingSite::Question(0), PROMPT),
        &protected,
    );
    assert_eq!(out["questions"][0]["correctIndex"], json!(2));
    assert_eq!(out["questions"][0]["options"][2], json!("ls projects/"));
    assert!(out["questions"][0]["explanation"]
        .as_str()
        .unwrap()
        .contains(PROTECTED));
}

#[test]
fn a_question_repair_that_rewrites_a_protected_sentence_keeps_it_as_it_was() {
    let previous = question_lesson();
    let mut repaired = whole_question_repair(&previous);
    repaired["questions"][0]["explanation"] = json!("Only ls lists a folder.");
    let protected = vec![Protected {
        unit: 0,
        field: "explanation".into(),
        quote: PROTECTED.into(),
        released: false,
    }];
    let result = protected::confine_review(
        &previous,
        &repaired.to_string(),
        &[finding(FindingSite::Question(0), PROMPT)],
        &protected,
    );
    if let Ok((Some(text), _)) = result {
        let out: Value = serde_json::from_str(&text).unwrap();
        assert!(out["questions"][0]["explanation"]
            .as_str()
            .unwrap()
            .contains(PROTECTED));
        assert_eq!(out["questions"][0]["correctIndex"], json!(2));
    }
}

#[test]
fn a_check_finding_on_its_question_keeps_the_rewritten_answer() {
    let previous = check_lesson();
    let mut repaired = previous.clone();
    repaired["blocks"][0]["body"] = json!("What does ls print for the projects folder?\n\n### Check your thinking\nIt prints notes.txt. The folder is not hidden.");
    let out = kept(
        &previous,
        &repaired,
        finding(
            FindingSite::Section(0),
            "What does ls print for the projects folder?",
        ),
        &[],
    );
    assert_eq!(out["blocks"][0]["body"], repaired["blocks"][0]["body"]);
}

#[test]
fn an_explanation_finding_with_a_passage_stays_confined_to_it() {
    let mut previous = check_lesson();
    previous["blocks"][0]["kind"] = json!("explanation");
    previous["blocks"][0]["body"] = json!(
        "The ls command lists files. The cd command changes folders. The rm command deletes."
    );
    let mut repaired = previous.clone();
    repaired["blocks"][0]["body"] = json!(
        "The ls command lists the files. The cd command moves between folders. The rm command deletes."
    );
    let out = kept(
        &previous,
        &repaired,
        finding(FindingSite::Section(0), "The ls command lists files."),
        &[],
    );
    assert_eq!(
        out["blocks"][0]["body"],
        json!("The ls command lists the files. The cd command changes folders. The rm command deletes.")
    );
}

#[test]
fn a_question_finding_keeps_an_edited_citation() {
    let previous = question_lesson();
    let mut repaired = previous.clone();
    repaired["questions"][0]["correctIndex"] = json!(2);
    repaired["questions"][0]["citations"][0]["quote"] = json!("ls prints the names of the files.");
    let out = kept(
        &previous,
        &repaired,
        finding(FindingSite::Question(0), PROMPT),
        &[],
    );
    assert_eq!(
        out["questions"][0]["citations"],
        repaired["questions"][0]["citations"]
    );
}
