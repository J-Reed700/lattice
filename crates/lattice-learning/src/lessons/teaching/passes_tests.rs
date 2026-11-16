#![allow(clippy::panic, clippy::indexing_slicing)]
use super::*;
use crate::review_findings::{FindingKind, FindingSite, KeyDefect, PlanCheck, Rule};
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

/// `finding` as the review names it once `passes` has placed it in `candidate`.
fn placed(passes: &mut Passes, candidate: &Value, finding: ReviewFinding) -> ReviewFinding {
    let mut one = [finding];
    passes.place(candidate, &mut one);
    let [finding] = one;
    finding
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
        "ids": IDS_BY_UNIT,
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
    let first = placed(&mut passes, &candidate, own(0, "A first sentence."));
    sent(&mut passes, &candidate, vec![first.clone()]);
    passes.worked_on(&first, ["A second sentence.".to_owned()]);
    let second = placed(&mut passes, &candidate, own(0, "A second sentence."));
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
fn a_finding_about_words_a_refused_repair_was_charged_for_starts_at_that_charge() {
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
    passes.charge(&first, refused());
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
    let first = placed(&mut passes, &candidate, own(0, "A first sentence."));
    let filler = placed(&mut passes, &candidate, own(1, "A filler."));
    sent(&mut passes, &candidate, vec![first.clone(), filler]);
    sent(&mut passes, &candidate, vec![first.clone()]);
    passes.worked_on(&first, ["A second sentence.".to_owned()]);
    let second = placed(&mut passes, &candidate, own(2, "A second sentence."));
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
        told(&mut passes, refused()),
        "The last repair's correction for this finding was refused, and none of its edits were applied: A sentence must stay as it is."
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

/// A correction refused whole for changing a sentence that must stay.
fn refused() -> Outcome {
    Outcome::Refused(vec!["A sentence must stay as it is.".into()])
}

#[test]
fn an_outcome_saved_as_reverted_is_read_as_applied() {
    assert_eq!(
        serde_json::from_str::<Outcome>("\"Reverted\"").unwrap(),
        Outcome::Applied
    );
}

#[test]
fn what_became_of_the_last_attempt_survives_a_saved_review() {
    let mut passes = Passes::default();
    let finding = section(0, "A");
    passes.charge(&finding, refused());
    let saved = passes.saved().unwrap().to_string();
    let mut resumed = Passes::default();
    resumed.resume(&saved);
    assert_eq!(resumed.last.get(&finding.id()), Some(&refused()));
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
    let saved =
        serde_json::json!({"spent": [[id, 1]], "last": [], "worked": {}, "ids": IDS_BY_UNIT})
            .to_string();
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
    let first = placed(&mut passes, &candidate, own(0, "A first sentence."));
    sent(&mut passes, &candidate, vec![first.clone()]);
    passes.worked_on(&first, ["A second sentence.".to_owned()]);
    let second = placed(&mut passes, &candidate, own(0, "A second sentence."));
    sent(&mut passes, &candidate, vec![second.clone()]);
    assert_eq!(passes.account(&second).inherited, 1);
}

/// A lesson of questions named by their prompts, one after another.
fn questions(prompts: &[&str]) -> Value {
    let questions: Vec<Value> = prompts
        .iter()
        .map(|prompt| serde_json::json!({"prompt": prompt, "options": ["Left", "Right"]}))
        .collect();
    serde_json::json!({"blocks": [], "questions": questions})
}

const PARCELS: [&str; 6] = [
    "Which bin takes a parcel under one kilo?",
    "Which bin takes a parcel over ten kilos?",
    "Where does a parcel with a torn label wait?",
    "Who signs for a parcel left at the desk?",
    "What is done with a parcel marked fragile?",
    "When is a parcel sent back to its sender?",
];

#[test]
fn a_new_defect_on_a_question_whose_budget_an_older_defect_spent_is_repaired() {
    let mut passes = Passes::default();
    let candidate = questions(&PARCELS);
    let repeats =
        || ReviewFinding::answer_key(5, KeyDefect::Repeats, "It asks what another question asks.");
    sent(&mut passes, &candidate, vec![repeats()]);
    sent(&mut passes, &candidate, vec![repeats()]);
    let other = ReviewFinding::answer_key(
        5,
        KeyDefect::ReasonMissesWhy,
        "A reason does not say why its option is wrong.",
    );
    assert!(
        matches!(passes.next(&candidate, vec![other]), Next::Repair(_)),
        "a defect no repair has been asked about is not stopped by the budget another defect spent"
    );
}

#[test]
fn a_question_that_moves_up_after_a_cut_does_not_inherit_the_budget_of_the_one_cut() {
    let mut passes = Passes::default();
    let before = questions(&PARCELS);
    let cut_one =
        || ReviewFinding::answer_key(4, KeyDefect::Untrue, "Its explanation contradicts the key.");
    sent(&mut passes, &before, vec![cut_one()]);
    sent(&mut passes, &before, vec![cut_one()]);
    let after: Vec<&str> = PARCELS
        .iter()
        .copied()
        .enumerate()
        .filter(|(index, _)| *index != 4)
        .map(|(_, prompt)| prompt)
        .collect();
    let moved =
        ReviewFinding::answer_key(4, KeyDefect::Untrue, "Its explanation contradicts the key.");
    assert!(
        matches!(
            passes.next(&questions(&after), vec![moved]),
            Next::Repair(_)
        ),
        "the last question has been repaired never, whatever stood at its new position"
    );
}

#[test]
fn state_saved_under_position_ids_keeps_what_the_words_were_put_through_and_nothing_else() {
    let mut passes = Passes::default();
    let first = own(0, "A first sentence.");
    let candidate = serde_json::json!({"blocks":[{"body":"A first sentence."}],"questions":[]});
    sent(&mut passes, &candidate, vec![first.clone()]);
    passes.worked_on(&first, ["A first sentence.".to_owned()]);
    let mut saved: Value = serde_json::from_str(&passes.saved().unwrap().to_string()).unwrap();
    saved.as_object_mut().unwrap().remove("ids");
    let mut resumed = Passes::default();
    resumed.resume(&saved.to_string());
    assert!(resumed.spent.is_empty());
    assert!(resumed.last.is_empty());
    assert!(resumed.attempts.is_empty());
    assert!(resumed.passage_less.is_empty());
    assert_eq!(resumed.worked, passes.worked);
}

#[test]
fn the_lineage_a_review_kept_is_resumed_and_joined_with_what_is_held() {
    let candidate = questions(&PARCELS);
    let mut passes = Passes::default();
    let mut findings = vec![ReviewFinding::answer_key(2, KeyDefect::Untrue, "x")];
    passes.place(&candidate, &mut findings);
    let unit = findings[0].unit.clone().unwrap();
    let saved = passes.saved().unwrap().to_string();
    let mut resumed = Passes::default();
    resumed.resume(&saved);
    let mut again = vec![ReviewFinding::answer_key(2, KeyDefect::Untrue, "x")];
    resumed.place(&candidate, &mut again);
    assert_eq!(again[0].unit, Some(unit));
}

/// A lesson of sections named by their bodies, one after another.
fn sections(bodies: &[&str]) -> Value {
    let blocks: Vec<Value> = bodies
        .iter()
        .map(|body| serde_json::json!({"title": "Sorting", "body": body}))
        .collect();
    serde_json::json!({"blocks": blocks, "questions": []})
}

fn title_repeated(section: usize) -> ReviewFinding {
    ReviewFinding::rule(
        Rule::TitleRepeated,
        FindingKind::Structure,
        FindingSite::Section(section),
        "The title repeats an earlier one.",
    )
}

fn block_body(section: usize) -> ReviewFinding {
    ReviewFinding::rule(
        Rule::BlockBody,
        FindingKind::Structure,
        FindingSite::Section(section),
        "The section has no body.",
    )
}

#[test]
fn a_section_a_rewrite_moves_up_and_changes_is_a_new_part() {
    let mut passes = Passes::default();
    let before = sections(&[
        "A parcel is sorted.",
        "A parcel is weighed.",
        "A parcel is shipped.",
    ]);
    let mut earlier = vec![title_repeated(0), title_repeated(1), title_repeated(2)];
    passes.place(&before, &mut earlier);
    let after = sections(&["A parcel is sorted.", "A parcel is shipped by road."]);
    passes.carry_rewrite(&before, &after);
    assert_eq!(
        placed(&mut passes, &after, title_repeated(0)).id(),
        earlier[0].id()
    );
    let changed = placed(&mut passes, &after, title_repeated(1)).id();
    assert!(
        earlier.iter().all(|finding| finding.id() != changed),
        "a changed section takes the id of no part it did not come from: {changed:?}"
    );
}

#[test]
fn a_rewrite_that_drops_a_section_and_rewords_the_next_does_not_hand_it_the_dropped_one_s_repairs()
{
    let mut passes = Passes::default();
    let before = sections(&[
        "A parcel is sorted.",
        "A parcel is weighed.",
        "A parcel is shipped.",
    ]);
    sent(&mut passes, &before, vec![title_repeated(1)]);
    sent(&mut passes, &before, vec![title_repeated(1)]);
    let after = sections(&["A parcel is sorted.", "A parcel is shipped by road."]);
    passes.carry_rewrite(&before, &after);
    assert!(
        matches!(
            passes.next(&after, vec![title_repeated(1)]),
            Next::Repair(_)
        ),
        "the reworded section has never been repaired"
    );
}

/// A lesson whose second section has no body, under a title that names `round`.
fn bodiless(round: usize) -> Value {
    serde_json::json!({"blocks": [
        {"title": "Sorting", "body": "A parcel is sorted."},
        {"title": format!("Weighing {round}"), "body": ""}
    ], "questions": []})
}

#[test]
fn a_finding_only_a_rewrite_repairs_stops_after_its_repairs_though_each_rewrite_gives_its_section_new_text(
) {
    let mut passes = Passes::default();
    let mut current = bodiless(0);
    for round in 1..=usize::from(REPAIRS_PER_FINDING) {
        sent(&mut passes, &current, vec![block_body(1)]);
        let rewritten = bodiless(round);
        passes.carry_rewrite(&current, &rewritten);
        current = rewritten;
    }
    let left = done(passes.next(&current, vec![block_body(1)]));
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].1, REPAIRS_PER_FINDING);
}

