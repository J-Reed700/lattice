#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::test_support::candidate;
use super::*;
use crate::lessons::course_generation_tests::{ScriptedModel, Windowed};
use crate::review_findings::FindingKind;

fn check(index: usize) -> Value {
    json!({"index":index,"passageId":format!("section-{index}-passage-0"),"finding":"The explanation correctly distinguishes the stated inputs and resulting behavior.","hasDefect":false})
}
fn model(replies: Vec<Value>) -> ScriptedModel {
    ScriptedModel {
        outputs: std::sync::Mutex::new(replies.into_iter().map(|v| v.to_string()).collect()),
        prompts: Default::default(),
    }
}

#[tokio::test]
async fn retains_more_than_six_findings_without_rejecting_a_complete_review() -> Result<()> {
    let findings: Vec<_> = (0..12)
        .map(|i| format!("Finding {i}: this assessment has multiple valid answers."))
        .collect();
    let response =
        json!({"issues":findings,"blockChecks":fixture_checks(vec![check(0),check(1),check(2)])});
    let (_, passages) = review_candidate(&candidate());
    assert!(
        jsonschema::JSONSchema::compile(&schema(&[0, 1, 2], &passages))
            .unwrap()
            .is_valid(&response)
    );
    let model = model(vec![response]);
    let actual = review(
        &model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        None,
        LengthRule::Fail,
    )
    .await?;
    assert_eq!(
        actual.iter().map(|f| f.note.clone()).collect::<Vec<_>>(),
        findings
    );
    assert!(actual
        .iter()
        .all(|f| f.kind == FindingKind::Teaching && f.site.is_none()));
    // The reviewer's own statement of the problem is what the user reads.
    assert_eq!(
        actual.iter().map(|f| f.summary.clone()).collect::<Vec<_>>(),
        findings
    );
    assert_eq!(model.prompts.lock().unwrap().len(), 1);
    Ok(())
}

#[test]
fn response_slots_cannot_renumber_sections_or_cite_unknown_passages() {
    let (_, passages) = review_candidate(&candidate());
    let compiled = jsonschema::JSONSchema::compile(&schema(&[0, 1, 2], &passages)).unwrap();
    let mut response =
        json!({"issues":[],"blockChecks":fixture_checks(vec![check(0),check(1),check(2)])});
    assert!(compiled.is_valid(&response));
    response["blockChecks"][0]["passageId"] = json!("invented-passage");
    assert!(!compiled.is_valid(&response));
    response["blockChecks"][0]["passageId"] = json!("section-0-passage-0");
    response["blockChecks"][0]["index"] = json!(1);
    assert!(!compiled.is_valid(&response));
    response["blockChecks"][0]
        .as_object_mut()
        .unwrap()
        .remove("index");
    response["blockChecks"][0]["sectionId"] = json!("section-7");
    assert!(!compiled.is_valid(&response));
}

#[test]
fn each_reviewed_part_carries_the_label_the_user_sees_beside_its_key() {
    let original = json!({
        "blocks":[{"body":"one","title":"First"},{"body":"two","title":"Second"}],
        "questions":[{"prompt":"ask"},{"prompt":"again"}]
    });
    let (reviewed, ids) = review_candidate(&original);
    assert_eq!(reviewed["blocks"][0]["label"], "Section 1");
    assert_eq!(reviewed["blocks"][1]["label"], "Section 2");
    assert_eq!(reviewed["questions"][0]["label"], "Question 1");
    assert_eq!(reviewed["questions"][1]["label"], "Question 2");
    assert_eq!(reviewed["blocks"][1]["index"], 1);
    assert_eq!(ids[1], ["section-1-passage-0"]);
    assert_eq!(reviewed["blocks"][1]["title"], "Second");
    assert_eq!(original["blocks"][1].get("label"), None);
}

#[test]
fn passages_preserve_every_byte_and_ids_are_scoped_to_the_section() {
    let body = "🦀 Rust’s ‘book’\n```rust\n    println!(\"a  b\");\n```\r\n\r\n".repeat(40);
    let original = json!({"blocks":[{"body":body,"title":"Unicode and code"}]});
    let (reviewed, ids) = review_candidate(&original);
    let rejoined: String = reviewed["blocks"][0]["bodyPassages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["text"].as_str().unwrap())
        .collect();
    assert_eq!(rejoined, body);
    assert!(ids[0].len() > 1);
    assert_eq!(original["blocks"][0]["body"], body);
    assert_eq!(reviewed["blocks"][0]["title"], "Unicode and code");
    for indices in [vec![0], vec![]] {
        jsonschema::JSONSchema::compile(&schema(&indices, &ids)).unwrap();
    }
}

#[tokio::test]
async fn malformed_section_recovers_without_rewriting_content_or_rechecking_valid_sections(
) -> Result<()> {
    let mut wrong_source = check(1);
    wrong_source["passageId"] = json!("section-0-passage-0");
    wrong_source["hasDefect"] = json!(true);
    wrong_source["finding"] =
        json!("The claimed membrane behavior contradicts the stated boundary condition.");
    let model = model(vec![
        json!({"issues":["An assessment has two defensible correct choices."],"blockChecks":crate::teaching_review::fixture_checks(vec![check(0),wrong_source,check(2)])}),
        json!({"issues":[],"blockChecks":crate::teaching_review::fixture_checks(vec![check(1)])}),
    ]);
    let issues = review(
        &model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        None,
        LengthRule::Fail,
    )
    .await?;
    assert_eq!(issues.len(), 2);
    let defect = issues
        .iter()
        .find(|i| i.note.contains("membrane behavior"))
        .unwrap();
    assert_eq!(defect.site, Some(FindingSite::Section(1)));
    assert_eq!(
        defect.summary,
        "The claimed membrane behavior contradicts the stated boundary condition."
    );
    let prompts = model.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 2);
    let retry: Value = serde_json::from_str(prompts[1].rsplit("\n\n").next().unwrap())?;
    assert_eq!(retry["sectionIndicesToReview"], json!([1]));
    assert_eq!(retry["acceptedBlockChecks"].as_array().unwrap().len(), 2);
    assert!(retry["responseCorrection"]
        .as_str()
        .unwrap()
        .contains("not the lesson"));
    Ok(())
}

