#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::practice::session::fake_kind::fake_kind;
use crate::practice::session::kind::Kinds;
use lattice_core::domain::learning::practice::work::{
    Brief, Case, CaseVisibility, Difficulty, KindId, PartRef,
};
use serde_json::json;

const REFERENCE: [&str; 4] = ["a", "b", "c", "d"];

fn piece(slots: &[&str]) -> Piece {
    Piece::new(json!(slots))
}

fn cases() -> Vec<Case> {
    REFERENCE
        .iter()
        .enumerate()
        .map(|(index, expected)| Case {
            id: format!("c{index}"),
            title: format!("Slot {index}"),
            visibility: CaseVisibility::Hidden,
            body: json!({"index": index, "expected": expected}),
        })
        .collect()
}

fn misreading(id: &str, slots: &[&str]) -> Misreading {
    Misreading {
        id: id.to_owned(),
        description: format!("{id} reads the brief wrongly."),
        piece: piece(slots),
        killed_by: Vec::new(),
    }
}

/// Fails c0 only.
fn first_wrong() -> Misreading {
    misreading("m1", &["x", "b", "c", "d"])
}

/// Fails c0 and c1.
fn two_wrong() -> Misreading {
    misreading("m2", &["x", "y", "c", "d"])
}

/// Fails c1 and c2.
fn other_two_wrong() -> Misreading {
    misreading("m3", &["a", "y", "z", "d"])
}

fn draft(form: Form) -> Draft {
    Draft {
        kind: KindId::new("letters"),
        form,
        brief: Brief {
            title: "Spell it".to_owned(),
            difficulty: Difficulty::Easy,
            spec: json!(null),
        },
        reference: piece(&REFERENCE),
        misreadings: Vec::new(),
        cases: cases(),
        starter: None,
        asks: Vec::new(),
    }
}

async fn run(kind: &Kind, draft: Draft) -> Result<Proof> {
    prove(kind, draft, CancellationToken::new()).await
}

fn proven(proof: Result<Proof>) -> Exercise {
    match proof {
        Ok(Proof::Proven(exercise)) => exercise,
        other => panic!("expected a proven exercise, got {other:?}"),
    }
}

fn unproven(proof: Result<Proof>) -> String {
    match proof {
        Ok(Proof::Unproven(reason)) => reason,
        other => panic!("expected an unproven draft, got {other:?}"),
    }
}

fn invalid_state(proof: Result<Proof>) -> String {
    match proof {
        Err(AppError::InvalidState(message)) => message,
        other => panic!("expected an invalid-state error, got {other:?}"),
    }
}

#[tokio::test]
async fn a_wrong_reference_is_unproven_with_the_truths_reason() {
    let kind = fake_kind(&REFERENCE).build();
    let mut wrong = draft(Form::Build);
    wrong.reference = piece(&["a", "x", "c", "d"]);
    wrong.starter = Some(piece(&["", "", "", ""]));
    assert_eq!(
        unproven(run(&kind, wrong).await),
        "The reference fails the cases c1."
    );
}

#[tokio::test]
async fn a_misreading_that_passes_every_case_is_dropped_and_named() {
    let kind = fake_kind(&REFERENCE).needing(2).build();
    let mut with_twin = draft(Form::Build);
    with_twin.starter = Some(piece(&["", "", "", ""]));
    with_twin.misreadings = vec![first_wrong(), misreading("twin", &REFERENCE)];
    let reason = unproven(run(&kind, with_twin).await);
    assert!(reason.starts_with("Only 1 of the 2 misreadings fail a case and run; 2 are needed."));
    assert!(reason
        .contains("Misreading twin passes every case, so nothing tells it from the reference."));
}

#[tokio::test]
async fn a_misreading_that_errors_on_every_case_is_dropped_as_not_plausible() {
    let kind = fake_kind(&REFERENCE).needing(1).build();
    let mut with_broken = draft(Form::Build);
    with_broken.starter = Some(piece(&["", "", "", ""]));
    with_broken.misreadings = vec![Misreading {
        piece: Piece::new(json!("not an array")),
        ..misreading("broken", &[])
    }];
    let reason = unproven(run(&kind, with_broken).await);
    assert!(reason
        .contains("Misreading broken raises on every case, so it is not a plausible reading."));
}

