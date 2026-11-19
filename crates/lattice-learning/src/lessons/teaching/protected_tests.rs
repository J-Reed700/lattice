//! A finding that the lesson lacks teaching is corrected by adding sentences, so protection does not hold it.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::review_findings::PlanCheck;
use serde_json::json;

const PROTECTED: &str = "The protected sentence.";
const OPEN: &str = "An open sentence.";
const ADDED: &str = "An added sentence.";

fn lesson(body: &str) -> Value {
    json!({"blocks":[{
        "kind": "explanation",
        "title": "Opening",
        "body": body,
        "rubric": [],
        "sourceIndex": 0,
        "quote": "An excerpt."
    }],"questions":[]})
}

fn protect() -> Protected {
    Protected {
        unit: 0,
        field: "body".into(),
        quote: PROTECTED.into(),
        released: false,
    }
}

fn used(passage: &str) -> ReviewFinding {
    ReviewFinding::plan(
        PlanCheck::Used,
        "an-idea",
        FindingSite::Section(0),
        "It is used before it is taught.",
    )
    .with_passage(passage)
}

fn planned(site: FindingSite) -> ReviewFinding {
    ReviewFinding::plan(
        PlanCheck::Planned,
        "an-idea",
        site,
        "The lesson does not teach it.",
    )
}

fn reviewer(site: FindingSite) -> ReviewFinding {
    ReviewFinding::reviewer(1, Some(site), "It is wrong.")
}

#[test]
fn a_finding_that_a_use_lacks_teaching_is_not_held_on_a_protected_passage() {
    let candidate = lesson(&format!("{OPEN} {PROTECTED}"));
    let protected = [protect()];
    assert!(on_protected(&candidate, &[used(PROTECTED)], &protected).is_empty());
    let other = reviewer(FindingSite::Section(0)).with_passage(PROTECTED);
    assert_eq!(
        on_protected(&candidate, std::slice::from_ref(&other), &protected),
        vec![other.id()]
    );
}

#[test]
fn a_finding_that_the_lesson_lacks_something_is_not_held_when_something_is_protected() {
    let candidate = lesson(&format!("{OPEN} {PROTECTED}"));
    let protected = [protect()];
    assert!(on_protected(&candidate, &[planned(FindingSite::Lesson)], &protected).is_empty());
    let other = reviewer(FindingSite::Lesson);
    assert_eq!(
        on_protected(&candidate, std::slice::from_ref(&other), &protected),
        vec![other.id()]
    );
}

fn confined(
    previous: &str,
    repaired: &str,
    finding: &ReviewFinding,
) -> std::result::Result<String, Vec<String>> {
    let (previous, repaired) = (lesson(previous), lesson(repaired));
    let touched = BTreeMap::from([("/blocks/0".to_owned(), vec![finding])]);
    confine_review(&previous, &repaired, &touched, &[protect()])
        .unwrap()
        .map(|out| out["blocks"][0]["body"].as_str().unwrap().to_owned())
}

#[test]
fn a_sentence_added_after_a_protected_one_is_kept() {
    let kept = confined(
        &format!("{OPEN} {PROTECTED}"),
        &format!("{OPEN} {PROTECTED} {ADDED}"),
        &used(PROTECTED),
    )
    .unwrap();
    assert_eq!(kept, format!("{OPEN} {PROTECTED} {ADDED}"));
}

#[test]
fn a_sentence_added_before_a_protected_one_is_kept() {
    let kept = confined(
        &format!("{PROTECTED} {OPEN}"),
        &format!("{ADDED} {PROTECTED} {OPEN}"),
        &used(PROTECTED),
    )
    .unwrap();
    assert_eq!(kept, format!("{ADDED} {PROTECTED} {OPEN}"));
}

#[test]
fn a_sentence_added_beside_the_only_sentence_when_all_are_protected_is_kept() {
    let kept = confined(PROTECTED, &format!("{PROTECTED} {ADDED}"), &used(PROTECTED)).unwrap();
    assert_eq!(kept, format!("{PROTECTED} {ADDED}"));
}

#[test]
fn a_change_that_rewrites_a_protected_sentence_is_refused_with_its_reason() {
    let previous = format!("{OPEN} {PROTECTED}");
    let refused = confined(
        &previous,
        &format!("{OPEN} A rewritten sentence."),
        &used(PROTECTED),
    );
    assert_eq!(
        refused,
        Err(vec![format!(
            "it changes \"{PROTECTED}\", which states a checked claim and must stay as it is"
        )])
    );
}

const SHELF: &str = "The kettle sits on the top shelf by the window.";
const HOOK: &str = "A long rope hangs on the hook beside the kettle.";
const JAR: &str = "Glass beads fill the jar on the lowest shelf.";

fn cited_lesson(first: &str, second: &str) -> Value {
    let mut cited = lesson(&format!("{SHELF} {HOOK} {JAR}"));
    cited["blocks"][0]["citations"] = json!([
        {"sentence": first, "sourceIndex": 0, "quote": "An excerpt."},
        {"sentence": second, "sourceIndex": 0, "quote": "Another excerpt."},
    ]);
    cited
}

#[test]
fn citations_naming_other_sentences_of_the_body_survive_a_repair_of_the_whole_section() {
    let previous = cited_lesson(SHELF, HOOK);
    let repaired = cited_lesson(HOOK, JAR);
    let finding = reviewer(FindingSite::Section(0));
    let touched = BTreeMap::from([("/blocks/0".to_owned(), vec![&finding])]);
    let out = confine_review(&previous, &repaired, &touched, &[])
        .unwrap()
        .unwrap();
    assert_eq!(
        out["blocks"][0]["citations"],
        repaired["blocks"][0]["citations"]
    );
}
