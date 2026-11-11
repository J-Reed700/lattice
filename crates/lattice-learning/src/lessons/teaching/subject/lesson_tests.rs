//! Which check of a lesson pass is the one that reads it against its plan.
#![allow(clippy::indexing_slicing)]
use super::*;
use crate::review_findings::PlanCheck;

fn contradiction(subject: &str, note: &str) -> ReviewFinding {
    ReviewFinding::plan(
        PlanCheck::Contradiction,
        subject,
        FindingSite::Section(0),
        note,
    )
}

#[test]
fn the_plan_check_stands_where_the_gate_is_told_it_does() {
    assert_eq!(BESIDE[PLAN_CHECK], contract::DECIDES);
}

#[test]
fn a_finding_both_asks_give_appears_once() {
    let first = vec![
        contradiction("beads", "The beads are counted twice."),
        contradiction("rope", "The rope is cut twice."),
    ];
    let second = vec![
        contradiction("rope", "The rope is cut twice, once in each section."),
        contradiction("kettle", "The kettle boils twice."),
    ];
    let all = plan_findings_of(first.clone(), second);
    let ids: HashSet<_> = all.iter().map(ReviewFinding::id).collect();
    assert_eq!(ids.len(), 3);
    assert_eq!(all.len(), 3);
    assert_eq!(all[..2], first[..]);
}

const WRITTEN: &str = "This saved reference describes the exact behavior of the example tool.";
const RESEARCHED: &str =
    "Replication copies every write to several machines so that one failure loses nothing at all.";

fn source(id: &str, excerpt: &str) -> crate::dto::LearningSourceDto {
    crate::dto::LearningSourceDto {
        id: id.into(),
        title: id.into(),
        url: None,
        acquired_at: 0,
        excerpt: excerpt.into(),
    }
}

/// The writer's prompt names one source; research then numbered a second.
fn numbered() -> Vec<crate::dto::LearningSourceDto> {
    vec![source("written", WRITTEN), source("researched", RESEARCHED)]
}

fn writer_context() -> Value {
    json!({"sources":[{"sourceIndex":0,"excerpt":WRITTEN}]})
}

fn cited(index: u64, quote: &str) -> Value {
    json!({"blocks":[{"kind":"explanation","title":"Example","body":"A teaching explanation.","sourceIndex":index,"quote":quote}],"questions":[]})
}

fn quote_checks_of(numbered: &[crate::dto::LearningSourceDto], candidate: &Value) -> Vec<Value> {
    let _ = numbered;
    let sources = numbered_references();
    let references = ReferenceCollection::lexical(&sources).unwrap();
    LessonSubject::new(&references, &[], Held::default())
        .quote_checks(candidate, &writer_context(), 1)
        .unwrap()
}

fn numbered_references() -> Vec<crate::dto::LearningSourceDto> {
    vec![source("written", WRITTEN)]
}

#[test]
fn a_quotation_of_a_source_research_numbered_holds_though_the_writer_was_not_given_it() {
    let candidate = cited(1, RESEARCHED);
    assert!(quote_checks_of(&numbered(), &candidate).is_empty());

    let sources = numbered_references();
    let references = ReferenceCollection::lexical(&sources).unwrap();
    let subject = LessonSubject::new(&references, &[], Held::default());
    let mut settled = candidate.clone();
    subject.settle_quotes(&mut settled, &writer_context(), 1);
    assert_eq!(settled, candidate);
}

#[test]
fn a_quotation_the_numbered_source_does_not_hold_still_fails() {
    let checks = quote_checks_of(&numbered(), &cited(1, WRITTEN));
    assert_eq!(checks.len(), 1);
    assert_eq!(checks[0]["path"], "blocks[0]");
}

#[test]
fn a_source_index_past_the_numbered_sources_still_fails() {
    let checks = quote_checks_of(&numbered(), &cited(2, RESEARCHED));
    assert_eq!(checks.len(), 1);
    assert_eq!(
        checks[0]["error"],
        "The citation must select a supplied sourceIndex."
    );
}
