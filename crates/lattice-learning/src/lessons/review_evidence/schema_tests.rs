use super::*;
use crate::lessons::course_generation_tests::property_count;

fn issues(count: usize) -> Vec<ReviewFinding> {
    (0..count)
        .map(|i| ReviewFinding::reviewer(u32::try_from(i).unwrap(), None, format!("Proposal {i}.")))
        .collect()
}

fn keys(count: usize) -> Vec<String> {
    (0..count).map(|i| format!("issue-{i}")).collect()
}

fn evidence() -> HashMap<String, EvidenceKind> {
    HashMap::from([
        ("blocks-0".into(), EvidenceKind::Candidate),
        ("reference-0".into(), EvidenceKind::Reference),
    ])
}

fn withheld(key: &str) -> Value {
    json!({"issueId":key,"verdict":"not_established","basis":"external_fact","evidenceIds":[],"requirementId":null,"partId":"lesson","reason":"No passage establishes this correction."})
}

fn grounded(key: &str) -> Value {
    json!({"issueId":key,"verdict":"actionable","basis":"internal_conflict","evidenceIds":["blocks-0"],"requirementId":null,"partId":"blocks-0","reason":"The candidate contradicts itself."})
}

fn answer(decisions: Vec<Value>) -> String {
    json!({ "decisions": decisions }).to_string()
}

fn established(raw: &str, count: usize) -> Result<Vec<ReviewFinding>> {
    accepted(raw, &issues(count), Beside::NONE, &evidence()).map(|checked| checked.established)
}

#[test]
fn the_schema_is_as_large_for_fifty_issues_as_for_one() {
    let ids = ["blocks-0", "reference-0", "requirements"].map(String::from);
    let one = schema(&keys(1), &ids, &[], &[]);
    let fifty = schema(&keys(50), &ids, &[], &[]);
    assert_eq!(property_count(&one), property_count(&fifty));
    let listed = &fifty["properties"]["decisions"]["items"]["properties"]["issueId"]["enum"];
    assert_eq!(listed.as_array().map(Vec::len), Some(50));
}

#[test]
fn a_complete_answer_in_any_order_is_accepted() -> Result<()> {
    let raw = answer(vec![withheld("issue-1"), grounded("issue-0")]);
    let found = established(&raw, 2)?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].note, "Proposal 0.");
    Ok(())
}

#[test]
fn a_missing_issue_is_refused_with_its_key() {
    let error = established(&answer(vec![withheld("issue-0")]), 2).unwrap_err();
    assert!(error.to_string().contains("issue-1"), "{error}");
    assert!(!error.to_string().contains("issue-0"), "{error}");
}

#[test]
fn a_repeated_issue_is_refused_with_its_key() {
    let raw = answer(vec![
        withheld("issue-0"),
        withheld("issue-1"),
        withheld("issue-1"),
    ]);
    let error = established(&raw, 2).unwrap_err();
    assert!(error.to_string().contains("issue-1"), "{error}");
    assert!(!error.to_string().contains("issue-0"), "{error}");
}

#[test]
fn a_decision_for_an_issue_that_was_not_asked_is_refused_with_its_key() {
    let raw = answer(vec![withheld("issue-0"), withheld("issue-7")]);
    let error = established(&raw, 1).unwrap_err();
    assert!(error.to_string().contains("issue-7"), "{error}");
}

#[test]
fn the_decision_schema_requires_a_part_among_the_candidates_or_lesson() {
    let ids = ["blocks-0", "questions-1", "reference-0", "requirements"].map(String::from);
    let schema = schema(&keys(1), &ids, &[], &[]);
    let decision = &schema["properties"]["decisions"]["items"];
    assert!(decision["required"]
        .as_array()
        .unwrap()
        .contains(&json!("partId")));
    assert_eq!(
        decision["properties"]["partId"]["enum"],
        json!(["blocks-0", "questions-1", "lesson"])
    );
}

#[test]
fn the_decision_schema_bounds_no_text_but_a_blank() {
    let ids = ["blocks-0", "requirements"].map(String::from);
    let bounds =
        crate::lessons::course_generation_tests::length_bounds(&schema(&keys(1), &ids, &[], &[]));
    assert!(bounds.is_empty(), "{bounds:?}");
}
