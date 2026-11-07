#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::features::learning::lessons::course_generation_tests::length_bounds;
use serde_json::{json, Value};

const EXCERPT: &str =
    "Water boils at 99.97 degrees Celsius at one standard atmosphere of pressure.";

fn source() -> LearningSourceDto {
    LearningSourceDto {
        id: "source-a".into(),
        title: "Steam tables".into(),
        url: None,
        excerpt: EXCERPT.into(),
        acquired_at: 1,
    }
}

fn block(kind: &str, title: &str, body: &str, cited: bool) -> Value {
    json!({"kind":kind,"title":title,"body":body,"rubric":[],
        "sourceIndex":if cited { json!(0) } else { Value::Null },
        "quote":if cited { EXCERPT } else { "" },"citations":[]})
}

fn question(kind: &str, prompt: &str, cited: bool) -> Value {
    json!({"kind":kind,"prompt":prompt,"options":["One","Two","Three","Four"],"correctIndex":1,"explanation":"Two follows.",
        "sourceIndex":if cited { json!(0) } else { Value::Null },
        "quote":if cited { EXCERPT } else { "" },"citations":[]})
}

/// Seven sections, two sharing a title, one with no body, no reflection; five
/// questions, one of them practice.
fn off_shape_value(cited: bool) -> Value {
    let long = "A substantive explanation. ".repeat(30);
    let mut blocks: Vec<_> = [
        "explanation",
        "explanation",
        "worked_example",
        "worked_example",
        "guided_practice",
        "independent_practice",
    ]
    .iter()
    .enumerate()
    .map(|(i, kind)| block(kind, &format!("Part {i}"), &long, cited))
    .collect();
    blocks[1]["title"] = json!("Part 0");
    blocks.push(block("recap", "Recap", "", cited));
    let questions: Vec<_> = ["practice", "quiz", "quiz", "test", "test"]
        .iter()
        .enumerate()
        .map(|(i, kind)| question(kind, &format!("Question {i}?"), cited))
        .collect();
    json!({"blocks":blocks,"questions":questions})
}

fn off_shape(cited: bool) -> GeneratedLesson {
    serde_json::from_value(off_shape_value(cited)).unwrap()
}

#[test]
fn a_lesson_off_its_shape_is_saved_and_its_rules_are_findings_not_refusals() {
    let sources = [source()];
    let owned = HashSet::from(["source-a"]);
    let (prepared, citations) = assemble(off_shape(true), &sources, &owned).unwrap();
    assert_eq!((prepared.blocks.len(), prepared.questions.len()), (7, 5));
    assert_eq!(prepared.keys.len(), 5);
    assert!(citations.is_empty());
    // The rules come back as findings for the user, not as an error.
    let findings = crate::features::learning::teaching::structural_findings(
        &lesson_schema(1),
        &off_shape_value(true).to_string(),
    )
    .unwrap();
    let keys: std::collections::BTreeSet<_> = findings.iter().map(|f| f.key().id()).collect();
    let expected = [
        "structure:lesson",
        "structure:section-1",
        "structure:section-4",
        "structure:section-5",
        "structure:section-6",
    ]
    .map(String::from);
    assert_eq!(keys, std::collections::BTreeSet::from(expected));
}

#[test]
fn a_quotation_that_does_not_match_keeps_its_source_and_waits_for_the_user() {
    let mut lesson = off_shape(true);
    lesson.blocks[2].quote = "Water boils at exactly one hundred degrees in every kitchen.".into();
    let sources = [source()];
    let (prepared, citations) = assemble(lesson, &sources, &HashSet::from(["source-a"])).unwrap();
    assert_eq!(prepared.blocks[2].source_ids, ["source-a"]);
    assert_eq!(citations.len(), 1);
    assert_eq!(citations[0].key().id(), "quotation:section-2");
    assert!(citations[0].note.contains("blocks[2]"));
}

#[test]
fn a_cited_sentence_adds_its_source_and_one_that_cannot_be_confirmed_is_a_finding() {
    const KETTLE: &str = "The kettle whistles once the water inside it has boiled away.";
    let kettle = LearningSourceDto {
        id: "source-b".into(),
        title: "Kettle notes".into(),
        url: None,
        excerpt: KETTLE.into(),
        acquired_at: 2,
    };
    let mut lesson = off_shape(true);
    lesson.blocks[2].citations = vec![
        GeneratedCitation {
            sentence: "A kettle whistles when it runs dry.".into(),
            source_index: Some(1),
            quote: KETTLE.into(),
        },
        GeneratedCitation {
            sentence: "Water boils at 50 degrees Celsius.".into(),
            source_index: Some(0),
            quote: "Water boils at exactly fifty degrees in every kitchen.".into(),
        },
    ];
    let owned = HashSet::from(["source-a", "source-b"]);

    let (prepared, findings) = assemble(lesson, &[source(), kettle], &owned).unwrap();

    assert_eq!(prepared.blocks[2].source_ids, ["source-a", "source-b"]);
    assert_eq!(prepared.blocks[3].source_ids, ["source-a"]);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].key().id(), "quotation:section-2");
    assert!(findings[0].note.contains("Water boils at 50 degrees"));
}

