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

const KEPT_TO_THE_CONTRACT: [&str; 2] = [
    "Added or changed text uses a term, or asks the learner to do or use something, only where the lesson has taught it by that point; otherwise it says there what the term means or how to do it.",
    "It never says again what the sentence before or after it already says.",
];

fn asked<'a>(edits: bool, schema: &'a Value, wording: &'a Wording) -> Request<'a> {
    Request {
        system: "",
        schema,
        authoring: schema,
        candidate: schema,
        quote_checks: &[],
        issues: Vec::new(),
        edits,
        wording,
        output_tokens: 0,
    }
}

#[test]
fn the_edits_form_holds_what_a_repair_adds_to_what_the_lesson_is_held_to() {
    let (schema, wording) = (Value::Null, Wording::default());
    let system = Edits.system(&asked(true, &schema, &wording));
    for sentence in KEPT_TO_THE_CONTRACT {
        assert!(system.contains(sentence), "{sentence}");
    }
}

#[test]
fn the_rewrite_form_holds_what_a_repair_adds_to_what_the_lesson_is_held_to() {
    let (schema, wording) = (Value::Null, Wording::default());
    let prompt = Rewrite.prompt(&asked(false, &schema, &wording), Value::Null);
    let instruction = prompt["instruction"].as_str().unwrap();
    for sentence in KEPT_TO_THE_CONTRACT {
        assert!(instruction.contains(sentence), "{sentence}");
    }
}

#[test]
fn the_repair_forms_keep_their_own_words_sentence_and_their_instructions() {
    let (schema, wording) = (Value::Null, Wording::default());
    let system = Edits.system(&asked(true, &schema, &wording));
    let own_words = "added or changed text teaches in the lesson's own words and never mentions the course plan, the review, the sources' excerpts or these instructions. ";
    assert!(system.contains(own_words), "{system}");
    assert!(
        system.ends_with(&text_edits::instructions(&schema)),
        "{system}"
    );
    let prompt = Rewrite.prompt(&asked(false, &schema, &wording), Value::Null);
    let instruction = prompt["instruction"].as_str().unwrap();
    assert!(instruction.starts_with("Give the complete corrected material. Change every defective passage, exercise, key or explanation identified above. An unchanged draft is not a repair. Preserve valid content. Do not mention this internal review, the course plan or the sources' excerpts in the learner-facing material."));
}
