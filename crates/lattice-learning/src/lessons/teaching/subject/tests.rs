#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::lessons::teaching::standing::Standing;
use crate::review_findings::{FindingSite, Held};
use crate::teaching_review::{Asked, Judged, Reported};

/// `SHARED_CHECKS` as it was written out in full before it was built from
/// `block_checks!`.
const SHARED_CHECKS_BEFORE: &str = "For lessons, inspect EVERY teaching block, including unchanged text in a repaired draft. Each candidate block has an explicit zero-based index and bodyPassages whose text concatenates to its complete original body. Give blockChecks one entry for each section listed in sectionIndicesToReview, in the order listed, with that section's section-N ID as its sectionId. The application owns these section IDs; never renumber sections or put question checks in blockChecks. In every issue and finding, name a section or question by its label (the candidate's label field, as in 'Section 3') or its title, never by its section-N ID or its index. Put assessment findings in issues. For each section choose the passageId from that same block’s bodyPassages, give a concise factual finding that names every material defect the block has, so one repair can fix them all, and set hasDefect if the block contains any material defect. Read the WHOLE block, not only the selected passage. Never copy a quotation or invent a passage ID. Inspect stated conditions, exceptions, quantities, relationships, procedures and claimed results. Universal claims such as 'always', 'all', and 'never' need to hold for the stated scope. Compare explanations with exceptions described elsewhere in the lesson. For exercises check that the task can be attempted with preceding instruction and that its rubric evaluates the requested work. Written tutoring only reviews submitted text; it cannot run code/tests or provide external peer review or expert certification. Flag any promise otherwise, even if an earlier review missed it. Code execution requires a separate learner-started lab runtime. Report specific errors with the correct behavior; a generic 'looks correct' is not a factual finding. This is a model review, not execution or independent expert verification.";

fn lesson(count: usize) -> Value {
    let blocks: Vec<Value> = (0..count)
        .map(|n| json!({"kind":"explanation","title":"Part","body":format!("Part {n} states a fact."),"rubric":[],"sourceIndex":0,"quote":"q"}))
        .collect();
    json!({"blocks":blocks,"questions":[]})
}

/// The round after the reviewer saw `before` and reported a defect in its
/// first section.
fn recheck(before: &Value, after: &Value) -> Round {
    let mut standing = Standing::new(&Held::default());
    let first = standing.round(before).unwrap();
    let reported = Reported {
        site: Some(FindingSite::Section(0)),
        note: "A defect.".to_owned(),
        summary: "A defect.".to_owned(),
        passage: None,
        passage_id: None,
    };
    let judged = Judged {
        verdicts: Vec::new(),
        reported: vec![reported],
    };
    let fresh = standing.judged(before, &first, judged);
    standing.admit(before, fresh);
    standing.round(after).unwrap()
}

#[test]
fn the_shared_checks_are_the_text_they_were_before_they_were_built_from_a_macro() {
    assert_eq!(SHARED_CHECKS, SHARED_CHECKS_BEFORE);
}

#[test]
fn a_first_review_of_everything_reads_as_it_did_before_rounds() {
    let scope = "A scope.";
    let round = Standing::new(&Held::default()).round(&lesson(2)).unwrap();
    assert_eq!(
        review_instructions(REVIEW_SYSTEM, &round, true, scope),
        format!("{REVIEW_SYSTEM}\n{SHARED_CHECKS}\n{scope}")
    );
    let outline = Standing::new(&Held::default())
        .round(&json!({"modules":[]}))
        .unwrap();
    assert_eq!(
        review_instructions("First.", &outline, false, ""),
        "First.\n\n"
    );
}

#[test]
fn a_recheck_is_told_to_judge_the_correction_and_to_check_only_the_sections_that_changed() {
    let before = lesson(3);
    let mut after = before.clone();
    after["blocks"][0]["body"] = json!("Part 0 states another fact.");
    let round = recheck(&before, &after);
    let text = review_instructions(REVIEW_SYSTEM, &round, true, "A scope.");
    assert!(text.starts_with(RECHECK_SYSTEM));
    assert!(text.contains(CHANGED_CHECKS));
    assert!(!text.contains(SHARED_CHECKS));
    assert!(!text.contains(KNOWN_FINDINGS));
}

#[test]
fn a_recheck_of_every_section_holds_the_shared_checks() {
    let before = lesson(2);
    let mut after = before.clone();
    for n in 0..2 {
        after["blocks"][n]["body"] = json!(format!("Part {n} states another fact."));
    }
    let round = recheck(&before, &after);
    let text = review_instructions(REVIEW_SYSTEM, &round, true, "A scope.");
    assert!(text.contains(SHARED_CHECKS));
    assert!(!text.contains(CHANGED_CHECKS));
}

#[test]
fn known_findings_are_explained_only_when_there_are_some() {
    let before = lesson(3);
    let mut after = before.clone();
    after["blocks"][2]["body"] = json!("Part 2 states another fact.");
    let round = recheck(&before, &after);
    assert_eq!(round.known.len(), 1);
    let text = review_instructions(REVIEW_SYSTEM, &round, true, "A scope.");
    assert!(text.contains(KNOWN_FINDINGS));
    let alone = Round {
        known: Vec::<Asked>::new(),
        ..round
    };
    assert!(!review_instructions(REVIEW_SYSTEM, &alone, true, "A scope.").contains(KNOWN_FINDINGS));
}
