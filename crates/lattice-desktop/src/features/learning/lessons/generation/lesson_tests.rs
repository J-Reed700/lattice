#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
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
        "quote":if cited { EXCERPT } else { "" }})
}

fn question(kind: &str, prompt: &str, cited: bool) -> Value {
    json!({"kind":kind,"prompt":prompt,"options":["One","Two","Three","Four"],"correctIndex":1,"explanation":"Two follows.",
        "sourceIndex":if cited { json!(0) } else { Value::Null },
        "quote":if cited { EXCERPT } else { "" }})
}

/// Seven sections, two sharing a title, one too short, no reflection; five
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
    blocks.push(block("recap", "Recap", "Too short.", cited));
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
