//! The check, the confirming read and the writer work from one definition of what a record needs taught.
use super::*;

const OWN_NOTES: &str = "Writing in the learner's own notes, on paper or wherever they already keep notes, needs nothing taught. A record the lesson asks for anywhere else needs the way to make it taught.";

#[test]
fn the_check_and_the_confirming_read_share_the_own_notes_sentence() {
    assert!(SYSTEM.contains(OWN_NOTES), "{SYSTEM}");
    assert!(confirm::SYSTEM.contains(OWN_NOTES), "{}", confirm::SYSTEM);
}

#[test]
fn the_check_excepts_the_learners_own_notes_from_a_recorded_use() {
    assert!(!SYSTEM.contains("records or saves the work)"), "{SYSTEM}");
    assert!(!BRIEF.contains("records or saves the work,"), "{BRIEF}");
    assert!(SYSTEM.contains("records or saves the work anywhere but their own notes)"));
    assert!(BRIEF.contains("records or saves the work anywhere but their own notes,"));
}

#[test]
fn a_result_saved_under_the_earlier_policy_is_not_the_current_one() {
    assert_ne!(POLICY, "lesson-contract-v5");
    assert_eq!(POLICY, "lesson-contract-v6");
}
