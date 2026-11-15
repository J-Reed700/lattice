#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::test_support::candidate;
use super::*;
use crate::content_verification::tests::fixture_task;
use crate::lessons::course_generation_tests::{property_count, ScriptedModel};

fn passages(sections: usize) -> Vec<Vec<String>> {
    (0..sections)
        .map(|i| vec![format!("section-{i}-passage-0")])
        .collect()
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
    let listed =
        &size(16)["properties"]["sectionDefects"]["items"]["properties"]["passageId"]["enum"];
    assert_eq!(listed.as_array().map(Vec::len), Some(16));
}

#[test]
fn the_schema_accepts_entries_with_only_known_passages_and_no_section_field() {
    let validator =
        jsonschema::JSONSchema::compile(&schema(&[0, 1, 2], &passages(3), &[], Remit::Open))
            .unwrap();
    let mut response = defects_reply(vec![defect(0, "A claim contradicts its premise.")]);
    assert!(validator.is_valid(&response));
    assert!(validator.is_valid(&defects_reply(vec![])));
    response["sectionDefects"][0]["passageId"] = json!("invented");
    assert!(!validator.is_valid(&response));
    response["sectionDefects"][0]["passageId"] = json!("section-0-passage-0");
    response["sectionDefects"][0]["sectionId"] = json!("section-0");
    assert!(!validator.is_valid(&response));
}

#[tokio::test]
async fn a_defect_without_its_passage_is_asked_for_again() {
    let mut nameless = defect(2, "A claim contradicts its premise.");
    nameless.as_object_mut().unwrap().remove("passageId");
    let (result, prompts) = reviewed(vec![
        defects_reply(vec![nameless]),
        defects_reply(vec![defect(2, "A claim contradicts its premise.")]),
    ])
    .await;
    assert_eq!(result.unwrap().len(), 1);
    assert_eq!(prompts.len(), 2);
    assert!(prompts[1]
        .contains("A section defect has missing, unexpected or incorrectly typed fields."));
}

fn defect(section: usize, finding: &str) -> Value {
    json!({"passageId":format!("section-{section}-passage-0"),"finding":finding})
}

fn defects_reply(defects: Vec<Value>) -> Value {
    json!({"issues":[],"sectionDefects":defects})
}

#[tokio::test]
async fn a_review_that_reports_no_section_defect_is_read() {
    let (result, prompts) = reviewed(vec![defects_reply(vec![])]).await;
    assert!(result.unwrap().is_empty());
    assert_eq!(prompts.len(), 1);
}

#[tokio::test]
async fn a_section_defect_is_placed_in_the_section_its_passage_is_in() {
    let finding = "The stated boundary contradicts the example that follows it.";
    let (result, _) = reviewed(vec![defects_reply(vec![defect(2, finding)])]).await;
    let reported = result.unwrap();
    assert_eq!(reported.len(), 1);
    assert_eq!(reported[0].site, Some(FindingSite::Section(2)));
    assert!(reported[0]
        .note
        .starts_with(&format!("{}: ", FindingSite::Section(2).note_label())));
    assert_eq!(
        reported[0].passage.as_deref(),
        Some(passages_of(&candidate())[2][0].1.as_str())
    );
}

#[tokio::test]
async fn two_defects_in_one_section_are_both_reported() {
    let reply = defects_reply(vec![
        defect(1, "The first claim contradicts the stated premise."),
        defect(1, "The second claim omits a condition the example needs."),
    ]);
    let (result, _) = reviewed(vec![reply]).await;
    let reported = result.unwrap();
    assert_eq!(reported.len(), 2);
    assert!(reported
        .iter()
        .all(|found| found.site == Some(FindingSite::Section(1))));
}

#[test]
fn a_section_defect_in_the_schema_has_a_passage_and_no_verdict() {
    let keys = |remit| {
        let schema = schema(&[0, 1, 2], &passages(3), &[], remit);
        assert!(schema["properties"].get("blockChecks").is_none());
        let list = &schema["properties"]["sectionDefects"];
        assert!(list.get("minItems").is_none() && list.get("maxItems").is_none());
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("sectionDefects")));
        let mut keys: Vec<String> = list["items"]["properties"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        keys.sort();
        keys
    };
    assert_eq!(keys(Remit::Lesson), ["concerns", "finding", "passageId"]);
    assert_eq!(keys(Remit::Open), ["finding", "passageId"]);
}

#[tokio::test]
async fn a_defect_outside_the_sections_in_review_is_corrected_then_dropped() {
    let model = model(vec![
        defects_reply(vec![defect(1, "A claim outside the review.")]),
        defects_reply(vec![defect(1, "A claim outside the review.")]),
    ]);
    let judged = review(
        &model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        Scope {
            sections: Some(&[0]),
            asked: None,
        },
        Remit::Open,
        None,
    )
    .await
    .unwrap();
    assert!(judged.reported.is_empty());
    let prompts = model.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 2);
    assert!(prompts[1]
        .contains("passageId section-1-passage-0 is not a passage of a section in review."));
    assert!(!prompts[0].contains("is not a passage of a section in review"));
}

#[tokio::test]
async fn the_candidate_shown_marks_the_sections_in_review_and_gives_no_index() {
    let model = model(vec![defects_reply(vec![])]);
    let _ = review(
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
    .await;
    let prompts = model.prompts.lock().unwrap();
    let sent = fixture_task(&prompts[0]).unwrap();
    let blocks = sent["candidate"]["blocks"].as_array().unwrap();
    let marks: Vec<_> = blocks
        .iter()
        .map(|block| block["inReview"].clone())
        .collect();
    assert_eq!(marks, [json!(true), json!(false), json!(true)]);
    assert!(blocks
        .iter()
        .all(|block| block.get("index").is_none() && block.get("label").is_some()));
    assert!(sent.get("sectionIndicesToReview").is_none());
}
