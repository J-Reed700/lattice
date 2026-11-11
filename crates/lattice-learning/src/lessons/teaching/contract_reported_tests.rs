//! What the plan check is given of the review's reports.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;

fn prompt() -> Value {
    json!({"task":"Check","sections":[],"questions":[]})
}

fn report(site: Option<FindingSite>, note: &str, passage: Option<&str>) -> ReviewFinding {
    let mut finding =
        ReviewFinding::plan(PlanCheck::Contradiction, "beads", FindingSite::Lesson, note);
    finding.site = site;
    finding.passage = passage.map(str::to_owned);
    finding
}

#[test]
fn without_reports_the_request_and_the_system_text_are_as_they_were() {
    let plain = prompt();
    assert_eq!(with_reported(plain.clone(), &[]), plain);
    assert!(with_reported(plain, &[]).get("reported").is_none());
    assert_eq!(system_for(&[]), SYSTEM);
}

#[test]
fn a_report_gives_its_part_its_defect_and_its_words() {
    let first = report(
        Some(FindingSite::Section(1)),
        "The beads are counted twice.",
        Some("counted twice"),
    );
    let asked = with_reported(prompt(), &[&first]);
    assert_eq!(
        asked["reported"],
        json!([{"part":"Section 2","defect":"The beads are counted twice.","words":"counted twice"}])
    );
}

#[test]
fn a_report_without_a_site_or_about_the_whole_lesson_names_no_part_and_one_without_a_passage_no_words(
) {
    let whole = report(Some(FindingSite::Lesson), "The rope is cut twice.", None);
    let unplaced = report(None, "The shelf is full.", Some("full"));
    let asked = with_reported(prompt(), &[&whole, &unplaced]);
    assert_eq!(
        asked["reported"],
        json!([
            {"defect":"The rope is cut twice."},
            {"defect":"The shelf is full.","words":"full"}
        ])
    );
}

#[test]
fn the_system_text_with_reports_is_the_plain_one_followed_by_the_sentence() {
    let one = report(None, "The kettle is cold.", None);
    let system = system_for(&[&one]);
    assert_eq!(system, format!("{SYSTEM}{REPORTED}"));
}
