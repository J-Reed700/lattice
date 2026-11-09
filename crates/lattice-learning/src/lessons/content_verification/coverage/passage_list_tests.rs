#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::lessons::course_generation_tests::{property_count, ScriptedModel};

fn passage(id: &str) -> Value {
    json!({"id":id,"text":"A fact."})
}

fn claim(id: &str) -> Value {
    json!({"id":id,"statement":"A fact."})
}

fn units(passages: usize) -> Vec<Value> {
    vec![json!({"index":0,"kind":"teaching",
        "passages":(0..passages).map(|i| passage(&format!("p{i}"))).collect::<Vec<_>>(),
        "claims":(0..passages).map(|i| claim(&format!("c{i}"))).collect::<Vec<_>>()})]
}

/// Two sections, each with one passage and its own claim.
fn two_sections() -> Vec<Value> {
    vec![
        json!({"index":0,"kind":"teaching","passages":[passage("p0")],"claims":[claim("c0")]}),
        json!({"index":1,"kind":"teaching","passages":[passage("p1")],"claims":[claim("c1")]}),
    ]
}

fn covered(id: &str, claims: &[&str]) -> Value {
    json!({"passageId":id,"claimIds":claims,"nonFactualReason":"","missingClaims":[]})
}

fn answer(decisions: Vec<Value>) -> String {
    json!({ "passages": decisions }).to_string()
}

/// A model that gives `answers` in turn.
fn model(answers: Vec<String>) -> ScriptedModel {
    ScriptedModel {
        outputs: std::sync::Mutex::new(answers.into()),
        prompts: Default::default(),
    }
}

/// The error of a mapping the model answers with `first`, and with `second`
/// when asked again for what is still unanswered.
async fn refused(units: &[Value], first: String, second: String) -> String {
    let model = model(vec![first, second]);
    mapping::review(&model, units, None)
        .await
        .unwrap_err()
        .to_string()
}

#[test]
fn the_schema_is_as_large_for_fifty_passages_as_for_one() {
    let (one, fifty) = (schema(&units(1)), schema(&units(50)));
    assert_eq!(property_count(&one), property_count(&fifty));
    let listed = &fifty["properties"]["passages"]["items"]["properties"]["passageId"]["enum"];
    assert_eq!(listed.as_array().map(Vec::len), Some(50));
}

#[test]
fn the_schema_accepts_one_entry_per_passage_and_only_known_ids() {
    let units = two_sections();
    let validator = jsonschema::JSONSchema::compile(&schema(&units)).unwrap();
    let complete = json!({"passages":[covered("p0", &["c0"]), covered("p1", &["c1"])]});
    assert!(validator.is_valid(&complete));
    let unknown_passage = json!({"passages":[covered("p9", &["c0"])]});
    assert!(!validator.is_valid(&unknown_passage));
    let unknown_claim = json!({"passages":[covered("p0", &["c9"])]});
    assert!(!validator.is_valid(&unknown_claim));
}

#[tokio::test]
async fn a_complete_answer_is_kept_by_passage() {
    let units = two_sections();
    let model = model(vec![answer(vec![
        covered("p0", &["c0"]),
        covered("p1", &["c1"]),
    ])]);
    let raw = mapping::review(&model, &units, None).await.unwrap();
    let kept: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(kept["p0"]["claimIds"], json!(["c0"]));
    assert_eq!(kept["p1"]["claimIds"], json!(["c1"]));
    assert!(kept["p0"].get("passageId").is_none());
    assert!(resolve(&raw, &units).is_ok());
}

#[tokio::test]
async fn a_passage_left_out_is_named_and_the_others_are_not() {
    let units = two_sections();
    let first = answer(vec![covered("p0", &["c0"])]);
    let error = refused(&units, first, answer(Vec::new())).await;
    assert!(error.contains("p1"), "{error}");
    assert!(!error.contains("p0"), "{error}");
}

#[tokio::test]
async fn a_passage_answered_twice_is_named_and_the_others_are_not() {
    let units = two_sections();
    let first = answer(vec![
        covered("p0", &["c0"]),
        covered("p0", &["c0"]),
        covered("p1", &["c1"]),
    ]);
    let again = answer(vec![covered("p0", &["c0"]), covered("p0", &["c0"])]);
    let error = refused(&units, first, again).await;
    assert!(error.contains("p0"), "{error}");
    assert!(!error.contains("p1"), "{error}");
}

#[tokio::test]
async fn a_claim_from_another_section_is_named_with_its_passage() {
    let units = two_sections();
    let first = answer(vec![covered("p0", &["c1"]), covered("p1", &["c1"])]);
    let again = answer(vec![covered("p0", &["c1"])]);
    let error = refused(&units, first, again).await;
    assert!(error.contains("p0"), "{error}");
    assert!(!error.contains("p1"), "{error}");
}

#[tokio::test]
async fn a_passage_asked_for_again_is_the_only_one_in_the_next_request() {
    let units = two_sections();
    let model = model(vec![
        answer(vec![covered("p0", &["c0"])]),
        answer(vec![covered("p1", &["c1"])]),
    ]);
    let raw = mapping::review(&model, &units, None).await.unwrap();
    assert!(resolve(&raw, &units).is_ok());
    let prompts = model.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 2);
    let next: Value = serde_json::from_str(prompts[1].rsplit("\n\n").next().unwrap()).unwrap();
    let asked: Vec<_> = next["units"][0]["passages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|passage| passage["id"].clone())
        .collect();
    assert_eq!(asked, [json!("p1")]);
}
