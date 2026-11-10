#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::lessons::course_generation_tests::ScriptedModel;
use std::sync::Mutex;

/// The replay's planted statements (evals/grounding/claim-classes/
/// replay_classes.py) and what the rules alone make of each: a figure or a
/// quotation is caught, and a statement with neither, a named product's
/// behaviour among them, is the model's to sort.
const PLANTED: [(&str, Offer); 17] = [
    ("The study found that OCR-based RAG improved retrieval recall by 23% over the vision-based pipeline on degraded documents.", Offer::Checked),
    ("The survey reports that modular document parsing pipelines were introduced by Google in 2019.", Offer::Checked),
    ("Microsoft's grounding feature returns bounding box coordinates in millimetres.", Offer::Any),
    ("Donut is an OCR-free model that goes from image to output with a single Transformer.", Offer::Any),
    ("The survey states that \"the deployment and inference costs associated with VLMs cannot be ignored.\"", Offer::Only(Class::Sourced)),
    ("In a modular pipeline, an early OCR error cannot reach later stages because each later stage re-reads the page image.", Offer::Any),
    ("Because Tesseract 5.3 switched to an LSTM recogniser, a text-only parser can recover misread digits.", Offer::Checked),
    ("In a modular pipeline, each stage hands an intermediate representation to the next, so an early error travels forward.", Offer::Any),
    ("OCR engines convert spoken audio into text.", Offer::Any),
    ("A confidence score tells a reviewer where on the page an extracted value came from.", Offer::Any),
    ("Two plus two equals five.", Offer::Any),
    ("A bounding box is measured in seconds.", Offer::Any),
    ("OCR reads the pixels of a page image into text.", Offer::Any),
    ("Two plus two equals four.", Offer::Any),
    ("Tesseract is an OCR engine that reads pixels into text.", Offer::Any),
    ("A page number identifies which page of a document a value appears on.", Offer::Any),
    ("Vision-language models were first proposed in 2017 and reached human parity on document parsing in 2021.", Offer::Checked),
];

fn body(statement: &str) -> ToSort<'_> {
    ToSort {
        statement,
        field: "body",
        case: Case::Stated(Givens::NONE),
    }
}

/// What the rules decide alone: the class of an `Offer::Only`.
fn rule_class(statement: &str, field: &str, case: &Case) -> Option<Class> {
    offer(statement, field, case).rule_class()
}

fn scripted(replies: Vec<Value>) -> ScriptedModel {
    ScriptedModel {
        outputs: Mutex::new(replies.into_iter().map(|reply| reply.to_string()).collect()),
        prompts: Mutex::new(Vec::new()),
    }
}

