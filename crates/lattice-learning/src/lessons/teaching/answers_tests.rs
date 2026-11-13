//! A repair's answer, one entry for each finding, as the flat patch the edits apply from.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::answers::{read, reread};
use serde_json::{json, Value};

fn sent() -> Vec<String> {
    vec!["reviewer/1".to_owned(), "reviewer/2".to_owned()]
}

fn retitle(before: &str, after: &str) -> Value {
    json!({"path":"/blocks/0/title","before":before,"after":after})
}

fn entry(finding: &str, plan: &str, edits: Value) -> Value {
    json!({"finding":finding,"plan":plan,"edits":edits})
}

fn answer(entries: Vec<Value>) -> String {
    json!({ "answers": entries }).to_string()
}

fn refusal(raw: &str) -> String {
    read(raw, &sent()).err().unwrap().to_string()
}

#[test]
fn every_change_of_an_entry_is_flattened_with_the_finding_of_its_entry() {
    let cite = json!({"path":"/blocks/0","sentence":"A kettle boils.","sourceIndex":0,"quote":"Water boils in a kettle."});
    let raw = answer(vec![
        json!({"finding":"reviewer/1","plan":"Retitle it.","edits":[retitle("Kettle","Kettles")],"citations":[cite.clone()],"uncite":[{"path":"/blocks/0/citations/0"}]}),
        entry(
            "reviewer/2",
            "Retitle the shelf.",
            json!([retitle("Shelf", "Shelves"), retitle("Rope", "Ropes")]),
        ),
    ]);
    let answered = read(&raw, &sent()).unwrap();
    let patch: Value = serde_json::from_str(&answered.patch).unwrap();
    assert_eq!(
        patch,
        json!({
            "edits":[
                {"finding":"reviewer/1","path":"/blocks/0/title","before":"Kettle","after":"Kettles"},
                {"finding":"reviewer/2","path":"/blocks/0/title","before":"Shelf","after":"Shelves"},
                {"finding":"reviewer/2","path":"/blocks/0/title","before":"Rope","after":"Ropes"}
            ],
            "citations":[
                {"finding":"reviewer/1","path":"/blocks/0","sentence":"A kettle boils.","sourceIndex":0,"quote":"Water boils in a kettle."}
            ],
            "uncite":[{"finding":"reviewer/1","path":"/blocks/0/citations/0"}]
        })
    );
    assert_eq!(
        answered.plans.into_iter().collect::<Vec<_>>(),
        [
            ("reviewer/1".to_owned(), "Retitle it.".to_owned()),
            ("reviewer/2".to_owned(), "Retitle the shelf.".to_owned())
        ]
    );
}

#[test]
fn an_entry_with_no_change_keeps_its_plan_and_adds_nothing_to_the_patch() {
    let raw = answer(vec![
        entry(
            "reviewer/1",
            "Retitle it.",
            json!([retitle("Kettle", "Kettles")]),
        ),
        entry(
            "reviewer/2",
            "Beads are already right as written.",
            json!([]),
        ),
    ]);
    let answered = read(&raw, &sent()).unwrap();
    let patch: Value = serde_json::from_str(&answered.patch).unwrap();
    assert_eq!(patch["edits"].as_array().unwrap().len(), 1);
    assert_eq!(
        answered.plans.get("reviewer/2").map(String::as_str),
        Some("Beads are already right as written.")
    );
}

#[test]
fn an_answer_that_leaves_a_finding_out_is_refused_naming_it() {
    let reason = refusal(&answer(vec![entry(
        "reviewer/1",
        "Retitle it.",
        json!([retitle("Kettle", "Kettles")]),
    )]));
    assert!(reason.contains("reviewer/2"), "{reason}");
    assert!(!reason.contains("reviewer/1"), "{reason}");
}

#[test]
fn an_answer_that_names_a_finding_twice_is_refused_naming_it() {
    let reason = refusal(&answer(vec![
        entry(
            "reviewer/1",
            "Retitle it.",
            json!([retitle("Kettle", "Kettles")]),
        ),
        entry(
            "reviewer/2",
            "Retitle the shelf.",
            json!([retitle("Shelf", "Shelves")]),
        ),
        entry(
            "reviewer/1",
            "Retitle it again.",
            json!([retitle("Rope", "Ropes")]),
        ),
    ]));
    assert!(reason.contains("reviewer/1"), "{reason}");
    assert!(!reason.contains("reviewer/2"), "{reason}");
}

#[test]
fn an_answer_with_a_blank_plan_is_refused_naming_its_finding() {
    for blank in ["", "  \n"] {
        let reason = refusal(&answer(vec![
            entry(
                "reviewer/1",
                "Retitle it.",
                json!([retitle("Kettle", "Kettles")]),
            ),
            entry("reviewer/2", blank, json!([retitle("Shelf", "Shelves")])),
        ]));
        assert!(reason.contains("reviewer/2"), "{reason}");
        assert!(!reason.contains("reviewer/1"), "{reason}");
    }
}

#[test]
fn every_problem_of_an_answer_is_named_together() {
    let reason = refusal(&answer(vec![
        entry("reviewer/1", " ", json!([retitle("Kettle", "Kettles")])),
        entry("reviewer/1", "Retitle it again.", json!([])),
    ]));
    assert!(reason.contains("reviewer/1"), "{reason}");
    assert!(reason.contains("reviewer/2"), "{reason}");
}

#[test]
fn an_answer_that_is_not_one_entry_for_each_finding_in_a_list_is_refused() {
    for raw in [
        json!({"edits":[retitle("Kettle", "Kettles")]}).to_string(),
        json!({"answers":{"finding":"reviewer/1"}}).to_string(),
        json!({"answers":[{"finding":"reviewer/1","edits":[]}]}).to_string(),
        "not json".to_owned(),
    ] {
        assert!(read(&raw, &sent()).is_err(), "{raw}");
    }
}

#[test]
fn a_saved_answer_is_flattened_without_being_checked_against_the_findings_again() {
    let raw = answer(vec![entry(
        "reviewer/1",
        "Retitle it.",
        json!([retitle("Kettle", "Kettles")]),
    )]);
    let answered = reread(&raw).unwrap();
    let patch: Value = serde_json::from_str(&answered.patch).unwrap();
    assert_eq!(patch["edits"][0]["finding"], "reviewer/1");
    assert_eq!(answered.plans.len(), 1);
}
