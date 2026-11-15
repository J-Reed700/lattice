#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::test_support::candidate;
use super::*;
use crate::content_verification::tests::fixture_task;
use crate::lessons::course_generation_tests::{ScriptedModel, Windowed};

/// A first review of everything: what it reported.
async fn review(
    llm: &dyn LLMPort,
    system: &str,
    prompt: Value,
    candidate: &Value,
    progress: Option<&OutlineProgress>,
) -> Result<Vec<Reported>> {
    Ok(super::review(
        llm,
        system,
        prompt,
        candidate,
        Scope::default(),
        Remit::Open,
        progress,
    )
    .await?
    .reported)
}

fn entry(index: usize) -> Value {
    json!({"passageId":format!("section-{index}-passage-0"),"finding":"The explanation contradicts the stated inputs and resulting behavior."})
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
    let response = json!({"issues":findings,"sectionDefects":[]});
    let (_, passages) = review_candidate(&candidate(), &[0, 1, 2]);
    assert!(
        jsonschema::JSONSchema::compile(&schema(&[0, 1, 2], &passages, &[], Remit::Open))
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
    )
    .await?;
    assert_eq!(
        actual.iter().map(|f| f.note.clone()).collect::<Vec<_>>(),
        findings
    );
    assert!(actual.iter().all(|f| f.site.is_none()));
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
    let (_, passages) = review_candidate(&candidate(), &[0, 1, 2]);
    let compiled =
        jsonschema::JSONSchema::compile(&schema(&[0, 1, 2], &passages, &[], Remit::Open)).unwrap();
    let mut response = json!({"issues":[],"sectionDefects":vec![entry(0),entry(1),entry(2)]});
    assert!(compiled.is_valid(&response));
    response["sectionDefects"][0]["passageId"] = json!("invented-passage");
    assert!(!compiled.is_valid(&response));
    response["sectionDefects"][0]["passageId"] = json!("section-0-passage-0");
    response["sectionDefects"][0]["index"] = json!(1);
    assert!(!compiled.is_valid(&response));
    response["sectionDefects"][0]
        .as_object_mut()
        .unwrap()
        .remove("index");
    response["sectionDefects"][0]["sectionId"] = json!("section-7");
    assert!(!compiled.is_valid(&response));
}

#[test]
fn each_reviewed_part_carries_the_label_the_user_sees_beside_its_key() {
    let original = json!({
        "blocks":[{"body":"one","title":"First"},{"body":"two","title":"Second"}],
        "questions":[{"prompt":"ask"},{"prompt":"again"}]
    });
    let (reviewed, ids) = review_candidate(&original, &[0, 1]);
    assert_eq!(reviewed["blocks"][0]["label"], "Section 1");
    assert_eq!(reviewed["blocks"][1]["label"], "Section 2");
    assert_eq!(reviewed["questions"][0]["label"], "Question 1");
    assert_eq!(reviewed["questions"][1]["label"], "Question 2");
    assert_eq!(reviewed["blocks"][1]["inReview"], true);
    assert!(reviewed["blocks"][1].get("index").is_none());
    assert_eq!(ids[1], ["section-1-passage-0"]);
    assert_eq!(reviewed["blocks"][1]["title"], "Second");
    assert_eq!(original["blocks"][1].get("label"), None);
}

#[test]
fn passages_preserve_every_byte_and_ids_are_scoped_to_the_section() {
    let body = "🦀 Rust’s ‘book’\n```rust\n    println!(\"a  b\");\n```\r\n\r\n".repeat(40);
    let original = json!({"blocks":[{"body":body,"title":"Unicode and code"}]});
    let (reviewed, ids) = review_candidate(&original, &[0]);
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
        jsonschema::JSONSchema::compile(&schema(&indices, &ids, &[], Remit::Open)).unwrap();
    }
}

#[tokio::test]
async fn a_malformed_section_defect_is_corrected_without_rewriting_content() -> Result<()> {
    let mut wrong_source = entry(1);
    wrong_source["finding"] = json!(7);
    let model = model(vec![
        json!({"issues":["An assessment has two defensible correct choices."],"sectionDefects":vec![wrong_source]}),
        json!({"issues":["An assessment has two defensible correct choices."],"sectionDefects":[]}),
    ]);
    let issues = review(
        &model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        None,
    )
    .await?;
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].site, None);
    let prompts = model.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 2);
    assert!(prompts[1].contains("missing, unexpected or incorrectly typed fields"));
    Ok(())
}