#[tokio::test]
async fn the_review_is_corrected_while_each_answer_checks_another_section() -> Result<()> {
    let only = |index| json!({"issues":[],"blockChecks":crate::teaching_review::fixture_checks(vec![check(index)])});
    let model = model(vec![only(0), only(1), only(2)]);
    review(
        &model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        None,
        LengthRule::Fail,
    )
    .await?;
    assert_eq!(model.prompts.lock().unwrap().len(), 3);
    Ok(())
}

#[tokio::test]
async fn missing_duplicate_unknown_and_invented_checks_never_approve_unchecked_content() {
    for checks in [
        vec![],
        vec![check(0), check(0), check(2)],
        vec![check(1), check(2), check(3)],
        vec![
            json!({"index":0,"passageId":"invented","finding":"An invented passage cannot establish a complete review.","hasDefect":false}),
            check(1),
            check(2),
        ],
    ] {
        let bad = json!({"issues":[],"blockChecks":crate::teaching_review::fixture_checks(checks)});
        let model = model(vec![bad.clone(), bad]);
        let error = review(
            &model,
            "Review instructional quality.",
            json!({}),
            &candidate(),
            None,
            LengthRule::Fail,
        )
        .await
        .unwrap_err();
        assert!(matches!(error, AppError::ServiceNotAvailable(_)));
        assert!(error.to_string().contains("Nothing was published"));
        assert_eq!(model.prompts.lock().unwrap().len(), 2);
    }
}

#[tokio::test]
async fn invalid_global_findings_can_be_corrected_without_losing_section_checks() -> Result<()> {
    let model = model(vec![
        json!({"issues":["  "],"blockChecks":crate::teaching_review::fixture_checks(vec![check(0),check(1),check(2)])}),
        json!({"issues":[],"blockChecks":crate::teaching_review::fixture_checks(vec![])}),
    ]);
    assert!(review(
        &model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        None,
        LengthRule::Fail
    )
    .await?
    .is_empty());
    assert!(model.prompts.lock().unwrap()[1].contains("\"sectionIndicesToReview\":[]"));
    Ok(())
}

#[tokio::test]
async fn a_lesson_s_corrected_review_drops_blank_findings_and_keeps_the_rest() -> Result<()> {
    let long = format!(
        "The worked example's stated result is wrong: {}",
        "x".repeat(720)
    );
    let mut blank = check(1);
    blank["hasDefect"] = json!(true);
    blank["finding"] = json!("  ");
    let first = json!({"issues":["  "],"blockChecks":fixture_checks(vec![check(0),blank.clone(),check(2)])});
    let second = json!({"issues":[" ",long],"blockChecks":fixture_checks(vec![blank])});
    let lesson = model(vec![first.clone(), second.clone()]);
    let issues = review(
        &lesson,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        None,
        LengthRule::DropBlank,
    )
    .await?;
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].note, long);
    let strict = model(vec![first, second]);
    assert!(review(
        &strict,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        None,
        LengthRule::Fail,
    )
    .await
    .is_err());
    Ok(())
}

#[tokio::test]
async fn a_section_finding_carries_the_passage_the_reviewer_chose() -> Result<()> {
    let mut defect = check(2);
    defect["hasDefect"] = json!(true);
    defect["finding"] = json!("The rest is placed after rolling, contradicting the recipe.");
    let model = model(vec![
        json!({"issues":[],"blockChecks":fixture_checks(vec![check(0),check(1),defect])}),
    ]);
    let issues = review(
        &model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        None,
        LengthRule::DropBlank,
    )
    .await?;
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].site, Some(FindingSite::Section(2)));
    assert_eq!(
        issues[0].passage.as_deref(),
        candidate()["blocks"][2]["body"].as_str()
    );
    Ok(())
}

