#![allow(clippy::panic, clippy::indexing_slicing)]
use super::*;
use crate::review_findings::{FindingSite, PlanCheck};
use std::collections::BTreeSet;

fn section(index: usize, note: &str) -> ReviewFinding {
    ReviewFinding::reviewer(
        u32::try_from(index).unwrap(),
        Some(FindingSite::Section(index)),
        note,
    )
}

/// A plan finding at section 0: every call is the same finding.
fn plan(note: &str) -> ReviewFinding {
    ReviewFinding::plan(PlanCheck::Used, "an idea", FindingSite::Section(0), note)
}

/// A finding in section `index` about `words`.
fn own(index: usize, words: &str) -> ReviewFinding {
    ReviewFinding::plan(
        PlanCheck::Used,
        subject_of(words),
        FindingSite::Section(index),
        words,
    )
    .with_passage(words)
}

/// The ids the next repair is sent, each charged as a repair that applied.
fn sent(passes: &mut Passes, candidate: &Value, findings: Vec<ReviewFinding>) -> Vec<FindingId> {
    match passes.next(candidate, findings) {
        Next::Repair(send) => {
            for finding in &send {
                passes.charge(finding, Outcome::Applied);
            }
            send.iter().map(ReviewFinding::id).collect()
        }
        Next::Done(_) => panic!("expected a repair"),
    }
}

fn done(next: Next) -> Vec<(FindingId, u8)> {
    match next {
        Next::Done(set_aside) => set_aside
            .iter()
            .map(|s| (s.finding.id(), s.repairs))
            .collect(),
        Next::Repair(_) => panic!("expected the review to stop"),
    }
}

#[test]
fn every_open_finding_goes_to_one_repair() {
    let mut passes = Passes::default();
    let findings: Vec<_> = (0..9).map(|i| section(i, "x")).collect();
    assert_eq!(sent(&mut passes, &Value::Null, findings).len(), 9);
}

#[test]
fn state_saved_with_fields_no_longer_kept_still_resumes() {
    let id = serde_json::to_value(section(3, "").id()).unwrap();
    let saved = serde_json::json!({
        "spent": [[id, 1]],
        "last": [[id, "Applied"]],
        "worked": {},
        "open_at_repair": [id],
        "sent_at_repair": [],
        "stalled": [],
    })
    .to_string();
    let mut passes = Passes::default();
    passes.resume(&saved);
    assert_eq!(passes.spent.get(&section(3, "").id()), Some(&1));
}

#[test]
fn a_clean_first_pass_publishes_with_nothing_set_aside() {
    assert!(done(Passes::default().next(&Value::Null, vec![])).is_empty());
}

#[test]
fn a_pass_that_fixes_something_continues_even_with_its_wording_rewritten() {
    let mut passes = Passes::default();
    assert_eq!(
        sent(
            &mut passes,
            &Value::Null,
            vec![section(0, "A"), section(1, "B")]
        )
        .len(),
        2
    );
    // Section 1 is fixed; section 0 survives one repair, reworded.
    let again = sent(
        &mut passes,
        &Value::Null,
        vec![section(0, "A, said differently")],
    );
    assert_eq!(again, vec![section(0, "").id()]);
    // Fixed at the second repair: the lesson publishes clean.
    assert!(done(passes.next(&Value::Null, vec![])).is_empty());
}

#[test]
fn a_finding_that_survives_two_repairs_is_set_aside_while_others_continue() {
    let mut passes = Passes::default();
    sent(
        &mut passes,
        &Value::Null,
        vec![section(0, "A"), section(1, "B"), section(2, "C")],
    );
    sent(
        &mut passes,
        &Value::Null,
        vec![section(0, "A"), section(1, "B")],
    );
    // B was fixed, so the review goes on; A survived two repairs and
    // stops; D is new and is repaired.
    let third = sent(
        &mut passes,
        &Value::Null,
        vec![section(0, "A"), section(3, "D")],
    );
    assert_eq!(third, vec![section(3, "").id()]);
    // D fixed; A is the only finding left and it stays set aside.
    assert_eq!(
        done(passes.next(&Value::Null, vec![section(0, "A")])),
        vec![(section(0, "").id(), 2)]
    );
}

