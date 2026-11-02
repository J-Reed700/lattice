#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::features::learning::lessons::course_generation_tests::ScriptedModel;
use std::sync::Mutex;

/// The replay's planted statements (evals/grounding/claim-classes/
/// replay_classes.py) and what the rules alone make of each: every planted
/// specific is caught, and a statement with no cue is the model's to sort.
const PLANTED: [(&str, Option<Class>); 17] = [
    ("The study found that OCR-based RAG improved retrieval recall by 23% over the vision-based pipeline on degraded documents.", Some(Class::Sourced)),
    ("The survey reports that modular document parsing pipelines were introduced by Google in 2019.", Some(Class::Sourced)),
    ("Microsoft's grounding feature returns bounding box coordinates in millimetres.", Some(Class::Sourced)),
    ("Donut is an OCR-free model that goes from image to output with a single Transformer.", None),
    ("The survey states that 'the deployment and inference costs associated with VLMs cannot be ignored.'", Some(Class::Sourced)),
    ("In a modular pipeline, an early OCR error cannot reach later stages because each later stage re-reads the page image.", None),
    ("Because Tesseract 5.3 switched to an LSTM recogniser, a text-only parser can recover misread digits.", Some(Class::Sourced)),
    ("In a modular pipeline, each stage hands an intermediate representation to the next, so an early error travels forward.", None),
    ("OCR engines convert spoken audio into text.", None),
    ("A confidence score tells a reviewer where on the page an extracted value came from.", None),
    ("Two plus two equals five.", None),
    ("A bounding box is measured in seconds.", None),
    ("OCR reads the pixels of a page image into text.", None),
    ("Two plus two equals four.", None),
    ("Tesseract is an OCR engine that reads pixels into text.", None),
    ("A page number identifies which page of a document a value appears on.", None),
    ("Vision-language models were first proposed in 2017 and reached human parity on document parsing in 2021.", Some(Class::Sourced)),
];

fn body(statement: &str) -> ToSort<'_> {
    ToSort {
        statement,
        field: "body",
    }
}

fn scripted(replies: Vec<Value>) -> ScriptedModel {
    ScriptedModel {
        outputs: Mutex::new(replies.into_iter().map(|reply| reply.to_string()).collect()),
        prompts: Mutex::new(Vec::new()),
    }
}

#[test]
fn the_rules_alone_catch_every_planted_specific_and_leave_the_rest_to_the_model() {
    for (statement, expected) in PLANTED {
        assert_eq!(rule_class(statement, "body"), expected, "{statement}");
    }
}

#[test]
fn a_remark_about_the_lessons_own_sources_or_options_is_internal_whatever_else_it_says() {
    for statement in [
        "The sources state that the choice depends on document quality.",
        "  The sources do not support the claim that all 3 designs differ.",
        "The other options each remove the evidence needed to check a table.",
    ] {
        assert_eq!(
            rule_class(statement, "body"),
            Some(Class::Internal),
            "{statement}"
        );
    }
}

#[test]
fn what_a_rubric_or_an_option_says_is_internal_even_with_a_figure_in_it() {
    assert_eq!(
        rule_class(
            "The answer names at least 2 trade-offs.",
            "rubric/2/description"
        ),
        Some(Class::Internal)
    );
    assert_eq!(
        rule_class(
            "Retries use exponential backoff three times, up to 30 seconds.",
            "options/1"
        ),
        Some(Class::Internal)
    );
    assert_eq!(
        rule_class("Retries use exponential backoff up to 30 seconds.", "body"),
        Some(Class::Sourced)
    );
}

#[test]
fn quoted_wording_is_sourced_but_a_lone_apostrophe_is_not() {
    assert_eq!(
        rule_class(
            "The page reads \"Total due on receipt of goods\" at the foot.",
            "body"
        ),
        Some(Class::Sourced)
    );
    assert_eq!(
        rule_class("The model's output is parsed by the next stage.", "body"),
        None
    );
}

