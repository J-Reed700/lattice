//! A finding about a unit is repaired as a whole: the key,
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
    ReviewFinding::reviewer(1, Some(site), "the key is wrong").with_passage(passage)
}

fn kept(
    previous: &Value,
    repaired: &Value,
    finding: ReviewFinding,
    protected: &[Protected],
) -> Value {
    let part = finding.site.and_then(FindingSite::pointer).unwrap();
    let touched = std::collections::BTreeMap::from([(part, vec![&finding])]);
    let out = protected::confine_review(previous, repaired, &touched, protected)
        .unwrap()
        .expect("the repair stays inside what its finding may change");
    assert_ne!(&out, previous, "the confined repair kept nothing");
    out
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
fn a_question_repair_that_rewrites_a_protected_sentence_is_refused_with_its_reason() {
    let previous = question_lesson();
    let mut repaired = whole_question_repair(&previous);
    repaired["questions"][0]["explanation"] = json!("Only ls lists a folder.");
    let protected = vec![Protected {
        unit: 0,
        field: "explanation".into(),
        quote: PROTECTED.into(),
        released: false,
    }];
    let named = finding(FindingSite::Question(0), PROMPT);
    let touched = std::collections::BTreeMap::from([("/questions/0".to_owned(), vec![&named])]);
    let reasons = protected::confine_review(&previous, &repaired, &touched, &protected)
        .unwrap()
        .expect_err("the repair rewrote a protected sentence");
    assert!(reasons.contains(&format!(
        "it changes \"{PROTECTED}\", which states a checked claim and must stay as it is"
    )));
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

const CASE: &str = "Run ls on the file notes.txt in the projects folder.";
const STEP_ONE: &str = "First, type ls notes.txt and press enter.";
const STEP_TWO: &str = "Then check that notes.txt is listed.";
const OTHER: &str = "The cd command changes folders.";

fn worked_lesson() -> Value {
    json!({"blocks":[
        {"kind":"worked_example","title":"Example","body":format!("{CASE} {STEP_ONE} {STEP_TWO} {PROTECTED}"),"rubric":[],"sourceIndex":0,"quote":"ls shows the files."},
        {"kind":"explanation","title":"Other","body":OTHER,"rubric":[],"sourceIndex":0,"quote":"ls shows the files."}
    ],"questions":[]})
}

fn renamed_repair(previous: &Value) -> Value {
    let mut repaired = previous.clone();
    repaired["blocks"][0]["body"] = json!(format!(
        "{} {} {} {PROTECTED}",
        CASE.replace("notes.txt", "report.txt"),
        STEP_ONE.replace("notes.txt", "report.txt"),
        STEP_TWO.replace("notes.txt", "report.txt")
    ));
    repaired
}

#[test]
fn a_block_finding_on_its_case_sentence_keeps_the_rename_in_the_steps_too() {
    let previous = worked_lesson();
    let repaired = renamed_repair(&previous);
    let protected = vec![Protected {
        unit: 0,
        field: "body".into(),
        quote: PROTECTED.into(),
        released: false,
    }];
    let out = kept(
        &previous,
        &repaired,
        finding(FindingSite::Section(0), CASE),
        &protected,
    );
    assert_eq!(out["blocks"][0]["body"], repaired["blocks"][0]["body"]);
}

#[test]
fn a_repair_that_also_changes_another_unit_is_refused_with_its_reason() {
    let previous = worked_lesson();
    let mut repaired = renamed_repair(&previous);
    repaired["blocks"][1]["body"] = json!("The cd command moves between folders.");
    let protected = vec![Protected {
        unit: 0,
        field: "body".into(),
        quote: PROTECTED.into(),
        released: false,
    }];
    let named = finding(FindingSite::Section(0), CASE);
    let touched = std::collections::BTreeMap::from([("/blocks/0".to_owned(), vec![&named])]);
    let reasons = protected::confine_review(&previous, &repaired, &touched, &protected)
        .unwrap()
        .expect_err("the repair changed a unit its finding is not about");
    assert_eq!(
        reasons,
        vec![format!(
            "it changes \"{OTHER}\", which is outside what the correction was asked to change"
        )]
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

fn cited_lesson() -> Value {
    let mut lesson = worked_lesson();
    lesson["blocks"][0]["kind"] = json!("explanation");
    lesson["blocks"][0]["citations"] = json!([]);
    lesson
}

fn uncited(site: FindingSite) -> ReviewFinding {
    ReviewFinding::rule(
        Rule::Uncited,
        FindingKind::Quotation,
        site,
        "states a detail without citing it",
    )
}

#[test]
fn a_quotation_finding_with_no_passage_keeps_an_appended_citation() {
    let previous = cited_lesson();
    let mut repaired = previous.clone();
    repaired["blocks"][0]["citations"] =
        json!([{"sentence":CASE,"sourceIndex":0,"quote":"ls shows the files."}]);
    let out = kept(&previous, &repaired, uncited(FindingSite::Section(0)), &[]);
    assert_eq!(
        out["blocks"][0]["citations"],
        repaired["blocks"][0]["citations"]
    );
}

#[test]
fn a_quotation_finding_keeps_a_citation_and_a_sentence_written_in_its_general_form() {
    let previous = cited_lesson();
    let mut repaired = previous.clone();
    repaired["blocks"][0]["citations"] =
        json!([{"sentence":CASE,"sourceIndex":0,"quote":"ls shows the files."}]);
    repaired["blocks"][0]["body"] = json!(format!(
        "Run ls on a file in the projects folder. {STEP_ONE} {STEP_TWO} {PROTECTED}"
    ));
    let protected = vec![Protected {
        unit: 0,
        field: "body".into(),
        quote: PROTECTED.into(),
        released: false,
    }];
    let out = kept(
        &previous,
        &repaired,
        uncited(FindingSite::Section(0)),
        &protected,
    );
    assert_eq!(out["blocks"][0], repaired["blocks"][0]);
}
