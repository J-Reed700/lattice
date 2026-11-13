//! A finding's repairs are counted per kind of defect, and carried along the words a repair wrote.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::review_findings::{FindingSite, PlanCheck};
use serde_json::json;

fn candidate_of(sentences: &[String]) -> Value {
    json!({"blocks":[{"body": sentences.join(" ")}],"questions":[]})
}

fn sentence(number: usize) -> String {
    format!("Sentence number {number} stands here.")
}

/// A plan finding of kind `check` about `words`, in section 0.
fn about(check: PlanCheck, words: &str) -> ReviewFinding {
    ReviewFinding::plan(check, subject_of(words), FindingSite::Section(0), words)
        .with_passage(words)
}

/// Whether `finding` was sent to a repair, which is charged as applied.
fn repaired(passes: &mut Passes, candidate: &Value, finding: &ReviewFinding) -> bool {
    match passes.next(candidate, vec![finding.clone()]) {
        Next::Repair(send) => {
            for sent in &send {
                passes.charge(sent, Outcome::Applied);
            }
            true
        }
        Next::Done(_) => false,
    }
}

#[test]
fn a_plan_finding_about_words_a_reviewer_finding_was_charged_for_is_sent_with_its_full_count() {
    let words = sentence(0);
    let candidate = candidate_of(&[words.clone(), sentence(1)]);
    let reviewer = ReviewFinding::reviewer(1, Some(FindingSite::Section(0)), "It is wrong.")
        .with_passage(words.as_str());
    let mut passes = Passes::default();
    for _ in 0..REPAIRS_PER_FINDING {
        assert!(repaired(&mut passes, &candidate, &reviewer));
    }
    assert!(!passes.repairable(&reviewer));
    let used = about(PlanCheck::Used, &words);
    assert!(repaired(&mut passes, &candidate, &used));
    assert_eq!(passes.spent.get(&used.id()), Some(&1));
}

#[test]
fn a_repair_carries_the_count_of_every_kind_to_the_words_it_writes() {
    let candidate = candidate_of(&[sentence(0), sentence(1)]);
    let mut passes = Passes::default();
    let used = about(PlanCheck::Used, &sentence(0));
    assert!(repaired(&mut passes, &candidate, &used));
    let unseen = about(PlanCheck::Unseen, &sentence(0));
    assert!(repaired(&mut passes, &candidate, &unseen));
    assert!(repaired(&mut passes, &candidate, &unseen));
    passes.worked_on(&unseen, [sentence(1)]);
    let used_again = about(PlanCheck::Used, &sentence(1));
    let unseen_again = about(PlanCheck::Unseen, &sentence(1));
    assert!(matches!(
        passes.next(&candidate, vec![used_again.clone(), unseen_again.clone()]),
        Next::Repair(_)
    ));
    assert_eq!(passes.inherited.get(&used_again.id()), Some(&1));
    assert_eq!(passes.inherited.get(&unseen_again.id()), Some(&2));
}

#[test]
fn alternating_kinds_on_rewritten_words_stop_after_the_limit_of_each_kind() {
    let rounds = usize::from(REPAIRS_PER_FINDING) * 4;
    let sentences: Vec<String> = (0..=rounds).map(sentence).collect();
    let candidate = candidate_of(&sentences);
    let mut passes = Passes::default();
    let mut charged = [0u8; 2];
    let mut ended = false;
    for round in 0..rounds {
        let kind = round % 2;
        let check = if kind == 0 {
            PlanCheck::Used
        } else {
            PlanCheck::Unseen
        };
        let finding = about(check, &sentence(round));
        if !repaired(&mut passes, &candidate, &finding) {
            ended = true;
            break;
        }
        charged[kind] += 1;
        passes.worked_on(&finding, [sentence(round + 1)]);
    }
    assert!(ended, "the chain never stopped");
    assert_eq!(charged, [REPAIRS_PER_FINDING; 2]);
}

#[test]
fn the_counts_per_kind_survive_a_saved_review() {
    let candidate = candidate_of(&[sentence(0)]);
    let mut passes = Passes::default();
    assert!(repaired(
        &mut passes,
        &candidate,
        &about(PlanCheck::Used, &sentence(0))
    ));
    let saved = passes.saved().unwrap().to_string();
    let mut resumed = Passes::default();
    resumed.resume(&saved);
    let same_kind = ReviewFinding::plan(
        PlanCheck::Used,
        "another-idea",
        FindingSite::Section(0),
        "Another note.",
    )
    .with_passage(&sentence(0));
    let other_kind = about(PlanCheck::Unseen, &sentence(0));
    assert!(matches!(
        resumed.next(&candidate, vec![same_kind.clone(), other_kind.clone()]),
        Next::Repair(_)
    ));
    assert_eq!(resumed.inherited.get(&same_kind.id()), Some(&1));
    assert_eq!(resumed.inherited.get(&other_kind.id()), None);
}

#[test]
fn state_saved_with_one_count_for_every_kind_resumes_as_none() {
    let id = serde_json::to_value(about(PlanCheck::Used, &sentence(0)).id()).unwrap();
    let mut saved = json!({"spent": [[id, 1]], "last": [], "worked": {}});
    saved["worked"][subject_of(&sentence(0)).as_str()] = json!(2);
    let saved = saved.to_string();
    let mut passes = Passes::default();
    passes.resume(&saved);
    assert!(passes.spent.is_empty());
    assert!(passes.worked.is_empty());
}
