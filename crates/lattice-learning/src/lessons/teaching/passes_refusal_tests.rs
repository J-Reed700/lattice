//! Edits that could not be placed are asked for once more before the finding is charged.
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
use super::*;
use crate::review_findings::{FindingSite, PlanCheck};
use serde_json::Value;

fn finding() -> ReviewFinding {
    ReviewFinding::plan(
        PlanCheck::Used,
        "alpha-idea",
        FindingSite::Section(0),
        "Alpha holds the line.",
    )
}

fn refused() -> Outcome {
    Outcome::NotApplied(vec!["edit 0 did not occur.".to_owned()])
}

fn spent(passes: &Passes, finding: &ReviewFinding) -> u8 {
    passes.spent.get(&finding.id()).copied().unwrap_or_default()
}

fn offered(passes: &mut Passes, finding: &ReviewFinding) -> bool {
    matches!(
        passes.next(&Value::Null, vec![finding.clone()]),
        Next::Repair(send) if send.iter().any(|sent| sent.id() == finding.id())
    )
}

#[test]
fn a_first_refusal_to_place_edits_is_not_charged_and_is_told() {
    let mut passes = Passes::default();
    let finding = finding();
    assert!(offered(&mut passes, &finding));
    passes.charge(&finding, refused());
    assert_eq!(spent(&passes, &finding), 0);
    assert!(offered(&mut passes, &finding));
    assert_eq!(
        passes.last_attempt(&finding.id()).as_deref(),
        Some(
            "The last repair's edits for this finding could not be applied: edit 0 did not occur."
        )
    );
    assert_eq!(passes.attempts.get(&finding.id()), Some(&vec![refused()]));
}

#[test]
fn a_second_refusal_in_a_row_is_charged() {
    let mut passes = Passes::default();
    let finding = finding();
    passes.charge(&finding, refused());
    passes.charge(&finding, refused());
    assert_eq!(spent(&passes, &finding), 1);
    assert!(offered(&mut passes, &finding));
}

#[test]
fn a_refusal_after_an_applied_attempt_is_a_first_refusal() {
    let mut passes = Passes::default();
    let finding = finding();
    passes.charge(&finding, refused());
    passes.charge(&finding, Outcome::Applied);
    passes.charge(&finding, refused());
    assert_eq!(spent(&passes, &finding), 1);
}

#[test]
fn two_charged_attempts_still_stop_a_finding() {
    let mut passes = Passes::default();
    let finding = finding();
    passes.charge(&finding, refused());
    passes.charge(&finding, refused());
    passes.charge(&finding, refused());
    assert_eq!(spent(&passes, &finding), 2);
    assert!(!offered(&mut passes, &finding));
}