#[test]
fn state_saved_before_finding_ids_resumes_with_fresh_budgets() {
    let key = section(3, "").place();
    let old = serde_json::to_string(&serde_json::json!({
        "repairs": [(key, 1)],
        "open_at_repair": [key],
    }))
    .unwrap();
    let mut passes = Passes::default();
    passes.resume(&old);
    assert!(passes.spent.is_empty());
    assert!(passes.worked.is_empty());
    // Nothing was kept, so the finding gets a repair as a new one.
    assert_eq!(
        sent(&mut passes, &Value::Null, vec![section(3, "x")]),
        vec![section(3, "").id()]
    );
}

#[test]
fn state_saved_mid_review_keeps_what_stopped() {
    let mut passes = Passes::default();
    sent(&mut passes, &Value::Null, vec![section(0, "A")]);
    sent(
        &mut passes,
        &Value::Null,
        vec![section(0, "A"), section(1, "B")],
    );
    let saved = passes.saved().unwrap().to_string();
    let mut resumed = Passes::default();
    resumed.resume(&saved);
    assert!(!resumed.repairable(&section(0, "")));
}

#[test]
fn a_finding_the_user_already_holds_is_never_repaired() {
    let held = section(4, "kept as written");
    let mut passes = Passes::new(Held {
        places: BTreeSet::from([held.place()]),
        ..Held::default()
    });
    assert!(!passes.repairable(&held));
    assert_eq!(
        done(passes.next(&Value::Null, vec![section(4, "still there")])),
        vec![(held.id(), 0)]
    );
}

#[test]
fn a_finding_held_during_the_review_is_never_repaired() {
    let held = section(4, "a protected sentence");
    let mut passes = Passes::default();
    passes.hold([held.id()], "a test");
    assert!(!passes.repairable(&held));
    assert_eq!(
        done(passes.next(&Value::Null, vec![held.clone()])),
        vec![(held.id(), 0)]
    );
}

#[test]
fn two_notes_with_one_id_are_one_finding() {
    let mut passes = Passes::default();
    let Next::Repair(send) = passes.next(&Value::Null, vec![plan("First."), plan("Second.")])
    else {
        panic!("expected a repair");
    };
    assert_eq!(send.len(), 1);
    assert_eq!(send[0].note, "First. Second.");
}

#[test]
fn two_findings_with_their_own_ids_at_one_place_stay_two() {
    let mut passes = Passes::default();
    let both = sent(
        &mut passes,
        &Value::Null,
        vec![own(0, "A first sentence."), own(0, "A second sentence.")],
    );
    assert_eq!(both.len(), 2);
}

#[test]
fn a_new_finding_about_words_a_repair_worked_on_starts_at_that_repair_s_count() {
    let candidate = serde_json::json!({"blocks":[{"body":"A first sentence. A second sentence."}],"questions":[]});
    let mut passes = Passes::default();
    let first = own(0, "A first sentence.");
    sent(&mut passes, &candidate, vec![first.clone()]);
    passes.worked_on(&first, ["A second sentence.".to_owned()]);
    let second = own(0, "A second sentence.");
    assert_eq!(
        sent(&mut passes, &candidate, vec![second.clone()]),
        vec![second.id()]
    );
    assert_eq!(
        passes.spent.get(&second.id()),
        Some(&2),
        "it started at the repair's count of 1 and was sent once more"
    );
}