#[tokio::test]
async fn a_blank_global_finding_is_corrected_by_a_complete_second_answer() -> Result<()> {
    let model = model(vec![
        json!({"issues":["  "],"sectionDefects":[]}),
        json!({"issues":[],"sectionDefects":[]}),
    ]);
    assert!(review(
        &model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        None,
    )
    .await?
    .is_empty());
    assert_eq!(model.prompts.lock().unwrap().len(), 2);
    Ok(())
}

#[tokio::test]
async fn a_lesson_s_corrected_review_drops_blank_findings_and_keeps_the_rest() -> Result<()> {
    let long = format!(
        "The worked example's stated result is wrong: {}",
        "x".repeat(720)
    );
    let mut blank = entry(1);
    blank["finding"] = json!("  ");
    let first = json!({"issues":["  "],"sectionDefects":vec![blank.clone()]});
    let second = json!({"issues":[" ",long],"sectionDefects":vec![blank]});
    let lesson = model(vec![first.clone(), second.clone()]);
    let issues = review(
        &lesson,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        None,
    )
    .await?;
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].note, long);
    Ok(())
}

#[tokio::test]
async fn a_section_finding_carries_the_passage_the_reviewer_chose() -> Result<()> {
    let mut defect = entry(2);
    defect["finding"] = json!("The rest is placed after rolling, contradicting the recipe.");
    let model = model(vec![json!({"issues":[],"sectionDefects":vec![defect]})]);
    let issues = review(
        &model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        None,
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
                json!({"issues":[],"sectionDefects":[]}).to_string(),
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
    )
    .await?
    .is_empty());
    assert_eq!(model.prompts.lock().unwrap().len(), 2);
    Ok(())
}