#[tokio::test]
async fn malformed_json_is_corrected_as_a_review_not_as_lesson_content() -> Result<()> {
    let model = ScriptedModel {
        outputs: std::sync::Mutex::new(
            vec![
                "not json".into(),
                json!({"issues":[],"blockChecks":crate::teaching_review::fixture_checks(vec![check(0),check(1),check(2)])}).to_string(),
            ]
            .into(),
        ),
        prompts: Default::default(),
    };
    assert!(review(
        &model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        None,
        LengthRule::Fail
    )
    .await?
    .is_empty());
    assert_eq!(model.prompts.lock().unwrap().len(), 2);
    Ok(())
}

#[tokio::test]
async fn missing_evidence_field_cannot_erase_a_reported_defect() -> Result<()> {
    let mut incomplete = check(1);
    incomplete.as_object_mut().unwrap().remove("passageId");
    incomplete["hasDefect"] = json!(true);
    let model = model(vec![
        json!({"issues":[],"blockChecks":crate::teaching_review::fixture_checks(vec![check(0),incomplete,check(2)])}),
        json!({"issues":[],"blockChecks":crate::teaching_review::fixture_checks(vec![check(1)])}),
    ]);
    let issues = review(
        &model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        None,
        LengthRule::Fail,
    )
    .await?;
    assert_eq!(issues.len(), 1);
    assert!(issues[0]
        .note
        .starts_with("Section 2 (candidate blocks-1):"));
    assert_eq!(issues[0].site, Some(FindingSite::Section(1)));
    Ok(())
}

#[tokio::test]
async fn a_long_issue_from_the_teaching_review_is_read_not_refused() -> Result<()> {
    let long = format!(
        "The worked example's stated result is wrong: {}",
        "it contradicts the premise it was given. ".repeat(25)
    );
    assert!(long.chars().count() > 700);
    let response =
        json!({"issues":[long],"blockChecks":fixture_checks(vec![check(0),check(1),check(2)])});
    for lengths in [LengthRule::Fail, LengthRule::DropBlank] {
        let model = model(vec![response.clone()]);
        let issues = review(
            &model,
            "Review instructional quality.",
            json!({}),
            &candidate(),
            None,
            lengths,
        )
        .await?;
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].note, long);
        assert_eq!(model.prompts.lock().unwrap().len(), 1);
    }
    Ok(())
}

#[tokio::test]
async fn a_section_finding_of_any_length_is_read_not_refused() -> Result<()> {
    let long = format!(
        "The stated boundary condition is contradicted: {}",
        "the membrane passes what it should hold back. ".repeat(20)
    );
    assert!(long.chars().count() > 700);
    let mut terse = check(1);
    terse["hasDefect"] = json!(true);
    terse["finding"] = json!("Wrong.");
    let mut lengthy = check(2);
    lengthy["hasDefect"] = json!(true);
    lengthy["finding"] = json!(long);
    let model = model(vec![
        json!({"issues":[],"blockChecks":fixture_checks(vec![check(0),terse,lengthy])}),
    ]);
    let issues = review(
        &model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        None,
        LengthRule::Fail,
    )
    .await?;
    let notes: Vec<_> = issues.iter().map(|issue| issue.note.clone()).collect();
    assert_eq!(
        notes,
        vec![
            "Section 2 (candidate blocks-1): Wrong.".to_owned(),
            format!("Section 3 (candidate blocks-2): {long}"),
        ]
    );
    assert_eq!(model.prompts.lock().unwrap().len(), 1);
    Ok(())
}

/// The review carries the lesson beside the writer's prompt, whose
/// excerpts filled the room the writer had: on a 9B's 32k window the
/// review of a PostgreSQL lesson did not fit, and the lesson failed after
/// an hour of checked work (2026-11-06). Six excerpts of 2,000 tokens
/// here, the lesson citing the fifth, in a window that holds some.
#[tokio::test]
async fn a_review_too_big_for_its_window_keeps_the_excerpts_that_fit_the_cited_first() -> Result<()>
{
    let sources: Vec<_> = (0..6)
        .map(|index| json!({"sourceIndex":index,"title":format!("Source {index}"),"url":null,"excerpt":"x ".repeat(4000)}))
        .collect();
    let mut lesson = candidate();
    lesson["blocks"][1]["sourceIndex"] = json!(4);
    let scripted = model(vec![
        json!({"issues":[],"blockChecks":fixture_checks(vec![check(0),check(1),check(2)])}),
    ]);
    let window = Windowed(&scripted, 13_000);

    review(
        &window,
        "Review instructional quality.",
        json!({"authoringContext":{"goal":"Read query plans.","sources":sources}}),
        &lesson,
        None,
        LengthRule::Fail,
    )
    .await?;

    let sent = scripted.prompts.lock().unwrap()[0].clone();
    assert!(sent.contains("Source 4"));
    assert!(!sent.contains("Source 5"));
    Ok(())
}

#[test]
fn the_review_schema_bounds_no_text_but_a_blank() {
    let (_, passages) = review_candidate(&candidate());
    let bounds =
        crate::lessons::course_generation_tests::length_bounds(&schema(&[0, 1, 2], &passages));
    assert!(bounds.is_empty(), "{bounds:?}");
}
