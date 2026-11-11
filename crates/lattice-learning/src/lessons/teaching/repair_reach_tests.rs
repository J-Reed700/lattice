//! Which parts a repair's change may touch, and what the repair is told of a finding's part.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::review_findings::PlanCheck;

const PLANNED: &str = "plan/planned/action-1";

fn planned_at_section_1() -> ReviewFinding {
    ReviewFinding::plan(
        PlanCheck::Planned,
        "action-1",
        FindingSite::Section(1),
        "Alpha holds the line.",
    )
}

fn used_at_section_1() -> ReviewFinding {
    ReviewFinding::plan(
        PlanCheck::Used,
        "beta-idea",
        FindingSite::Section(1),
        "Beta holds the line.",
    )
}

#[test]
fn a_planned_item_may_be_added_in_another_part() {
    let finding = planned_at_section_1();
    let may = reach([&finding]);
    assert_eq!(may(Some(PLANNED), "/blocks/3"), Ok(()));
    assert_eq!(may(Some(PLANNED), "/blocks/1"), Ok(()));
    assert_eq!(may(Some(PLANNED), "/questions/0"), Ok(()));
}

#[test]
fn a_use_before_teaching_is_still_mended_in_its_own_part_only() {
    let finding = used_at_section_1();
    let id = finding.id();
    let may = reach([&finding]);
    assert_eq!(may(Some(id.as_str()), "/blocks/1"), Ok(()));
    assert!(may(Some(id.as_str()), "/blocks/3").is_err());
}

#[test]
fn a_planned_item_finding_offers_the_repair_no_part() {
    let finding = planned_at_section_1();
    let entry = Issue {
        finding: &finding,
        last_attempt: None,
    }
    .entry();
    assert!(entry["part"].is_null());
    assert_eq!(entry["id"], PLANNED);
}

#[test]
fn a_finding_with_a_part_still_offers_it() {
    let finding = used_at_section_1();
    let entry = Issue {
        finding: &finding,
        last_attempt: None,
    }
    .entry();
    assert_eq!(entry["part"], "/blocks/1");
}

#[test]
fn only_a_finding_about_the_whole_material_or_a_missing_item_mends_anywhere() {
    assert!(planned_at_section_1().mends_anywhere());
    assert!(ReviewFinding::reviewer(1, None, "Gamma holds the line.").mends_anywhere());
    assert!(
        ReviewFinding::reviewer(2, Some(FindingSite::Lesson), "Gamma holds the line.")
            .mends_anywhere()
    );
    assert!(!used_at_section_1().mends_anywhere());
    assert!(
        !ReviewFinding::reviewer(3, Some(FindingSite::Section(1)), "Gamma holds the line.")
            .mends_anywhere()
    );
}