#[test]
fn a_finding_about_words_a_reverted_repair_was_charged_for_starts_at_that_charge() {
    let candidate = serde_json::json!({"blocks":[{"body":"A first sentence. A second sentence."}],"questions":[]});
    let about = |number| {
        ReviewFinding::reviewer(number, Some(FindingSite::Section(0)), "It is wrong.")
            .with_passage("A first sentence.")
    };
    let mut passes = Passes::default();
    let first = about(1);
    assert!(matches!(
        passes.next(&candidate, vec![first.clone()]),
        Next::Repair(_)
    ));
    passes.charge(&first, Outcome::Reverted);
    let again = about(2);
    assert!(matches!(
        passes.next(&candidate, vec![again.clone()]),
        Next::Repair(_)
    ));
    assert_eq!(
        passes.spent.get(&again.id()),
        Some(&1),
        "the defect was charged one repair before this finding named it"
    );
}

#[test]
fn a_reviewer_finding_about_words_a_stored_finding_is_about_is_not_repaired() {
    let candidate = serde_json::json!({"blocks":[{"body":"A first sentence. A second sentence."}],"questions":[]});
    let about = |number, words| {
        ReviewFinding::reviewer(number, Some(FindingSite::Section(0)), "It is wrong.")
            .with_passage(words)
    };
    let held = Held {
        reviewer: vec![(about(1, "A first sentence."), String::new())],
        next_number: 2,
        ..Held::default()
    };
    let mut passes = Passes::new(held);
    let restated = about(2, "A first sentence.");
    assert!(matches!(
        passes.next(&candidate, vec![restated.clone()]),
        Next::Done(_)
    ));
    let other = about(3, "A second sentence.");
    assert!(matches!(
        passes.next(&candidate, vec![other]),
        Next::Repair(_)
    ));
}

#[test]
fn a_new_finding_about_words_that_went_through_the_limit_is_not_repaired() {
    let candidate = serde_json::json!({"blocks":[
        {"body":"A first sentence."},
        {"body":"A filler."},
        {"body":"A second sentence."}
    ],"questions":[]});
    let mut passes = Passes::default();
    let first = own(0, "A first sentence.");
    sent(
        &mut passes,
        &candidate,
        vec![first.clone(), own(1, "A filler.")],
    );
    sent(&mut passes, &candidate, vec![first.clone()]);
    passes.worked_on(&first, ["A second sentence.".to_owned()]);
    let second = own(2, "A second sentence.");
    assert_eq!(
        done(passes.next(&candidate, vec![second.clone()])),
        vec![(second.id(), 2)]
    );
}

#[test]
fn the_summaries_of_two_notes_with_one_id_stay_together() {
    let mut passes = Passes::default();
    let Next::Repair(send) = passes.next(
        &Value::Null,
        vec![
            plan("First.").with_summary("One problem."),
            plan("Second.").with_summary("Another problem."),
            plan("Third.").with_summary("One problem."),
            plan("Fourth."),
        ],
    ) else {
        panic!("expected a repair");
    };
    assert_eq!(send.len(), 1);
    assert_eq!(send[0].summary, "One problem.\nAnother problem.");
}

/// A note about one sentence lets the user take that sentence out, so a
/// note that now stands for more than one has no sentence.
#[test]
fn a_merged_note_keeps_its_sentence_only_while_every_note_in_it_is_about_that_sentence() {
    let about = |sentence: &str| plan("A note.").with_sentence(sentence);
    let merged = |findings: Vec<ReviewFinding>| {
        let mut passes = Passes::default();
        let Next::Repair(send) = passes.next(&Value::Null, findings) else {
            panic!("expected a repair");
        };
        assert_eq!(send.len(), 1);
        send[0].sentence.clone()
    };
    assert_eq!(
        merged(vec![about("One."), about("One.")]).as_deref(),
        Some("One.")
    );
    assert_eq!(merged(vec![about("One."), about("Two.")]), None);
    assert_eq!(merged(vec![about("One."), plan("Other.")]), None);
    assert_eq!(merged(vec![plan("Other."), about("One.")]), None);
}

