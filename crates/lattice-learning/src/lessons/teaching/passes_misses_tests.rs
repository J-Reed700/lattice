//! An attempt that changed nothing for a finding is charged only after another such attempt.
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
use super::*;
use crate::review_findings::{FindingSite, PlanCheck};
use serde_json::Value;

fn finding() -> ReviewFinding {
    ReviewFinding::plan(
        PlanCheck::Used,
        "beta-idea",
        FindingSite::Section(0),
        "Beta holds the line.",
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
fn a_finding_no_edit_named_is_not_charged_and_is_told() {
    let mut passes = Passes::default();
    let finding = finding();
    passes.charge(&finding, Outcome::Unnamed);
    assert_eq!(spent(&passes, &finding), 0);
    assert!(offered(&mut passes, &finding));
    assert_eq!(
        passes.last_attempt(&finding.id()).as_deref(),
        Some("The last repair gave no edit for this finding.")
    );
}

fn declined() -> Outcome {
    Outcome::Declined("Beads are already right as written.".to_owned())
}

#[test]
fn a_finding_the_repair_gave_no_change_for_is_charged_like_an_unnamed_one() {
    let mut passes = Passes::default();
    let finding = finding();
    passes.charge(&finding, declined());
    assert_eq!(spent(&passes, &finding), 0);
    assert!(offered(&mut passes, &finding));
    passes.charge(&finding, declined());
    assert_eq!(spent(&passes, &finding), 1);
    assert!(offered(&mut passes, &finding));
    passes.charge(&finding, declined());
    assert_eq!(spent(&passes, &finding), 2);
    assert!(!offered(&mut passes, &finding));
}

#[test]
fn a_declined_attempt_and_an_unnamed_one_in_a_row_cost_one_repair() {
    let mut passes = Passes::default();
    let finding = finding();
    passes.charge(&finding, Outcome::Unnamed);
    passes.charge(&finding, declined());
    assert_eq!(spent(&passes, &finding), 1);
    let mut passes = Passes::default();
    passes.charge(&finding, declined());
    passes.charge(&finding, Outcome::Unnamed);
    assert_eq!(spent(&passes, &finding), 1);
}

#[test]
fn a_declined_attempt_is_told_and_logged_with_the_plan_the_repair_gave() {
    let mut passes = Passes::default();
    let finding = finding();
    passes.charge(&finding, declined());
    assert_eq!(
        passes.last_attempt(&finding.id()).as_deref(),
        Some("The last repair gave no change for this finding; its plan was: \"Beads are already right as written.\"")
    );
    assert_eq!(
        passes.account(&finding).attempts,
        "the repair gave no change for it; its plan was: \"Beads are already right as written.\""
    );
}

#[test]
fn a_declined_attempt_survives_a_saved_review() {
    let mut passes = Passes::default();
    let finding = finding();
    passes.charge(&finding, declined());
    let saved = passes.saved().unwrap().to_string();
    let mut resumed = Passes::default();
    resumed.resume(&saved);
    assert_eq!(resumed.last.get(&finding.id()), Some(&declined()));
}

#[test]
fn a_missed_attempt_before_two_applied_ones_does_not_cost_one() {
    let mut passes = Passes::default();
    let finding = finding();
    passes.charge(&finding, Outcome::Unnamed);
    passes.charge(&finding, Outcome::Applied);
    assert!(offered(&mut passes, &finding));
    passes.charge(&finding, Outcome::Applied);
    assert_eq!(spent(&passes, &finding), 2);
    assert!(!offered(&mut passes, &finding));
}

#[test]
fn a_second_miss_in_a_row_is_charged() {
    let mut passes = Passes::default();
    let finding = finding();
    passes.charge(&finding, Outcome::Unnamed);
    passes.charge(&finding, Outcome::Unnamed);
    assert_eq!(spent(&passes, &finding), 1);
    assert!(offered(&mut passes, &finding));
    passes.charge(&finding, Outcome::Unnamed);
    assert_eq!(spent(&passes, &finding), 2);
    assert!(!offered(&mut passes, &finding));
}

#[test]
fn an_unplaced_attempt_after_an_unnamed_one_is_charged() {
    let mut passes = Passes::default();
    let finding = finding();
    passes.charge(&finding, Outcome::Unnamed);
    assert_eq!(spent(&passes, &finding), 0);
    passes.charge(&finding, refused());
    assert_eq!(spent(&passes, &finding), 1);
}

#[test]
fn a_miss_between_applied_attempts_is_free() {
    let mut passes = Passes::default();
    let finding = finding();
    passes.charge(&finding, Outcome::Applied);
    passes.charge(&finding, Outcome::Unnamed);
    assert_eq!(spent(&passes, &finding), 1);
    assert!(offered(&mut passes, &finding));
    passes.charge(&finding, Outcome::Applied);
    assert_eq!(spent(&passes, &finding), 2);
    assert!(!offered(&mut passes, &finding));
}

#[test]
fn an_unusable_answer_is_charged_every_time() {
    let mut passes = Passes::default();
    let finding = finding();
    passes.charge(&finding, Outcome::Unusable("it was empty.".into()));
    assert_eq!(spent(&passes, &finding), 1);
    passes.charge(&finding, Outcome::Unusable("it was empty.".into()));
    assert_eq!(spent(&passes, &finding), 2);
    assert!(!offered(&mut passes, &finding));
}

#[test]
fn a_finding_given_only_misses_is_sent_at_most_twice_the_repairs_it_is_given() {
    let mut passes = Passes::default();
    let finding = finding();
    let mut sent = 0;
    while offered(&mut passes, &finding) {
        assert!(
            sent < 2 * usize::from(REPAIRS_PER_FINDING),
            "it never stopped"
        );
        passes.charge(&finding, Outcome::Unnamed);
        sent += 1;
    }
    assert!(sent > usize::from(REPAIRS_PER_FINDING));
    assert!(!offered(&mut passes, &finding));
}