#[test]
fn the_rules_decide_a_quotation_bar_internal_for_a_figure_and_leave_the_rest_to_the_model() {
    for (statement, expected) in PLANTED {
        assert_eq!(
            offer(statement, "body", &Case::Stated(Givens::NONE)),
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
        offer(
            "Retries use exponential backoff up to 30 seconds.",
            "body",
            &Case::Stated(Givens::NONE)
        ),
        Offer::Checked
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
    // In a case, a figure is the case's own; a quotation still needs a source.
    let figure = "Suppose 5 documents score below 0.70.";
    assert_eq!(rule_class(figure, "body", &hypothetical()), None);
    assert_eq!(offer(figure, "body", &stated()), Offer::Checked);
    assert_eq!(
        rule_class(
            "The page reads \"Total due on receipt of goods\" at the foot.",
            "body",
            &hypothetical()
        ),
        Some(Class::Sourced)
    );
    // An answer restates its question's figures as givens; one of its own
    // still needs a source.
    let restated = "Since 0.75 is below 0.80, review it; lowering it to 0.70 would route it.";
    assert_eq!(offer(restated, "explanation", &answer), Offer::Any);
    assert_eq!(
        offer(
            "About 30% of documents need review.",
            "explanation",
            &answer
        ),
        Offer::Checked
    );
    assert_eq!(
        offer("0.8 is below 0.80.", "explanation", &answer),
        Offer::Checked,
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
    let asked = vec![("s0".to_owned(), Offer::Any), ("s3".to_owned(), Offer::Any)];
    let validator = jsonschema::JSONSchema::compile(&schema(&asked)).unwrap();
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

#[test]
fn the_answer_schema_offers_each_id_only_its_own_classes() {
    let asked = vec![
        ("s0".to_owned(), Offer::Checked),
        ("s1".to_owned(), Offer::Any),
    ];
    let validator = jsonschema::JSONSchema::compile(&schema(&asked)).unwrap();
    assert!(validator.is_valid(&json!({"s0": "reasoning", "s1": "internal"})));
    assert!(!validator.is_valid(&json!({"s0": "internal", "s1": "internal"})));
    let error = classes_of(r#"{"s0": "internal", "s1": "common"}"#, &asked).unwrap_err();
    assert!(
        error.to_string().contains("sourced, reasoning or common"),
        "{error}"
    );
}

#[tokio::test]
async fn statements_the_rules_decide_make_no_model_call() -> Result<()> {
    let model = scripted(vec![]);
    let sorted = classify(
        &model,
        &[
            body("The page reads \"Total due on receipt of goods\" at the foot."),
            ToSort {
                statement: "Each option removes the evidence.",
                field: "options/1",
                case: Case::Stated(Givens::NONE),
            },
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
        "The page reads \"Total due on receipt of goods\" at the foot.".into(),
        "The sources state that cost matters.".into(),
    ];
    texts.extend(
        ('a'..='v').map(|letter| format!("A stage hands its output to the next stage {letter}.")),
    );
    let mut to_sort: Vec<ToSort<'_>> = texts.iter().map(|text| body(text)).collect();
    to_sort[1].field = "options/0";
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
            .all(|prompt| !prompt.contains("Total due on receipt")
                && !prompt.contains("The sources")),
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
    assert!(!states_a_specific("Source 0 describes the usual pattern."));
    assert!(states_a_specific("The default port is 22."));
}

#[test]
fn a_sentence_that_only_names_a_source_needs_no_citation() {
    for sentence in [
        "The documentation is available without leaving the program.",
        "According to the survey, most readers skim.",
    ] {
        assert!(!states_a_specific(sentence), "{sentence}");
    }
}

#[test]
fn an_apostrophe_is_not_a_quotation_mark() {
    for sentence in [
        "Run pwd before ls so you see the shell's folder rather than the window's.",
        "Don't run rm until you've checked the folder's contents with ls.",
        "Run pwd before ls so you see the shell\u{2019}s folder rather than the window\u{2019}s.",
        "Don\u{2019}t run rm until you\u{2019}ve checked the folder\u{2019}s contents with ls.",
    ] {
        assert!(!states_a_specific(sentence), "{sentence}");
    }
}

#[test]
fn a_real_quotation_still_needs_a_citation() {
    for sentence in [
        "The manual says \"rm deletes files permanently, with no undo\".",
        "Remember \"the shell keeps a working directory\" always.",
        "Remember \u{201c}the shell keeps a working directory\u{201d} always.",
    ] {
        assert!(states_a_specific(sentence), "{sentence}");
    }
}

const SINGLE_QUOTED: [&str; 4] = [
    "The users' files and the admins' settings stay apart.",
    "The page reads 'a quick way to see every entry' at the top.",
    "The page reads \u{2018}a quick way to see every entry\u{2019} at the top.",
    "The page reads \u{2018}a quick way to see every entry' at the top.",
];

#[test]
fn a_single_quotation_mark_never_marks_quoted_wording() {
    for sentence in SINGLE_QUOTED {
        assert_eq!(quoted_wording(sentence).next(), None, "{sentence}");
        assert!(!states_a_specific(sentence), "{sentence}");
    }
}

#[test]
fn double_quotation_marks_mark_quoted_wording() {
    for sentence in [
        "The page reads \"a quick way to see every entry\" at the top.",
        "The page reads \u{201c}a quick way to see every entry\u{201d} at the top.",
    ] {
        assert_eq!(
            quoted_wording(sentence).collect::<Vec<_>>(),
            ["a quick way to see every entry"],
            "{sentence}"
        );
    }
}

#[test]
fn a_statement_whose_only_cue_is_single_quoted_wording_is_left_to_the_model() {
    let stated = Case::Stated(Givens::NONE);
    for statement in [
        "The page reads 'Total due on receipt of goods' at the foot.",
        "The page reads \u{2018}Total due on receipt of goods\u{2019} at the foot.",
        "The users' files and the admins' settings stay apart.",
    ] {
        assert_eq!(rule_class(statement, "body", &stated), None, "{statement}");
        assert_eq!(
            rule_class(statement, "body", &Case::Hypothetical),
            None,
            "{statement}"
        );
    }
    assert_eq!(
        rule_class(
            "The page reads \"Total due on receipt of goods\" at the foot.",
            "body",
            &stated
        ),
        Some(Class::Sourced)
    );
}

fn in_a_section() -> Case {
    Case::Stated(Givens::NONE)
}

#[test]
fn a_sentence_that_names_the_documentation_without_a_mark_is_the_models_to_sort() {
    assert_eq!(
        rule_class(
            "The documentation is available from the tool's own help screen.",
            "body",
            &in_a_section()
        ),
        None
    );
}

#[test]
fn an_imperative_to_state_something_is_the_models_to_sort() {
    assert_eq!(
        rule_class(
            "State your assumptions before you begin.",
            "body",
            &in_a_section()
        ),
        None
    );
}

#[test]
fn a_file_that_is_not_found_is_the_models_to_sort() {
    assert_eq!(
        rule_class(
            "If the file is not found, the tool prints nothing.",
            "body",
            &in_a_section()
        ),
        None
    );
}

#[test]
fn a_remark_that_opens_the_sources_is_the_models_to_sort() {
    for statement in [
        "The sources do not cover this case.",
        "  The sources state that the choice depends on document quality.",
        "The other options each remove the evidence needed to check a table.",
    ] {
        assert_eq!(
            rule_class(statement, "body", &in_a_section()),
            None,
            "{statement}"
        );
    }
}

#[test]
fn a_span_between_double_quotes_under_the_quotation_floor_is_no_quotation() {
    for statement in [
        "The tool shows \"Changes saved\" when it is done.",
        "The manual says \"rm deletes files permanently\".",
    ] {
        assert_eq!(
            rule_class(statement, "body", &in_a_section()),
            None,
            "{statement}"
        );
        assert_eq!(
            rule_class(statement, "body", &Case::Hypothetical),
            None,
            "{statement}"
        );
    }
}

#[test]
fn a_figure_beyond_the_givens_is_a_mark_and_a_given_is_not() {
    let givens = Givens::of(&json!({"prompt": "A score of 0.75 meets a threshold of 0.80."}));
    let case = Case::Stated(givens);
    assert_eq!(
        offer("About 30 documents need review.", "explanation", &case),
        Offer::Checked
    );
    assert_eq!(
        offer("Since 0.75 is below 0.80, review it.", "explanation", &case),
        Offer::Any
    );
}

#[test]
fn in_a_hypothetical_case_a_figure_alone_is_no_mark_and_a_quotation_at_the_floor_is() {
    assert_eq!(
        rule_class(
            "Suppose 5 documents score below 0.70.",
            "body",
            &Case::Hypothetical
        ),
        None
    );
    assert_eq!(
        rule_class(
            "The page reads \"alpha beta gamma delta eps\" at the top.",
            "body",
            &Case::Hypothetical
        ),
        Some(Class::Sourced)
    );
}

#[test]
fn the_prose_between_two_short_quotations_is_no_quotation() {
    for row in [
        "Choose \"Open\" from the menu and then pick \"Save as copy\" from the list that opens.",
        "Choose \u{201c}Open\u{201d} from the menu and then pick \u{201c}Save as copy\u{201d} from the list that opens.",
    ] {
        assert_eq!(quoted_wording(row).next(), None, "{row}");
        assert_eq!(rule_class(row, "body", &in_a_section()), None, "{row}");
    }
}

#[test]
fn a_mark_after_a_figure_closes_nothing() {
    const S2: &str = "The screen is 27\" across, so the whole menu fits and shows \"Open\" first.";
    assert_eq!(quoted_wording(S2).next(), None);
    assert_eq!(rule_class(S2, "body", &Case::Hypothetical), None);
}

#[test]
fn a_quotation_after_a_short_one_is_found_whole() {
    const S3: &str =
        "Choose \"Open\", then read \"the page that reads text from images\" at the top.";
    assert_eq!(
        quoted_wording(S3).collect::<Vec<_>>(),
        ["the page that reads text from images"]
    );
}

#[test]
fn a_mark_at_the_start_of_the_text_or_after_an_opening_bracket_opens() {
    assert_eq!(
        quoted_wording("\"The shell keeps a working directory,\" the guide begins.")
            .collect::<Vec<_>>(),
        ["The shell keeps a working directory,"]
    );
    assert_eq!(
        quoted_wording("(see \"the page that reads text from images\")").collect::<Vec<_>>(),
        ["the page that reads text from images"]
    );
}

const OWN_FIGURES_SENTENCE: &str = "The lesson's own figures are the quantities its own example, scenario or exercise sets, and the results calculated from them.";

#[tokio::test]
async fn a_figure_in_a_part_that_states_what_is_so_is_the_model_s_to_sort() -> Result<()> {
    let model = scripted(vec![json!({"s0": "reasoning"})]);
    let sorted = classify(&model, &[body("Dividing 12 by 48 gives 0.25.")]).await?;
    assert_eq!(sorted, vec![(Class::Reasoning, ClassifiedBy::Model)]);
    Ok(())
}

#[tokio::test]
async fn a_figure_statement_the_model_calls_internal_is_read_as_reasoning() -> Result<()> {
    let model = scripted(vec![json!({"s0": "internal"}), json!({"s0": "reasoning"})]);
    let sorted = classify(
        &model,
        &[body("The sample batch holds 12 of the 48 units.")],
    )
    .await?;
    assert_eq!(sorted, vec![(Class::Reasoning, ClassifiedBy::Model)]);
    assert_eq!(
        model.prompts.lock().unwrap().len(),
        1,
        "the model was asked again"
    );
    Ok(())
}

#[tokio::test]
async fn a_statement_without_a_figure_the_model_calls_internal_stays_internal() -> Result<()> {
    let model = scripted(vec![json!({"s0": "internal"})]);
    let sorted = classify(&model, &[body("The sample batch is set by the exercise.")]).await?;
    assert_eq!(sorted, vec![(Class::Internal, ClassifiedBy::Model)]);
    assert_eq!(model.prompts.lock().unwrap().len(), 1);
    Ok(())
}

#[tokio::test]
async fn quoted_wording_is_still_sourced_by_the_rule() -> Result<()> {
    let model = scripted(vec![]);
    let sorted = classify(
        &model,
        &[body(
            "The page reads \"Total due on receipt of goods\" at the foot.",
        )],
    )
    .await?;
    assert_eq!(sorted, vec![(Class::Sourced, ClassifiedBy::Rule)]);
    assert!(model.prompts.lock().unwrap().is_empty());
    Ok(())
}

#[test]
fn the_classifier_is_told_what_the_lesson_s_own_figures_are() {
    assert!(SYSTEM.contains(OWN_FIGURES_SENTENCE), "{SYSTEM}");
    assert!(
        SYSTEM.contains(
            "They are not specifics, and no source states them: a statement that calculates with them is reasoning, and one that only sets them is internal."
        ),
        "{SYSTEM}"
    );
}