#[test]
fn a_finding_that_was_sent_is_not_charged_until_the_repair_is_settled() {
    let mut passes = Passes::default();
    let Next::Repair(send) = passes.next(&Value::Null, vec![section(0, "A")]) else {
        panic!("expected a repair");
    };
    assert!(passes.spent.is_empty());
    passes.charge(&send[0], Outcome::Applied);
    assert_eq!(passes.spent.get(&send[0].id()), Some(&1));
    assert_eq!(passes.last.get(&send[0].id()), Some(&Outcome::Applied));
}

#[test]
fn a_second_attempt_is_told_what_became_of_the_first() {
    let mut passes = Passes::default();
    let finding = section(0, "A");
    let id = finding.id();
    assert_eq!(passes.last_attempt(&id), None);
    let told = |passes: &mut Passes, outcome: Outcome| {
        passes.charge(&finding, outcome);
        passes.last_attempt(&id).unwrap()
    };
    assert_eq!(
        told(&mut passes, Outcome::Applied),
        "The last repair's edits for this finding were applied and the defect is still there."
    );
    assert_eq!(
        told(
            &mut passes,
            Outcome::NotApplied(vec!["edit 0 first.".into(), "edit 1 second.".into()])
        ),
        "The last repair's edits for this finding could not be applied: edit 0 first. edit 1 second."
    );
    assert_eq!(
        told(&mut passes, Outcome::Reverted),
        "The last repair's edits for this finding changed a sentence that must stay as it is; they were put back."
    );
    assert_eq!(
        told(&mut passes, Outcome::Unnamed),
        "The last repair gave no edit for this finding."
    );
    assert_eq!(
        told(&mut passes, Outcome::Unusable("it was empty.".into())),
        "The last repair's answer could not be used: it was empty."
    );
}

#[test]
fn what_became_of_the_last_attempt_survives_a_saved_review() {
    let mut passes = Passes::default();
    let finding = section(0, "A");
    passes.charge(&finding, Outcome::Reverted);
    let saved = passes.saved().unwrap().to_string();
    let mut resumed = Passes::default();
    resumed.resume(&saved);
    assert_eq!(resumed.last.get(&finding.id()), Some(&Outcome::Reverted));
    assert_eq!(resumed.spent.get(&finding.id()), Some(&1));
}

/// A reviewer finding numbered `number`, at section 0, with no passage.
fn without_passage(number: u32) -> ReviewFinding {
    ReviewFinding::reviewer(number, Some(FindingSite::Section(0)), "A defect.")
}

#[test]
fn a_new_reviewer_finding_with_no_passage_starts_at_the_count_of_the_one_before() {
    let mut passes = Passes::default();
    sent(&mut passes, &Value::Null, vec![without_passage(1)]);
    let second = without_passage(2);
    assert_eq!(
        sent(&mut passes, &Value::Null, vec![second.clone()]),
        vec![second.id()]
    );
    assert_eq!(passes.spent.get(&second.id()), Some(&2));
    assert!(!passes.repairable(&second));
}

#[test]
fn a_new_reviewer_finding_with_a_passage_does_not_start_at_the_count_of_the_one_before() {
    let mut passes = Passes::default();
    sent(&mut passes, &Value::Null, vec![without_passage(1)]);
    let second = without_passage(2).with_passage("Some words.");
    sent(&mut passes, &Value::Null, vec![second.clone()]);
    assert_eq!(passes.spent.get(&second.id()), Some(&1));
    assert!(passes.repairable(&second));
}

#[test]
fn a_plan_finding_does_not_start_at_the_count_of_a_reviewer_finding() {
    let mut passes = Passes::default();
    sent(&mut passes, &Value::Null, vec![without_passage(1)]);
    let other = plan("Another defect.");
    sent(&mut passes, &Value::Null, vec![other.clone()]);
    assert_eq!(passes.spent.get(&other.id()), Some(&1));
}