#[tokio::test]
async fn a_defect_missing_its_passage_is_corrected_not_approved() -> Result<()> {
    let mut incomplete = entry(1);
    incomplete.as_object_mut().unwrap().remove("passageId");
    let defect = entry(1);
    let model = model(vec![
        json!({"issues":[],"sectionDefects":vec![incomplete]}),
        json!({"issues":[],"sectionDefects":vec![defect]}),
    ]);
    let issues = review(
        &model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        None,
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
    let response = json!({"issues":[long],"sectionDefects":[]});
    let model = model(vec![response]);
    let issues = review(
        &model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        None,
    )
    .await?;
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].note, long);
    assert_eq!(model.prompts.lock().unwrap().len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_section_finding_of_any_length_is_read_not_refused() -> Result<()> {
    let long = format!(
        "The stated boundary condition is contradicted: {}",
        "the membrane passes what it should hold back. ".repeat(20)
    );
    assert!(long.chars().count() > 700);
    let mut terse = entry(1);
    terse["finding"] = json!("Wrong.");
    let mut lengthy = entry(2);
    lengthy["finding"] = json!(long);
    let model = model(vec![
        json!({"issues":[],"sectionDefects":vec![terse,lengthy]}),
    ]);
    let issues = review(
        &model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        None,
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
    let scripted = model(vec![json!({"issues":[],"sectionDefects":[]})]);
    let window = Windowed(&scripted, 13_000);

    review(
        &window,
        "Review instructional quality.",
        json!({"authoringContext":{"goal":"Read query plans.","sources":sources}}),
        &lesson,
        None,
    )
    .await?;

    let sent = scripted.prompts.lock().unwrap()[0].clone();
    assert!(sent.contains("Source 4"));
    assert!(!sent.contains("Source 5"));
    Ok(())
}

#[test]
fn the_review_schema_bounds_no_text_but_a_blank() {
    let (_, passages) = review_candidate(&candidate(), &[0, 1, 2]);
    let bounds = crate::lessons::course_generation_tests::length_bounds(&schema(
        &[0, 1, 2],
        &passages,
        &[],
        Remit::Open,
    ));
    assert!(bounds.is_empty(), "{bounds:?}");
}

fn asked(number: u32, index: usize) -> Asked {
    Asked {
        id: ReviewFinding::reviewer(number, Some(FindingSite::Section(index)), "x").id(),
        site: FindingSite::Section(index),
        passage_id: None,
        note: "A defect.".to_owned(),
    }
}

fn verdict_entry(number: u32, verdict: &str, quote: &str) -> Value {
    json!({"id":format!("reviewer/{number}"),"quote":quote,"verdict":verdict})
}

fn sent(model: &ScriptedModel, call: usize) -> Value {
    let prompts = model.prompts.lock().unwrap();
    fixture_task(&prompts[call]).unwrap()
}

#[test]
fn the_schema_asks_for_a_verdict_on_exactly_the_asked_ids_and_not_at_all_when_none_were_asked() {
    let (_, passages) = review_candidate(&candidate(), &[0, 1, 2]);
    let with = schema(&[0], &passages, &["reviewer/1", "reviewer/2"], Remit::Open);
    assert_eq!(
        with.pointer("/properties/openFindings/items/properties/id/enum"),
        Some(&json!(["reviewer/1", "reviewer/2"]))
    );
    assert_eq!(
        with.pointer("/properties/openFindings/minItems"),
        Some(&json!(2))
    );
    assert_eq!(
        with.pointer("/properties/openFindings/maxItems"),
        Some(&json!(2))
    );
    assert_eq!(with["required"][0], "openFindings");
    let without = schema(&[0], &passages, &[], Remit::Open);
    assert!(without.pointer("/properties/openFindings").is_none());
    assert!(!without["required"]
        .as_array()
        .unwrap()
        .contains(&json!("openFindings")));
}

#[tokio::test]
async fn an_answer_that_leaves_an_id_out_is_corrected_by_a_complete_one() -> Result<()> {
    let asked = [asked(1, 0), asked(2, 0)];
    let model = model(vec![
        json!({"openFindings":[verdict_entry(1, "resolved", "")],"issues":[],"sectionDefects":[]}),
        json!({"openFindings":[verdict_entry(1, "resolved", ""), verdict_entry(2, "present", " compiler is rustc. ")],"issues":[],"sectionDefects":[]}),
    ]);
    let judged = super::review(
        &model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        Scope {
            sections: Some(&[0]),
            asked: Some(&asked),
        },
        Remit::Open,
        None,
    )
    .await?;
    assert_eq!(judged.verdicts.len(), 2);
    assert_eq!(judged.verdicts[0].id, asked[0].id);
    assert!(!judged.verdicts[0].present);
    assert!(judged.verdicts[1].present);
    assert_eq!(judged.verdicts[1].quote, "compiler is rustc.");
    let prompts = model.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 2);
    assert!(prompts[1].contains("reviewer/2 needs one verdict."));
    Ok(())
}

#[tokio::test]
async fn a_second_verdict_for_one_id_is_refused_then_leaves_its_finding_as_it_stood() -> Result<()>
{
    let asked = [asked(1, 0)];
    let twice = || json!({"openFindings":[verdict_entry(1, "resolved", ""), verdict_entry(1, "present", "compiler is rustc.")],"issues":[],"sectionDefects":[]});
    let model = model(vec![twice(), twice()]);
    let judged = super::review(
        &model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        Scope {
            sections: Some(&[0]),
            asked: Some(&asked),
        },
        Remit::Open,
        None,
    )
    .await?;
    assert!(judged.verdicts.is_empty());
    let prompts = model.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 2);
    assert!(prompts[1].contains("reviewer/1 needs one verdict."));
    Ok(())
}

#[tokio::test]
async fn a_verdict_on_an_id_nobody_asked_about_is_refused_then_dropped() -> Result<()> {
    let asked = [asked(1, 0)];
    let stray = || json!({"openFindings":[verdict_entry(1, "resolved", ""), verdict_entry(9, "resolved", "")],"issues":[],"sectionDefects":[]});
    let model = model(vec![stray(), stray()]);
    let judged = super::review(
        &model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        Scope {
            sections: Some(&[0]),
            asked: Some(&asked),
        },
        Remit::Open,
        None,
    )
    .await?;
    assert_eq!(judged.verdicts.len(), 1);
    assert_eq!(judged.verdicts[0].id, asked[0].id);
    assert!(!judged.verdicts[0].present);
    let prompts = model.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 2);
    assert!(prompts[1].contains("reviewer/9 was not asked about"));
    Ok(())
}

