#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::test_support::candidate;
use super::*;
use crate::lessons::course_generation_tests::{property_count, ScriptedModel};

fn check(index: usize) -> Value {
    json!({"passageId":format!("section-{index}-passage-0"),"finding":"The explanation correctly distinguishes the stated inputs and resulting behavior.","hasDefect":false})
}

fn passages(sections: usize) -> Vec<Vec<String>> {
    (0..sections)
        .map(|i| vec![format!("section-{i}-passage-0")])
        .collect()
}

fn reply(checks: Vec<Value>) -> Value {
    json!({"issues":[],"blockChecks":checks})
}

fn model(replies: Vec<Value>) -> ScriptedModel {
    ScriptedModel {
        outputs: std::sync::Mutex::new(replies.iter().map(Value::to_string).collect()),
        prompts: Default::default(),
    }
}

/// The review of the seeded lesson, given `replies` in turn, and the prompts
/// the model was sent.
async fn reviewed(replies: Vec<Value>) -> (Result<Vec<Reported>>, Vec<String>) {
    let model = model(replies);
    let result = review(
        &model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        Scope::default(),
        Remit::Open,
        None,
    )
    .await
    .map(|judged| judged.reported);
    let prompts = model.prompts.lock().unwrap().clone();
    (result, prompts)
}

#[test]
fn the_schema_is_as_large_for_sixteen_sections_as_for_one() {
    let size = |sections: usize| {
        let indices: Vec<_> = (0..sections).collect();
        schema(&indices, &passages(sections), &[], Remit::Open)
    };
    assert_eq!(property_count(&size(1)), property_count(&size(16)));
    let listed = &size(16)["properties"]["blockChecks"]["items"]["properties"]["passageId"]["enum"];
    assert_eq!(listed.as_array().map(Vec::len), Some(16));
}

#[test]
fn the_schema_accepts_one_check_per_section_and_only_known_ids() {
    let validator =
        jsonschema::JSONSchema::compile(&schema(&[0, 1, 2], &passages(3), &[], Remit::Open))
            .unwrap();
    let mut response = reply(vec![check(0), check(1), check(2)]);
    assert!(validator.is_valid(&response));
    response["blockChecks"][0]["passageId"] = json!("invented");
    assert!(!validator.is_valid(&response));
    response["blockChecks"][0]["passageId"] = json!("section-0-passage-0");
    response["blockChecks"][0]["sectionId"] = json!("section-0");
    assert!(!validator.is_valid(&response));
    response["blockChecks"][0]
        .as_object_mut()
        .unwrap()
        .remove("sectionId");
    response["blockChecks"][0]["index"] = json!(1);
    assert!(!validator.is_valid(&response));
}

#[tokio::test]
async fn a_check_without_its_passage_is_asked_for_again() {
    let mut nameless = reply(vec![check(0), check(1), check(2)]);
    let third = nameless["blockChecks"][2].as_object_mut().unwrap();
    third.remove("passageId");
    let (result, prompts) =
        reviewed(vec![nameless, reply(vec![check(0), check(1), check(2)])]).await;
    assert!(result.unwrap().is_empty());
    assert_eq!(prompts.len(), 2);
    assert!(prompts[1].contains("needs a passageId"));
    assert!(prompts[1].contains("Section 3 still needs a check"));
}

/// A check as the reviewer now writes it: its passage alone names its section.
fn passage_check(section: usize, has_defect: bool) -> Value {
    json!({"passageId":format!("section-{section}-passage-0"),"finding":"The explanation correctly distinguishes the stated inputs and resulting behavior.","hasDefect":has_defect})
}

fn passage_reply(checks: Vec<Value>) -> Value {
    json!({"issues":[],"blockChecks":checks})
}

#[tokio::test]
async fn a_check_is_placed_in_the_section_its_passage_is_in() {
    let model = model(vec![passage_reply(vec![
        passage_check(0, false),
        passage_check(2, true),
    ])]);
    let judged = review(
        &model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        Scope {
            sections: Some(&[0, 2]),
            asked: None,
        },
        Remit::Open,
        None,
    )
    .await
    .unwrap();
    assert_eq!(judged.reported.len(), 1);
    assert_eq!(judged.reported[0].site, Some(FindingSite::Section(2)));
    assert_eq!(
        judged.reported[0].passage_id.as_deref(),
        Some("section-2-passage-0")
    );
}

#[test]
fn a_check_in_the_schema_names_its_passage_and_no_section() {
    let item =
        &schema(&[0, 1, 2], &passages(3), &[], Remit::Open)["properties"]["blockChecks"]["items"];
    assert!(item["properties"].get("sectionId").is_none());
    assert!(item["required"]
        .as_array()
        .unwrap()
        .contains(&json!("passageId")));
}

#[tokio::test]
async fn a_check_that_names_a_section_besides_its_passage_is_refused() {
    let mut named = passage_check(1, false);
    named["sectionId"] = json!("section-1");
    let first = passage_reply(vec![
        passage_check(0, false),
        named,
        passage_check(2, false),
    ]);
    let second = passage_reply(vec![
        passage_check(0, false),
        passage_check(1, false),
        passage_check(2, false),
    ]);
    let (result, prompts) = reviewed(vec![first, second]).await;
    assert!(result.unwrap().is_empty());
    assert_eq!(prompts.len(), 2);
    assert!(prompts[1].contains("missing, unexpected or incorrectly typed fields"));
}

#[tokio::test]
async fn a_section_with_no_check_is_named_by_its_label() {
    let first = passage_reply(vec![passage_check(0, false), passage_check(2, false)]);
    let second = passage_reply(vec![
        passage_check(0, false),
        passage_check(1, false),
        passage_check(2, false),
    ]);
    let (result, prompts) = reviewed(vec![first, second]).await;
    assert!(result.unwrap().is_empty());
    assert_eq!(prompts.len(), 2);
    assert!(prompts[1].contains("Section 2 still needs a check"));
}

#[tokio::test]
async fn a_section_checked_twice_is_named_by_its_label() {
    let first = passage_reply(vec![
        passage_check(0, false),
        passage_check(1, false),
        passage_check(1, false),
        passage_check(2, false),
    ]);
    let second = passage_reply(vec![
        passage_check(0, false),
        passage_check(1, false),
        passage_check(2, false),
    ]);
    let (result, prompts) = reviewed(vec![first, second]).await;
    assert!(result.unwrap().is_empty());
    assert_eq!(prompts.len(), 2);
    assert!(prompts[1].contains("Section 2 was checked more than once"));
}
