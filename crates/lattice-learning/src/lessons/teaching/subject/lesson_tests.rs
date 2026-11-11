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