#[tokio::test]
async fn the_scope_limits_the_sections_in_review() -> Result<()> {
    let model = model(vec![json!({"issues":[],"sectionDefects":[]})]);
    super::review(
        &model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        Scope {
            sections: Some(&[1]),
            ..Scope::default()
        },
        Remit::Open,
        None,
    )
    .await?;
    let marks: Vec<_> = sent(&model, 0)["candidate"]["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|block| block["inReview"].clone())
        .collect();
    assert_eq!(marks, [json!(false), json!(true), json!(false)]);
    Ok(())
}

#[tokio::test]
async fn a_first_review_sends_no_finding_keys() -> Result<()> {
    let model = model(vec![json!({"issues":[],"sectionDefects":[]})]);
    review(
        &model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        None,
    )
    .await?;
    let prompt = sent(&model, 0);
    assert!(prompt.get("openFindings").is_none());
    assert!(prompt.get("knownFindings").is_none());
    assert!(prompt.get("sectionIndicesToReview").is_none());
    assert!(prompt["candidate"]["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .all(|block| block["inReview"] == true));
    Ok(())
}

#[tokio::test]
async fn a_correction_asks_for_every_verdict_again() -> Result<()> {
    let asked = [asked(1, 0), asked(2, 0)];
    let model = model(vec![
        json!({"openFindings":[verdict_entry(1, "resolved", "")],"issues":[],"sectionDefects":[]}),
        json!({"openFindings":[verdict_entry(1, "resolved", ""), verdict_entry(2, "resolved", "")],"issues":[],"sectionDefects":[]}),
    ]);
    super::review(
        &model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        Scope {
            sections: Some(&[0]),
            asked: Some(&asked),
        },
        Remit::Open,
        None,
    )
    .await?;
    let ids = |call: usize| -> Vec<Value> {
        sent(&model, call)["openFindings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["id"].clone())
            .collect()
    };
    assert_eq!(ids(0), [json!("reviewer/1"), json!("reviewer/2")]);
    assert_eq!(ids(1), [json!("reviewer/1"), json!("reviewer/2")]);
    Ok(())
}

/// A first answer whose defect check for Section 2 names a passage that is
/// in no section, then a correction with the same finding and `passage`.
async fn a_defect_named_with_an_unknown_passage(passage: &str) -> (Judged, ScriptedModel) {
    let finding = "The claimed membrane behavior contradicts the stated boundary condition.";
    let mut wrong = entry(1);
    wrong["passageId"] = json!("invented-passage");
    wrong["finding"] = json!(finding);
    let mut right = wrong.clone();
    right["passageId"] = json!(passage);
    let model = model(vec![
        json!({"issues":["A general issue."],"sectionDefects":vec![wrong]}),
        json!({"issues":["A general issue."],"sectionDefects":vec![right]}),
    ]);
    let judged = super::review(
        &model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        Scope::default(),
        Remit::Open,
        None,
    )
    .await
    .unwrap();
    (judged, model)
}

#[tokio::test]
async fn a_report_takes_its_passage_from_the_check_that_was_accepted() {
    let (judged, _) = a_defect_named_with_an_unknown_passage("section-1-passage-0").await;
    let section: Vec<_> = judged
        .reported
        .iter()
        .filter(|reported| reported.site == Some(FindingSite::Section(1)))
        .collect();
    assert_eq!(section.len(), 1);
    assert_eq!(
        section[0].passage_id.as_deref(),
        Some("section-1-passage-0")
    );
    let (shown, _) = review_candidate(&candidate(), &[0, 1, 2]);
    assert_eq!(
        section[0].passage.as_deref(),
        shown["blocks"][1]["bodyPassages"][0]["text"].as_str()
    );
}

#[test]
fn a_schema_that_asks_about_open_findings_has_each_report_say_which_it_continues() {
    let (_, passages) = review_candidate(&candidate(), &[0, 1, 2]);
    let with = schema(&[0], &passages, &["reviewer/1", "reviewer/2"], Remit::Open);
    let choices = json!(["reviewer/1", "reviewer/2", "none"]);
    assert_eq!(
        with.pointer("/properties/sectionDefects/items/properties/continues/enum"),
        Some(&choices)
    );
    assert!(with["properties"]["sectionDefects"]["items"]["required"]
        .as_array()
        .unwrap()
        .contains(&json!("continues")));
    assert_eq!(
        with.pointer("/properties/issues/items/properties/continues/enum"),
        Some(&choices)
    );
    assert_eq!(
        with.pointer("/properties/issues/items/properties/note/minLength"),
        Some(&json!(1))
    );
}

#[test]
fn a_schema_that_asks_about_nothing_is_the_one_it_was_before_reports_could_continue() {
    let (_, passages) = review_candidate(&candidate(), &[0, 1, 2]);
    let expected = json!({"type":"object","additionalProperties":false,"required":["issues","sectionDefects"],"properties":{
        "issues":{"type":"array","items":{"type":"string","minLength":1}},
        "sectionDefects":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["passageId","finding"],"properties":{
            "passageId":{"type":"string","enum":passages[0]},
            "finding":{"type":"string","minLength":1}}}}}});
    assert_eq!(schema(&[0], &passages, &[], Remit::Open), expected);
}

/// The schema of a review that was asked about `asked`.
fn issue_schema(asked: &[&str]) -> Value {
    let (_, passages) = review_candidate(&candidate(), &[0, 1, 2]);
    schema(&[0], &passages, asked, Remit::Open)
}

#[test]
fn an_asked_finding_can_be_continued_so_an_issue_is_an_object_when_any_finding_is_asked() {
    let asked = issue_schema(&["reviewer/1", "reviewer/2"]);
    assert_eq!(
        asked.pointer("/properties/issues/items/required"),
        Some(&json!(["note", "continues"]))
    );
    let choices = json!(["reviewer/1", "reviewer/2", "none"]);
    assert_eq!(
        asked.pointer("/properties/issues/items/properties/continues/enum"),
        Some(&choices)
    );
    assert_eq!(
        asked.pointer("/properties/sectionDefects/items/properties/continues/enum"),
        Some(&choices)
    );
    assert_eq!(
        asked.pointer("/properties/openFindings/items/properties/id/enum"),
        Some(&json!(["reviewer/1", "reviewer/2"]))
    );
}

/// Passes before and after the fix: with nothing shown an issue is plain text.
#[test]
fn with_no_finding_shown_an_issue_is_a_plain_string() {
    let plain = issue_schema(&[]);
    assert_eq!(
        plain.pointer("/properties/issues/items"),
        Some(&json!({"type":"string","minLength":1}))
    );
}

#[tokio::test]
async fn a_report_carries_the_open_finding_it_continues_and_none_for_an_id_nobody_asked_about(
) -> Result<()> {
    let asked = [asked(1, 0)];
    let mut flagged = entry(0);
    flagged["continues"] = json!("reviewer/1");
    let model = model(vec![json!({
        "openFindings":[verdict_entry(1, "present", "compiler is rustc.")],
        "issues":[
            {"note":"The same defect in new words.","continues":"reviewer/1"},
            {"note":"A different defect.","continues":"none"},
            {"note":"A stray id.","continues":"reviewer/9"}
        ],
        "sectionDefects":vec![flagged]
    })]);
    let judged = super::review(
        &model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        Scope {
            sections: Some(&[0]),
            asked: Some(&asked),
        },
        Remit::Open,
        None,
    )
    .await?;
    let continues: Vec<_> = judged
        .reported
        .iter()
        .map(|report| report.continues.clone())
        .collect();
    assert_eq!(
        continues,
        [
            Some(asked[0].id.clone()),
            None,
            None,
            Some(asked[0].id.clone())
        ]
    );
    Ok(())
}

fn two_parts() -> Value {
    json!({"blocks":[{"body":"Alpha holds the line."},{"body":"Beta follows alpha."}],"questions":[]})
}

fn quoted(number: u32, verdict: &str, quote: &str) -> Value {
    json!({"id":format!("reviewer/{number}"),"quote":quote,"verdict":verdict})
}

fn asked_at(number: u32, site: FindingSite) -> Asked {
    Asked {
        id: ReviewFinding::reviewer(number, Some(site), "x").id(),
        site,
        passage_id: None,
        note: "A defect.".to_owned(),
    }
}

/// The verdicts of a review of section 0 that was asked about `asked`, given
/// `replies` in turn.
async fn judge(asked: &[Asked], replies: Vec<Value>) -> (Vec<Verdict>, ScriptedModel) {
    let model = model(replies);
    let judged = super::review(
        &model,
        "Review instructional quality.",
        json!({}),
        &two_parts(),
        Scope {
            sections: Some(&[0]),
            asked: Some(asked),
        },
        Remit::Open,
        None,
    )
    .await
    .unwrap();
    (judged.verdicts, model)
}

fn answer_of(verdicts: Vec<Value>) -> Value {
    json!({"openFindings":verdicts,"issues":[],"sectionDefects":[]})
}

#[tokio::test]
async fn a_present_verdict_whose_quote_is_not_in_its_part_is_asked_for_again_once_then_leaves_its_finding_as_it_stood(
) {
    let asked = [
        asked_at(1, FindingSite::Section(0)),
        asked_at(2, FindingSite::Section(0)),
    ];
    let bad = quoted(1, "present", "Gamma is absent here.");
    let resolved = quoted(2, "resolved", "");
    let (verdicts, model) = judge(
        &asked,
        vec![
            answer_of(vec![bad.clone(), resolved.clone()]),
            answer_of(vec![bad, resolved]),
        ],
    )
    .await;
    assert_eq!(verdicts.len(), 1);
    assert_eq!(verdicts[0].id, asked[1].id);
    assert!(!verdicts[0].present);
    let prompts = model.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 2);
    assert!(prompts[1].contains("reviewer/1: the quote is not in"));
}

#[tokio::test]
async fn a_present_verdict_whose_quote_stands_in_its_part_is_kept_with_the_quote() {
    let asked = [asked_at(1, FindingSite::Section(0))];
    let (verdicts, model) = judge(
        &asked,
        vec![answer_of(vec![quoted(
            1,
            "present",
            "Alpha holds the line.",
        )])],
    )
    .await;
    assert!(verdicts[0].present);
    assert_eq!(verdicts[0].quote, "Alpha holds the line.");
    assert_eq!(model.prompts.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_finding_on_a_part_the_review_was_not_sent_to_is_judged_on_that_parts_text() {
    let asked = [asked_at(1, FindingSite::Section(1))];
    let (verdicts, model) = judge(
        &asked,
        vec![
            answer_of(vec![quoted(1, "present", "Alpha holds the line.")]),
            answer_of(vec![quoted(1, "present", "Beta follows alpha.")]),
        ],
    )
    .await;
    assert!(verdicts[0].present);
    assert_eq!(verdicts[0].quote, "Beta follows alpha.");
    assert_eq!(model.prompts.lock().unwrap().len(), 2);
    let (kept, model) = judge(
        &asked,
        vec![answer_of(vec![quoted(1, "present", "Beta follows alpha.")])],
    )
    .await;
    assert!(kept[0].present);
    assert_eq!(model.prompts.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_finding_about_the_whole_lesson_is_grounded_by_words_in_any_part() {
    let asked = [asked_at(1, FindingSite::Lesson)];
    let (verdicts, model) = judge(
        &asked,
        vec![answer_of(vec![quoted(1, "present", "Beta follows alpha.")])],
    )
    .await;
    assert!(verdicts[0].present);
    assert_eq!(model.prompts.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_resolved_verdict_with_a_quote_is_just_resolved() {
    let asked = [asked_at(1, FindingSite::Section(0))];
    let (verdicts, model) = judge(
        &asked,
        vec![answer_of(vec![quoted(
            1,
            "resolved",
            "Gamma is absent here.",
        )])],
    )
    .await;
    assert!(!verdicts[0].present);
    assert_eq!(model.prompts.lock().unwrap().len(), 1);
}

#[test]
fn a_verdict_is_an_id_a_quote_and_a_judgement_in_that_order_with_no_note() {
    let (_, passages) = review_candidate(&candidate(), &[0, 1, 2]);
    let with = schema(&[0], &passages, &["reviewer/1"], Remit::Open);
    assert_eq!(
        with.pointer("/properties/openFindings/items/required"),
        Some(&json!(["id", "quote", "verdict"]))
    );
    let fields: Vec<&String> = with
        .pointer("/properties/openFindings/items/properties")
        .and_then(Value::as_object)
        .unwrap()
        .keys()
        .collect();
    assert_eq!(fields, ["id", "quote", "verdict"]);
}

#[test]
fn the_open_findings_sentence_sends_the_reviewer_to_the_part_as_it_now_stands_listed_or_not() {
    assert!(OPEN_FINDINGS_BRIEF.contains("whether or not that part is in review"));
    assert!(OPEN_FINDINGS_BRIEF.contains("quote"));
}

#[tokio::test]
async fn a_verdict_quote_is_stored_as_the_lesson_bytes() {
    let asked = [asked_at(1, FindingSite::Section(0))];
    let lesson = json!({"blocks":[{"body":"The borrow checker’s rule holds."}],"questions":[]});
    let model = model(vec![answer_of(vec![quoted(
        1,
        "present",
        "the borrow checker's rule",
    )])]);
    let judged = super::review(
        &model,
        "Review instructional quality.",
        json!({}),
        &lesson,
        Scope {
            sections: Some(&[0]),
            asked: Some(&asked),
        },
        Remit::Open,
        None,
    )
    .await
    .unwrap();
    assert_eq!(judged.verdicts[0].quote, "The borrow checker’s rule");
}

#[tokio::test]
async fn a_teaching_review_that_stays_unreadable_fails_the_lesson() {
    let unreadable = json!({"sectionDefects":[]});
    let model = model(vec![unreadable; 6]);
    let result = review(
        &model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        None,
    )
    .await;
    assert!(matches!(result, Err(AppError::ModelAnswerUnusable(_))));
    assert_eq!(model.prompts.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn a_blank_issue_left_after_the_correction_is_dropped_in_an_outline_review() {
    let long = format!(
        "The worked example's stated result is wrong: {}",
        "x".repeat(720)
    );
    let mut blank = entry(1);
    blank["finding"] = json!("  ");
    let first = json!({"issues":["  "],"sectionDefects":vec![blank.clone()]});
    let second = json!({"issues":[" ",long],"sectionDefects":vec![blank]});
    let model = model(vec![first, second]);
    let issues = review(
        &model,
        "Review instructional quality.",
        json!({}),
        &candidate(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].note, long);
}

#[test]
fn the_review_contract_reads_what_it_renders() {
    lattice_core::application::services::model_contract::test_support::assert_round_trip::<
        TeachingReview,
    >(&Review {
        issues: vec![
            Issue::Plain("A step is missing.".into()),
            Issue::Continuing {
                note: "Still missing.".into(),
                continues: Some("finding-1".into()),
                concern: None,
            },
            Issue::Continuing {
                note: "A new gap.".into(),
                continues: None,
                concern: None,
            },
        ],
        section_defects: vec![json!({"passageId":"section-1-passage-0","finding":"A gap."})],
        open_findings: vec![json!({"id":"finding-1","quote":"","verdict":"resolved"})],
    });
}

#[tokio::test]
async fn prompt_snapshot_teaching_review() {
    let asked = [
        asked_at(1, FindingSite::Section(0)),
        asked_at(2, FindingSite::Section(0)),
    ];
    let present = quoted(1, "present", "Alpha holds the line.");
    let resolved = quoted(2, "resolved", "");
    let scripted = model(vec![answer_of(vec![present, resolved], vec![check(0)])]);
    let log = crate::lessons::course_generation_tests::RequestLog::new(&scripted);
    super::review(
        &log,
        "Review instructional quality.",
        json!({"priorKnowledge":"The learner has seen a map of the town."}),
        &two_parts(),
        Scope {
            sections: Some(&[0]),
            asked: Some(&asked),
        },
        None,
    )
    .await
    .unwrap();
    log.assert_snapshot::<TeachingReview>();
}