#[test]
fn a_question_citing_a_numbered_source_the_program_does_not_own_is_refused() {
    const KETTLE: &str = "The kettle whistles once the water inside it has boiled away.";
    let kettle = LearningSourceDto {
        id: "source-b".into(),
        title: "Kettle notes".into(),
        url: None,
        excerpt: KETTLE.into(),
        acquired_at: 2,
    };
    let mut lesson = off_shape(true);
    lesson.questions[1].citations = vec![GeneratedCitation {
        sentence: "A kettle whistles when it runs dry.".into(),
        source_index: Some(1),
        quote: KETTLE.into(),
    }];

    let error = assemble(lesson, &[source(), kettle], &HashSet::from(["source-a"])).unwrap_err();

    assert!(
        matches!(&error, AppError::ModelAnswerUnusable(message)
            if message == "A question referred to a source outside this program."),
        "{error:?}"
    );
}

#[test]
fn the_schema_asks_for_a_citation_list_per_part_and_none_without_sources() {
    let pointer = "/properties/blocks/items/properties/citations/maxItems";
    assert_eq!(lesson_schema(2).pointer(pointer), Some(&json!(24)));
    assert_eq!(lesson_schema(0).pointer(pointer), Some(&json!(0)));
    for part in ["blocks", "questions"] {
        let required = lesson_schema(2)
            .pointer(&format!("/properties/{part}/items/required"))
            .cloned()
            .unwrap();
        assert!(required.as_array().unwrap().contains(&json!("citations")));
    }
}

#[test]
fn an_invented_citation_in_a_course_without_sources_is_dropped_and_reported() {
    let mut lesson = off_shape(false);
    lesson.questions[0].source_index = Some(0);
    lesson.questions[0].quote = "An imaginary textbook says this is true.".into();
    let (prepared, citations) = assemble(lesson, &[], &HashSet::new()).unwrap();
    assert!(prepared.questions[0].source_ids.is_empty());
    assert_eq!(citations[0].key().id(), "quotation:question-0");
}

#[test]
fn a_sourced_part_that_names_no_source_cannot_be_saved_and_is_not_called_invalid_input() {
    let mut lesson = off_shape(true);
    lesson.blocks[0].source_index = Some(3);
    let error = assemble(lesson, &[source()], &HashSet::from(["source-a"])).unwrap_err();
    assert!(
        matches!(error, AppError::ModelAnswerUnusable(_)),
        "{error:?}"
    );
    assert!(!error.to_string().contains("check your input"));
}

#[test]
fn the_lesson_schema_bounds_no_text_but_a_blank() {
    for sources in [0, 2] {
        let bounds = length_bounds(&lesson_schema(sources));
        assert!(bounds.is_empty(), "{sources} sources: {bounds:?}");
    }
}

#[test]
fn a_lessons_text_of_any_length_is_no_schema_finding() {
    let schema = lesson_schema(1);
    let kinds = [
        "explanation",
        "worked_example",
        "contrast",
        "check",
        "explanation",
        "worked_example",
        "check",
        "guided_practice",
        "independent_practice",
        "reflection",
        "recap",
    ];
    let blocks: Vec<_> = kinds
        .iter()
        .enumerate()
        .map(|(i, kind)| {
            let body = "A substantive explanation. ".repeat(30);
            block(kind, &format!("Part {i}"), &body, true)
        })
        .collect();
    let questions: Vec<_> = ["practice", "practice", "quiz", "quiz", "test", "test"]
        .iter()
        .enumerate()
        .map(|(i, kind)| question(kind, &format!("Question {i}?"), true))
        .collect();
    let mut lesson = json!({"blocks":blocks,"questions":questions});
    let schema_findings = |lesson: &Value| -> Vec<String> {
        crate::features::learning::teaching::structural_findings(&schema, &lesson.to_string())
            .unwrap()
            .into_iter()
            .map(|finding| finding.note)
            .filter(|note| note.starts_with("Fix the schema violation"))
            .collect()
    };
    let found = schema_findings(&lesson);
    assert!(found.is_empty(), "the fixture is not a lesson: {found:?}");

    lesson["blocks"][0]["title"] = json!("A title that runs on and on. ".repeat(10));
    lesson["blocks"][1]["body"] = json!("Short.");
    lesson["blocks"][2]["quote"] = json!(EXCERPT.repeat(20));
    lesson["blocks"][3]["citations"] = json!([
        {"sentence":"A cited sentence that runs on. ".repeat(30),"sourceIndex":0,"quote":EXCERPT},
        {"sentence":"No","sourceIndex":0,"quote":EXCERPT}
    ]);
    lesson["questions"][0]["prompt"] = json!("Which choice follows from the premise? ".repeat(40));
    lesson["questions"][0]["options"][0] = json!("A long answer choice. ".repeat(40));
    lesson["questions"][1]["explanation"] =
        json!("Because the premise fixes the outcome. ".repeat(60));
    let found = schema_findings(&lesson);
    assert!(found.is_empty(), "{found:?}");
}
