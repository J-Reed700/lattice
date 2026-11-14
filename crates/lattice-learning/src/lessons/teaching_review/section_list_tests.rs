#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::test_support::candidate;
use super::*;
use crate::lessons::course_generation_tests::{property_count, ScriptedModel};

fn check(index: usize) -> Value {
    json!({"index":index,"passageId":format!("section-{index}-passage-0"),"finding":"The explanation correctly distinguishes the stated inputs and resulting behavior.","hasDefect":false})
}

fn passages(sections: usize) -> Vec<Vec<String>> {
    (0..sections)
        .map(|i| vec![format!("section-{i}-passage-0")])
        .collect()
}

fn reply(checks: Vec<Value>) -> Value {
    json!({"issues":[],"blockChecks":fixture_checks(checks)})
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
    let listed = &size(16)["properties"]["blockChecks"]["items"]["properties"]["sectionId"]["enum"];
    assert_eq!(listed.as_array().map(Vec::len), Some(16));
}

#[test]
fn the_schema_accepts_one_check_per_section_and_only_known_ids() {
    let validator =
        jsonschema::JSONSchema::compile(&schema(&[0, 1, 2], &passages(3), &[], Remit::Open))
            .unwrap();
    let mut response = reply(vec![check(0), check(1), check(2)]);
    assert!(validator.is_valid(&response));
    response["blockChecks"][0]["sectionId"] = json!("section-9");
    assert!(!validator.is_valid(&response));
    response["blockChecks"][0]["sectionId"] = json!("section-0");
    response["blockChecks"][0]["passageId"] = json!("invented");
    assert!(!validator.is_valid(&response));
    response["blockChecks"][0]["passageId"] = json!("section-0-passage-0");
    response["blockChecks"][0]["index"] = json!(1);
    assert!(!validator.is_valid(&response));
}

#[tokio::test]
async fn a_section_checked_twice_is_named_when_the_review_is_asked_again() {
    let first = reply(vec![check(0), check(1), check(1), check(2)]);
    let (result, prompts) = reviewed(vec![first, reply(vec![check(0), check(1), check(2)])]).await;
    assert!(result.unwrap().is_empty());
    assert_eq!(prompts.len(), 2);
    assert!(prompts[1].contains("section-1 was checked more than once"));
}

#[tokio::test]
async fn a_section_left_out_is_named_when_the_review_is_asked_again() {
    let first = reply(vec![check(0), check(2)]);
    let (result, prompts) = reviewed(vec![first, reply(vec![check(0), check(1), check(2)])]).await;
    assert!(result.unwrap().is_empty());
    assert_eq!(prompts.len(), 2);
    assert!(prompts[1].contains("section-1 still needs a valid check."));
}

#[tokio::test]
async fn a_passage_of_another_section_is_named_with_its_section() {
    let mut crossed = check(0);
    crossed["passageId"] = json!("section-1-passage-0");
    let first = reply(vec![crossed, check(1), check(2)]);
    let (result, prompts) = reviewed(vec![first, reply(vec![check(0), check(1), check(2)])]).await;
    assert!(result.unwrap().is_empty());
    assert_eq!(prompts.len(), 2);
    assert!(prompts[1].contains("section-0: passageId must be one of"));
}

#[tokio::test]
async fn a_check_without_its_section_is_asked_for_again() {
    let mut nameless = reply(vec![check(0), check(1), check(2)]);
    let third = nameless["blockChecks"][2].as_object_mut().unwrap();
    third.remove("sectionId");
    let (result, prompts) =
        reviewed(vec![nameless, reply(vec![check(0), check(1), check(2)])]).await;
    assert!(result.unwrap().is_empty());
    assert_eq!(prompts.len(), 2);
    assert!(prompts[1].contains("needs the sectionId"));
}

#[tokio::test]
async fn a_missing_check_is_named_by_its_section_id() {
    let first = reply(vec![check(1), check(2)]);
    let (result, prompts) = reviewed(vec![first, reply(vec![check(0), check(1), check(2)])]).await;
    assert!(result.unwrap().is_empty());
    assert_eq!(prompts.len(), 2);
    assert!(prompts[1].contains("section-0 still needs a valid check."));
}