#[tokio::test]
async fn kept_misreadings_carry_the_cases_that_kill_them() {
    let kind = fake_kind(&REFERENCE).needing(2).build();
    let mut build = draft(Form::Build);
    build.starter = Some(piece(&["", "", "", ""]));
    build.misreadings = vec![first_wrong(), two_wrong()];
    let exercise = proven(run(&kind, build).await);
    let killed: Vec<&Vec<String>> = exercise
        .misreadings
        .iter()
        .map(|misreading| &misreading.killed_by)
        .collect();
    assert_eq!(
        killed,
        [
            &vec!["c0".to_owned()],
            &vec!["c0".to_owned(), "c1".to_owned()]
        ]
    );
}

#[tokio::test]
async fn the_kinds_need_wins_when_it_is_larger_than_the_forms() {
    let kind = fake_kind(&REFERENCE).needing(2).build();
    let mut repair = draft(Form::Repair);
    repair.misreadings = vec![first_wrong()];
    let reason = unproven(run(&kind, repair).await);
    assert!(reason.contains("Only 1 of the 1 misreadings"));
    assert!(reason.contains("2 are needed."));
}

#[tokio::test]
async fn the_forms_need_wins_when_it_is_larger_than_the_kinds() {
    let kind = fake_kind(&REFERENCE).needing(0).build();
    let mut repair = draft(Form::Repair);
    repair.misreadings = vec![misreading("twin", &REFERENCE)];
    let reason = unproven(run(&kind, repair).await);
    assert!(reason.contains("Only 0 of the 1 misreadings"));
    assert!(reason.contains("1 are needed."));
}

#[tokio::test]
async fn no_misreadings_are_needed_when_neither_asks_for_one() {
    let kind = fake_kind(&REFERENCE).build();
    let mut build = draft(Form::Build);
    build.starter = Some(piece(&["", "", "", ""]));
    let exercise = proven(run(&kind, build).await);
    assert!(exercise.misreadings.is_empty());
}

#[tokio::test]
async fn repair_starts_from_the_most_killed_misreading_and_records_it() {
    let kind = fake_kind(&REFERENCE).build();
    let mut repair = draft(Form::Repair);
    repair.misreadings = vec![first_wrong(), two_wrong(), other_two_wrong()];
    let exercise = proven(run(&kind, repair).await);
    assert_eq!(exercise.planted.as_deref(), Some("m2"));
    assert_eq!(exercise.start, two_wrong().piece);
}

#[tokio::test]
async fn repair_takes_the_first_misreading_on_a_tie() {
    let kind = fake_kind(&REFERENCE).build();
    let mut repair = draft(Form::Repair);
    repair.misreadings = vec![other_two_wrong(), two_wrong()];
    let exercise = proven(run(&kind, repair).await);
    assert_eq!(exercise.planted.as_deref(), Some("m3"));
    assert_eq!(exercise.start, other_two_wrong().piece);
}

#[tokio::test]
async fn repair_with_no_kept_misreading_is_unproven() {
    let kind = fake_kind(&REFERENCE).build();
    let repair = draft(Form::Repair);
    assert!(!unproven(run(&kind, repair).await).is_empty());
}

#[tokio::test]
async fn build_starts_from_the_starter_and_plants_nothing() {
    let kind = fake_kind(&REFERENCE).build();
    let mut build = draft(Form::Build);
    build.starter = Some(piece(&["", "", "", ""]));
    let exercise = proven(run(&kind, build).await);
    assert_eq!(exercise.start, piece(&["", "", "", ""]));
    assert_eq!(exercise.planted, None);
}

#[tokio::test]
async fn build_without_a_starter_is_a_wiring_error() {
    let kind = fake_kind(&REFERENCE).build();
    let message = invalid_state(run(&kind, draft(Form::Build)).await);
    assert!(message.contains("starting piece"));
}

#[tokio::test]
async fn complete_empties_exactly_the_asked_parts() {
    let kind = fake_kind(&REFERENCE).with_gaps().build();
    let mut complete = draft(Form::Complete);
    complete.asks = vec![PartRef::new("1"), PartRef::new("3")];
    let exercise = proven(run(&kind, complete).await);
    assert_eq!(exercise.start, piece(&["a", "", "c", ""]));
}