#[test]
fn the_answer_schema_names_every_id_once_and_only_the_four_classes() {
    let ids = vec!["s0".to_owned(), "s3".to_owned()];
    let validator = jsonschema::JSONSchema::compile(&schema(&ids)).unwrap();
    assert!(validator.is_valid(&json!({"s0": "common", "s3": "internal"})));
    assert!(
        !validator.is_valid(&json!({"s0": "common"})),
        "an id left out"
    );
    assert!(
        !validator.is_valid(&json!({"s0": "common", "s3": "internal", "s9": "common"})),
        "an id not asked for"
    );
    assert!(
        !validator.is_valid(&json!({"s0": "common", "s3": "maybe"})),
        "a class that is not one"
    );
}

#[tokio::test]
async fn statements_the_rules_decide_make_no_model_call() -> Result<()> {
    let model = scripted(vec![]);
    let sorted = classify(
        &model,
        &[
            body("The study found a gain of 23%."),
            body("The other options each fail."),
        ],
    )
    .await?;
    assert_eq!(
        sorted,
        vec![
            (Class::Sourced, ClassifiedBy::Rule),
            (Class::Internal, ClassifiedBy::Rule)
        ]
    );
    assert!(model.prompts.lock().unwrap().is_empty());
    Ok(())
}

#[tokio::test]
async fn the_model_sorts_only_what_the_rules_leave_twenty_to_a_call() -> Result<()> {
    let mut texts: Vec<String> = vec![
        "The study found a gain of 23%.".into(),
        "The sources state that cost matters.".into(),
    ];
    texts.extend(
        ('a'..='v').map(|letter| format!("A stage hands its output to the next stage {letter}.")),
    );
    let to_sort: Vec<ToSort<'_>> = texts.iter().map(|text| body(text)).collect();
    let first: Map<String, Value> = (2..=21)
        .map(|index| {
            let class = if index == 5 { "common" } else { "reasoning" };
            (format!("s{index}"), json!(class))
        })
        .collect();
    let model = scripted(vec![
        Value::Object(first),
        json!({"s22": "internal", "s23": "reasoning"}),
    ]);

    let sorted = classify(&model, &to_sort).await?;

    assert_eq!(sorted.len(), 24);
    assert_eq!(sorted[0], (Class::Sourced, ClassifiedBy::Rule));
    assert_eq!(sorted[1], (Class::Internal, ClassifiedBy::Rule));
    assert_eq!(sorted[2], (Class::Reasoning, ClassifiedBy::Model));
    assert_eq!(sorted[5], (Class::Common, ClassifiedBy::Model));
    assert_eq!(sorted[22], (Class::Internal, ClassifiedBy::Model));
    assert_eq!(sorted[23], (Class::Reasoning, ClassifiedBy::Model));
    let prompts = model.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 2, "twenty statements to a call");
    assert!(prompts[0].starts_with("You sort statements"));
    assert!(
        prompts
            .iter()
            .all(|prompt| !prompt.contains("The study found") && !prompt.contains("The sources")),
        "a statement the rules decided never reaches the model"
    );
    assert!(prompts[0].contains("stage a.") && !prompts[0].contains("stage u."));
    assert!(prompts[1].contains("stage u.") && prompts[1].contains("stage v."));
    Ok(())
}

#[tokio::test]
async fn an_answer_missing_a_statement_is_asked_for_once_more_and_then_refused() -> Result<()> {
    let to_sort = [
        body("A stage hands its output to the next stage."),
        body("A bounding box locates a value on the page."),
    ];

    let model = scripted(vec![
        json!({"s0": "reasoning"}),
        json!({"s0": "reasoning", "s1": "common"}),
    ]);
    let sorted = classify(&model, &to_sort).await?;
    assert_eq!(
        sorted,
        vec![
            (Class::Reasoning, ClassifiedBy::Model),
            (Class::Common, ClassifiedBy::Model)
        ]
    );
    assert_eq!(model.prompts.lock().unwrap().len(), 2);

    let model = scripted(vec![
        json!({"s0": "reasoning"}),
        json!({"s0": "reasoning", "s1": "maybe"}),
    ]);
    let error = classify(&model, &to_sort).await.unwrap_err();
    assert!(matches!(error, AppError::ModelAnswerUnusable(_)), "{error}");
    assert_eq!(model.prompts.lock().unwrap().len(), 2);
    Ok(())
}
