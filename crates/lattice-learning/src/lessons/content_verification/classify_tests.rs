use super::*;
use crate::lessons::course_generation_tests::ScriptedModel;
use std::sync::Mutex;

/// The replay's planted statements (evals/grounding/claim-classes/
/// replay_classes.py) and what the rules alone make of each: a figure, a
/// quotation or an attribution cue is caught, and a statement with no cue,
/// a named product's behaviour among them, is the model's to sort.
const PLANTED: [(&str, Option<Class>); 17] = [
    ("The study found that OCR-based RAG improved retrieval recall by 23% over the vision-based pipeline on degraded documents.", Some(Class::Sourced)),
    ("The survey reports that modular document parsing pipelines were introduced by Google in 2019.", Some(Class::Sourced)),
    ("Microsoft's grounding feature returns bounding box coordinates in millimetres.", None),
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
        case: Case::Stated(Givens::NONE),
    }
}

fn scripted(replies: Vec<Value>) -> ScriptedModel {
    ScriptedModel {
        outputs: Mutex::new(replies.into_iter().map(|reply| reply.to_string()).collect()),
        prompts: Mutex::new(Vec::new()),
    }
}

#[test]
fn the_rules_catch_a_figure_a_quotation_or_a_cue_and_leave_the_rest_to_the_model() {
    for (statement, expected) in PLANTED {
        assert_eq!(
            rule_class(statement, "body", &Case::Stated(Givens::NONE)),
            expected,
            "{statement}"
        );
    }
}

#[test]
fn a_vendors_name_alone_is_no_cue_and_the_model_sorts_what_a_named_product_does() {
    for statement in [
        "Microsoft's grounding feature returns bounding box coordinates in millimetres.",
        "Google builds its search index in tiers.",
        "Confluent Cloud manages the brokers of a cluster.",
        "The Kafka documentation is long.",
    ] {
        assert_eq!(
            rule_class(statement, "body", &Case::Stated(Givens::NONE)),
            None,
            "{statement}"
        );
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
            rule_class(statement, "body", &Case::Stated(Givens::NONE)),
            Some(Class::Internal),
            "{statement}"
        );
    }
}

#[test]
fn a_statement_about_the_sources_of_something_is_not_a_remark_about_the_lessons_sources() {
    assert_eq!(
        rule_class(
            "The sources of latency in a network are queuing and propagation.",
            "body",
            &Case::Stated(Givens::NONE)
        ),
        None
    );
    assert_eq!(
        rule_class(
            "The sources offered no figure for the choice.",
            "body",
            &Case::Stated(Givens::NONE)
        ),
        Some(Class::Internal)
    );
}

#[test]
fn what_a_rubric_or_an_option_says_is_internal_even_with_a_figure_in_it() {
    assert_eq!(
        rule_class(
            "The answer names at least 2 trade-offs.",
            "rubric/2/description",
            &Case::Stated(Givens::NONE)
        ),
        Some(Class::Internal)
    );
    assert_eq!(
        rule_class(
            "Retries use exponential backoff three times, up to 30 seconds.",
            "options/1",
            &Case::Stated(Givens::NONE)
        ),
        Some(Class::Internal)
    );
    assert_eq!(
        rule_class(
            "Retries use exponential backoff up to 30 seconds.",
            "body",
            &Case::Stated(Givens::NONE)
        ),
        Some(Class::Sourced)
    );
}

#[test]
fn an_explanation_a_contrast_a_recap_and_an_answer_state_what_is_so_and_the_rest_a_case() {
    let lesson = json!({"blocks":[
        {"kind":"explanation"},{"kind":"worked_example"},{"kind":"contrast"},{"kind":"check"},
        {"kind":"guided_practice"},{"kind":"independent_practice"},{"kind":"reflection"},
        {"kind":"recap"},{"title":"No kind"}],
        "questions":[{"kind":"practice","prompt":"A score of 0.75 meets a threshold of 0.80.",
            "options":["Review it","Route it","Retry it","Lower it to 0.70"]}]});
    let stated = || Case::Stated(Givens::NONE);
    let hypothetical = || Case::Hypothetical;
    let cases: Vec<Case> = (0..9).map(|unit| Case::of(&lesson, unit)).collect();
    assert_eq!(
        cases,
        [
            stated(),
            hypothetical(),
            stated(),
            hypothetical(),
            hypothetical(),
            hypothetical(),
            hypothetical(),
            stated(),
            stated()
        ]
    );
    let answer = Case::of(&lesson, 9);
    assert_eq!(
        answer,
        Case::Stated(Givens::of(&lesson["questions"][0])),
        "an answer states what is so from its question's givens"
    );
    // In a case, a figure is the case's own; a quotation or an attribution
    // still needs a source.
    let figure = "Suppose 5 documents score below 0.70.";
    assert_eq!(rule_class(figure, "body", &hypothetical()), None);
    assert_eq!(rule_class(figure, "body", &stated()), Some(Class::Sourced));
    for attributed in [
        "According to the survey, most teams retrain monthly.",
        "The vendor reports that 5 documents in 100 need review.",
        "The page reads \"Total due on receipt of goods\" at the foot.",
    ] {
        assert_eq!(
            rule_class(attributed, "body", &hypothetical()),
            Some(Class::Sourced),
            "{attributed}"
        );
    }
    // An answer restates its question's figures as givens; one of its own
    // still needs a source.
    let restated = "Since 0.75 is below 0.80, review it; lowering it to 0.70 would route it.";
    assert_eq!(rule_class(restated, "explanation", &answer), None);
    assert_eq!(
        rule_class(
            "About 30% of documents need review.",
            "explanation",
            &answer
        ),
        Some(Class::Sourced)
    );
    assert_eq!(
        rule_class("0.8 is below 0.80.", "explanation", &answer),
        Some(Class::Sourced),
        "a figure is a given only as its question writes it"
    );
}

#[test]
fn quoted_wording_is_sourced_but_a_lone_apostrophe_is_not() {
    assert_eq!(
        rule_class(
            "The page reads \"Total due on receipt of goods\" at the foot.",
            "body",
            &Case::Stated(Givens::NONE)
        ),
        Some(Class::Sourced)
    );
    assert_eq!(
        rule_class(
            "The model's output is parsed by the next stage.",
            "body",
            &Case::Stated(Givens::NONE)
        ),
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

#[test]
fn a_label_word_before_a_number_is_no_figure() {
    for text in [
        "Source 0 describes the usual pattern.",
        "Option 2 is wrong because it reverses the order.",
        "Step 3: run ls",
        "Idea 1:",
        "Step 3.2 runs the check.",
        "see sources 2",
        "Choice #3 fails.",
    ] {
        assert_eq!(figures(text).count(), 0, "{text}");
    }
    assert_eq!(figures("Open port 22").collect::<Vec<_>>(), ["22"]);
    assert_eq!(
        figures("Section 230 of the act").collect::<Vec<_>>(),
        ["230"]
    );
    assert_eq!(figures("Part 15 rules").collect::<Vec<_>>(), ["15"]);
    assert_eq!(figures("version 1.2 adds").collect::<Vec<_>>(), ["1.2"]);
    assert_eq!(figures("Step 3 retries 3 times").collect::<Vec<_>>(), ["3"]);
}

#[test]
fn a_source_label_alone_needs_no_citation() {
    assert!(!needs_citation(
        "Source 0 describes the usual pattern.",
        &Givens::NONE
    ));
    assert!(needs_citation("The default port is 22.", &Givens::NONE));
}