#[tokio::test]
async fn complete_with_no_asked_part_is_unproven() {
    let kind = fake_kind(&REFERENCE).with_gaps().build();
    let reason = unproven(run(&kind, draft(Form::Complete)).await);
    assert!(reason.contains("names no part to empty"));
}

#[tokio::test]
async fn arrange_starts_from_an_order_that_differs_from_the_reference() {
    let kind = fake_kind(&REFERENCE).with_order().build();
    let exercise = proven(run(&kind, draft(Form::Arrange)).await);
    assert_ne!(exercise.start, exercise.reference);
    let mut held: Vec<String> = serde_json::from_value(exercise.start.value().clone()).unwrap();
    held.sort();
    assert_eq!(held, REFERENCE);
}

#[tokio::test]
async fn arrange_starts_the_same_on_a_second_proof_of_the_same_draft() {
    let kind = fake_kind(&REFERENCE).with_order().build();
    let first = proven(run(&kind, draft(Form::Arrange)).await);
    let second = proven(run(&kind, draft(Form::Arrange)).await);
    assert_eq!(first.start, second.start);
}

#[tokio::test]
async fn a_form_the_kind_does_not_support_is_an_invalid_state_error() {
    let kind = fake_kind(&REFERENCE).build();
    let message = invalid_state(run(&kind, draft(Form::Complete)).await);
    assert!(message.contains("letters"));
    assert!(invalid_state(run(&kind, draft(Form::Arrange)).await).contains("Arrange"));
}

#[tokio::test]
async fn an_ambiguous_exercise_is_unproven_with_the_reason() {
    let kind = fake_kind(&REFERENCE)
        .ambiguous("Two orders are right.")
        .build();
    let mut build = draft(Form::Build);
    build.starter = Some(piece(&["", "", "", ""]));
    assert_eq!(unproven(run(&kind, build).await), "Two orders are right.");
}

#[tokio::test]
async fn a_cancelled_token_ends_in_the_cancelled_error() {
    let kind = fake_kind(&REFERENCE).build();
    let cancel = CancellationToken::new();
    cancel.cancel();
    let mut build = draft(Form::Build);
    build.starter = Some(piece(&["", "", "", ""]));
    let message = invalid_state(prove(&kind, build, cancel).await);
    assert_eq!(message, "The practice build was cancelled.");
}

#[test]
fn a_repeated_kind_id_is_refused() {
    let refused = Kinds::new([fake_kind(&REFERENCE).build(), fake_kind(&REFERENCE).build()]);
    match refused {
        Err(AppError::InvalidState(message)) => assert!(message.contains("letters")),
        _ => panic!("expected an invalid-state error"),
    }
}

#[test]
fn an_unknown_kind_id_is_named() {
    let kinds = Kinds::new([fake_kind(&REFERENCE).build()]).unwrap();
    assert!(kinds.get(&KindId::new("letters")).is_ok());
    match kinds.get(&KindId::new("shapes")) {
        Err(AppError::InvalidState(message)) => assert!(message.contains("shapes")),
        _ => panic!("expected an invalid-state error"),
    }
    assert_eq!(kinds.iter().count(), 1);
}

#[test]
fn a_kind_supports_the_forms_its_roles_allow() {
    let bare = fake_kind(&REFERENCE).build();
    assert!(bare.supports(Form::Build) && bare.supports(Form::Repair));
    assert!(!bare.supports(Form::Complete) && !bare.supports(Form::Arrange));
    let gaps = fake_kind(&REFERENCE).with_gaps().build();
    assert!(gaps.supports(Form::Complete) && !gaps.supports(Form::Arrange));
    let order = fake_kind(&REFERENCE).with_order().build();
    assert!(order.supports(Form::Arrange) && !order.supports(Form::Complete));
}

#[tokio::test]
async fn a_draft_made_for_another_kind_is_refused() {
    let kind = fake_kind(&REFERENCE).build();
    let mut other = draft(Form::Build);
    other.kind = KindId::new("shapes");
    other.starter = Some(piece(&["", "", "", ""]));
    assert_eq!(
        invalid_state(run(&kind, other).await),
        "The draft is for the kind shapes, not letters."
    );
}
