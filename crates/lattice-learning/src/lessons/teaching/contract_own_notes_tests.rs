//! The check, the confirming read and the writer work from one definition of what a record needs taught.
use super::*;

const OWN_NOTES: &str = "Writing in the learner's own notes, on paper or wherever they already keep notes, needs nothing taught. A record the lesson asks for anywhere else needs the way to make it taught.";

#[test]
fn the_check_and_the_confirming_read_share_the_own_notes_sentence() {
    assert!(FIND_SYSTEM.contains(OWN_NOTES), "{FIND_SYSTEM}");
    assert!(
        confirm::USE_SYSTEM.contains(OWN_NOTES),
        "{}",
        confirm::USE_SYSTEM
    );
}

#[test]
fn the_check_excepts_the_learners_own_notes_from_a_recorded_use() {
    assert!(
        !FIND_SYSTEM.contains("records or saves the work)"),
        "{FIND_SYSTEM}"
    );
    assert!(
        !FIND_BRIEF.contains("records or saves the work,"),
        "{FIND_BRIEF}"
    );
    assert!(FIND_SYSTEM.contains("records or saves the work anywhere but their own notes)"));
    assert!(FIND_BRIEF.contains("records or saves the work anywhere but their own notes,"));
}

#[test]
fn a_result_saved_under_the_earlier_policy_is_not_the_current_one() {
    assert_ne!(POLICY.as_str(), "lesson-contract-v5");
    assert_eq!(POLICY.as_str(), "lesson-contract-v6");
}
