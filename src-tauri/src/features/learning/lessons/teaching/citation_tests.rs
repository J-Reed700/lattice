use super::*;
use crate::features::learning::lessons::course_generation_tests::ScriptedModel;
use std::sync::Mutex;

const EXCERPT: &str = "This saved reference describes the exact behavior of the example tool.";

fn lesson(quote: &str) -> Value {
    json!({"blocks":[{"kind":"explanation","title":"Example","body":"A substantive teaching explanation with practice context. ".repeat(20),"rubric":[],"sourceIndex":0,"quote":quote}],"questions":[]})
}

fn context() -> Value {
    json!({"sources":[{"sourceIndex":0,"excerpt":EXCERPT}]})
}

fn approval() -> Value {
    json!({"issues":[],"blockChecks":crate::features::learning::teaching_review::fixture_checks(vec![json!({"index":0,"passageId":"section-0-passage-0","finding":"The complete explanation was reviewed and no teaching defect was found.","hasDefect":false})])})
}

#[tokio::test]
async fn lesson_review_only_rewrites_established_findings() -> Result<()> {
    let references =
        crate::features::learning::reference_collection::ReferenceCollection::lexical(&[
            crate::features::learning::dto::LearningSourceDto {
                id: "saved-reference".into(),
                title: "Example reference".into(),
                url: None,
                acquired_at: 0,
                excerpt: EXCERPT.into(),
            },
        ])?;
    for actionable in [false, true] {
        let original = lesson(EXCERPT);
        let mut corrected = original.clone();
        if let Some(title) = corrected.pointer_mut("/blocks/0/title") {
            *title = json!("Corrected example");
        }
        let mut review = approval();
        if let Some(issues) = review.get_mut("issues") {
            *issues = json!(["Correct the example's claimed behavior using the reference."]);
        }
        let decision = json!({"issue-0":{"verdict":if actionable {"actionable"} else {"not_established"},"basis":"external_fact","evidenceIds":["reference-0"],"requirementId":null,"reason":"The source comparison determines whether this proposed correction is justified."}});
        let mut outputs = vec![review.to_string(), decision.to_string()];
        if actionable {
            outputs.extend([json!({"edits":[{"path":"/blocks/0/title","before":"Example","after":"Corrected example"}]}).to_string(), approval().to_string()]);
        }
        let model = ScriptedModel {
            outputs: Mutex::new(outputs.into()),
            prompts: Mutex::new(Vec::new()),
        };
        let result = review_lesson(
            &model,
            LESSON_REPAIR_SYSTEM,
            &context().to_string(),
            &json!({"type":"object"}),
            original.to_string(),
            4000,
            &references,
        )
        .await?;
        assert_eq!(
            serde_json::from_str::<Value>(&result)?,
            if actionable { corrected } else { original }
        );
        let prompts = model
            .prompts
            .lock()
            .map_err(|_| AppError::InternalError("fixture lock".into()))?;
        assert_eq!(prompts.len(), if actionable { 4 } else { 2 });
        assert_eq!(
            prompts
                .iter()
                .filter(|prompt| prompt.contains("You are editing a rejected draft"))
                .count(),
            usize::from(actionable)
        );
    }
    Ok(())
}

#[tokio::test]
async fn source_typography_is_restored_without_a_model_rewrite() -> Result<()> {
    let exact = "We’ll download Rust through rustup, a\ncommand line tool for managing Rust versions and associated tools.";
    let authoring = json!({"sources":[{"sourceIndex":0,"excerpt":exact}]});
    let original = lesson("We'll download Rust through rustup, a command line tool for managing Rust versions and associated tools.");
    let model = ScriptedModel {
        outputs: Mutex::new(vec![approval().to_string()].into()),
        prompts: Mutex::new(Vec::new()),
    };
    let result = review_and_repair(
        &model,
        LESSON_REPAIR_SYSTEM,
        &authoring.to_string(),
        &json!({"type":"object"}),
        original.to_string(),
        4000,
    )
    .await?;
    assert_eq!(serde_json::from_str::<Value>(&result)?, lesson(exact));
    Ok(())
}

#[test]
fn typography_recovery_never_changes_words_quantities_or_the_cited_source() {
    let exact = "The cell’s response does not increase by 5 units: x != 10.";
    let authoring = json!({"sources":[{"sourceIndex":0,"excerpt":exact},{"sourceIndex":1,"excerpt":"Another source describes a different biological mechanism."}]});
    for quote in [
        exact.replace("does not", "does"),
        exact.replace("5 units", "6 units"),
        exact.replace("!=", "="),
        "This unsupported quotation has no matching source passage.".into(),
    ] {
        let mut candidate = lesson(&quote);
        let before = candidate.clone();
        assert_eq!(restore_lesson_quotes(&mut candidate, &authoring), 0);
        assert_eq!(candidate, before);
    }
    let mut wrong_source = json!({"questions":[{"sourceIndex":1,"quote":exact.replace('’', "'")}]});
    let before = wrong_source.clone();
    assert_eq!(restore_lesson_quotes(&mut wrong_source, &authoring), 0);
    assert_eq!(wrong_source, before);
}

#[test]
fn lesson_citations_match_the_published_source_index_and_quote_rules() {
    let mut candidate =
        lesson("This saved reference\n describes the exact behavior of the example tool.");
    assert!(lesson_quote_checks(&candidate, &context()).is_empty());
    if let Some(object) = candidate.as_object_mut() {
        object.insert(
            "questions".into(),
            json!([{"sourceIndex":1,"quote":EXCERPT}]),
        );
    }
    let checks = lesson_quote_checks(&candidate, &context());
    assert_eq!(checks.len(), 1);
    assert_eq!(
        checks
            .first()
            .and_then(|v| v.get("path"))
            .and_then(Value::as_str),
        Some("questions[0]")
    );
}

#[tokio::test]
async fn a_model_approval_cannot_skip_repair_of_an_invalid_lesson_quotation() -> Result<()> {
    let corrected = lesson(EXCERPT);
    let model = ScriptedModel {
        outputs: Mutex::new(
            vec![
                approval().to_string(),
                json!({"edits":[{"path":"/blocks/0/quote","before":"An invented quotation that does not appear in the selected source.","after":EXCERPT}]}).to_string(),
                approval().to_string(),
            ]
            .into(),
        ),
        prompts: Mutex::new(Vec::new()),
    };
    let result = review_and_repair(
        &model,
        LESSON_REPAIR_SYSTEM,
        &context().to_string(),
        &json!({"type":"object"}),
        lesson("An invented quotation that does not appear in the selected source.").to_string(),
        4000,
    )
    .await?;
    assert_eq!(serde_json::from_str::<Value>(&result)?, corrected);
    Ok(())
}

#[tokio::test]
async fn an_invalid_repaired_quotation_stops_before_more_model_review() {
    let invalid = lesson("An invented quotation that does not appear in the selected source.");
    let model = ScriptedModel {
        outputs: Mutex::new(vec![approval().to_string(), json!({"edits":[{"path":"/blocks/0/quote","before":"An invented quotation that does not appear in the selected source.","after":"A different invented quotation that is absent from the selected source."}]}).to_string()].into()),
        prompts: Mutex::new(Vec::new()),
    };
    let result = review_and_repair(
        &model,
        LESSON_REPAIR_SYSTEM,
        &context().to_string(),
        &json!({"type":"object"}),
        invalid.to_string(),
        4000,
    )
    .await;
    assert!(result.is_err_and(|error| error
        .to_string()
        .contains("revised lesson contains 1 source quotation")));
}