#[test]
fn the_count_of_findings_with_no_passage_survives_a_saved_review() {
    let mut passes = Passes::default();
    sent(&mut passes, &Value::Null, vec![without_passage(1)]);
    let saved = passes.saved().unwrap().to_string();
    let mut resumed = Passes::default();
    resumed.resume(&saved);
    let second = without_passage(2);
    sent(&mut resumed, &Value::Null, vec![second.clone()]);
    assert_eq!(resumed.spent.get(&second.id()), Some(&2));
}

#[test]
fn state_saved_before_findings_with_no_passage_were_counted_still_resumes() {
    let id = serde_json::to_value(section(3, "").id()).unwrap();
    let saved = serde_json::json!({"spent": [[id, 1]], "last": [], "worked": {}}).to_string();
    let mut passes = Passes::default();
    passes.resume(&saved);
    assert_eq!(passes.spent.get(&section(3, "").id()), Some(&1));
    assert!(passes.passage_less.is_empty());
}

#[test]
fn an_attempt_left_out_for_overlap_is_no_repair_and_is_told_to_try_again() {
    let mut passes = Passes::default();
    let finding = own(0, "A first sentence.");
    for _ in 0..3 {
        passes.charge(&finding, Outcome::Deferred);
    }
    assert_eq!(passes.spent.get(&finding.id()), None);
    assert!(passes.worked.is_empty());
    assert!(passes.repairable(&finding));
    assert_eq!(
        passes.last_attempt(&finding.id()).as_deref(),
        Some("The last repair's edit for this finding overlapped an edit for another finding and was left out; give it again.")
    );
    passes.charge(&finding, Outcome::Applied);
    assert_eq!(passes.spent.get(&finding.id()), Some(&1));
}

#[test]
fn a_deferred_attempt_survives_a_saved_review() {
    let mut passes = Passes::default();
    let finding = own(0, "A first sentence.");
    passes.charge(&finding, Outcome::Deferred);
    let saved = passes.saved().unwrap().to_string();
    let mut resumed = Passes::default();
    resumed.resume(&saved);
    assert_eq!(resumed.last.get(&finding.id()), Some(&Outcome::Deferred));
    assert_eq!(
        resumed.attempts.get(&finding.id()),
        Some(&vec![Outcome::Deferred])
    );
}

#[test]
fn the_account_of_a_finding_lists_its_attempts_in_order_and_why_it_was_held() {
    let mut passes = Passes::default();
    let finding = own(0, "A first sentence.");
    passes.charge(
        &finding,
        Outcome::NotApplied(vec!["It did not occur.".to_owned()]),
    );
    passes.charge(&finding, Outcome::Deferred);
    passes.hold([finding.id()], "a sentence a live claim stands on");
    let account = passes.account(&finding);
    assert_eq!(account.inherited, 0);
    assert_eq!(account.held, "a sentence a live claim stands on");
    assert_eq!(
        account.attempts,
        "its edits could not be applied: It did not occur. | its edit overlapped another finding's and was left out"
    );
    let user_held = section(4, "kept as written");
    let with_user = Passes::new(Held {
        places: BTreeSet::from([user_held.place()]),
        ..Held::default()
    });
    assert_eq!(with_user.account(&user_held).held, "already with the user");
}

#[test]
fn a_finding_that_starts_above_zero_says_so_in_its_account() {
    let candidate = serde_json::json!({"blocks":[{"body":"A first sentence. A second sentence."}],"questions":[]});
    let mut passes = Passes::default();
    let first = own(0, "A first sentence.");
    sent(&mut passes, &candidate, vec![first.clone()]);
    passes.worked_on(&first, ["A second sentence.".to_owned()]);
    let second = own(0, "A second sentence.");
    sent(&mut passes, &candidate, vec![second.clone()]);
    assert_eq!(passes.account(&second).inherited, 1);
}