#[test]
fn rewrites_that_swap_a_section_between_two_texts_end() {
    let mut passes = Passes::default();
    let mut current = bodiless(0);
    let mut rounds = 0;
    while let Next::Repair(send) = passes.next(&current, vec![block_body(1)]) {
        for finding in &send {
            passes.charge(finding, Outcome::Applied);
        }
        rounds += 1;
        assert!(
            rounds <= 2 * usize::from(REPAIRS_PER_FINDING),
            "each of the two texts is repaired at most its own repairs"
        );
        let rewritten = bodiless(rounds % 2);
        passes.carry_rewrite(&current, &rewritten);
        current = rewritten;
    }
}

#[test]
fn a_finding_only_a_rewrite_repairs_about_a_part_no_rewrite_wrote_has_repairs_of_its_own() {
    let mut passes = Passes::default();
    let mut current = bodiless(0);
    for round in 1..=usize::from(REPAIRS_PER_FINDING) {
        sent(&mut passes, &current, vec![block_body(1)]);
        let rewritten = bodiless(round);
        passes.carry_rewrite(&current, &rewritten);
        current = rewritten;
    }
    assert_eq!(sent(&mut passes, &current, vec![block_body(0)]).len(), 1);
}

#[test]
fn the_parts_rewrites_wrote_survive_a_saved_review() {
    let mut passes = Passes::default();
    let mut current = bodiless(0);
    for round in 1..=usize::from(REPAIRS_PER_FINDING) {
        sent(&mut passes, &current, vec![block_body(1)]);
        let rewritten = bodiless(round);
        passes.carry_rewrite(&current, &rewritten);
        current = rewritten;
    }
    let mut resumed = Passes::default();
    resumed.resume(&passes.saved().unwrap().to_string());
    assert_eq!(done(resumed.next(&current, vec![block_body(1)])).len(), 1);
}
