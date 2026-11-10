#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use super::*;
use crate::practice::coding::problem::Misreading;
use crate::practice::coding::proof::fakes::{cases, spec, Behaviour, FakeHarness, FakeRunner};
use crate::practice::coding::proof::{prove, scale_budget_ms, Proof};
use crate::practice::kinds::program::shape::{brief_of, solution_piece};
use crate::practice::kinds::program::toolchain::FakeToolchains;
use lattice_core::domain::learning::practice::work::{Form, KindId};
use lattice_core::shared::error::AppError;
use serde_json::json;

fn runner(scale_ms: u64) -> FakeRunner {
    FakeRunner::new(move |code, case| {
        if code.contains("BROKEN") {
            return Behaviour::Raises("SyntaxError: invalid syntax");
        }
        let failing = (code.contains("FAILS") && case.id == "hidden-1")
            || (code.contains("LATE") && ["hidden-1", "hidden-3"].contains(&case.id.as_str()))
            || (code.contains("EARLY") && case.id == "example-2");
        if failing {
            Behaviour::Fails
        } else {
            Behaviour::Passes {
                ms: if case.scale { scale_ms } else { 1 },
            }
        }
    })
}

fn misreadings() -> Vec<Misreading> {
    ["LATE", "EARLY"]
        .iter()
        .map(|code| Misreading {
            id: format!("misreading-{code}"),
            description: String::new(),
            code: (*code).to_owned(),
            killed_by: Vec::new(),
        })
        .collect()
}

fn draft(reference: &str) -> Draft {
    Draft {
        kind: KindId::new(crate::practice::kinds::program::KIND),
        form: Form::Build,
        brief: brief_of(&spec()).unwrap(),
        reference: solution_piece(reference),
        misreadings: Vec::new(),
        cases: cases().iter().map(|case| case_of(case).unwrap()).collect(),
        starter: None,
        asks: Vec::new(),
    }
}

async fn established(
    runner: FakeRunner,
    reference: &str,
    cancel: CancellationToken,
) -> Result<Established> {
    let truth = ProgramTruth {
        tools: Arc::new(FakeToolchains::new(runner)),
    };
    truth.establish(&draft(reference), cancel).await
}

async fn proved(runner: &FakeRunner, reference: &str, cancel: CancellationToken) -> Result<Proof> {
    prove(
        runner,
        &FakeHarness,
        &spec(),
        reference,
        &misreadings(),
        &cases(),
        cancel,
    )
    .await
}

#[tokio::test]
async fn a_right_reference_gives_the_cases_and_timings_prove_gives() {
    let Ok(Proof::Proven(proven)) = proved(&runner(7), "REF", CancellationToken::new()).await
    else {
        panic!("the reference should be proven");
    };
    let Ok(Established::Right { cases, proof }) =
        established(runner(7), "REF", CancellationToken::new()).await
    else {
        panic!("the reference should be right");
    };
    let back: Vec<_> = cases
        .iter()
        .map(|case| problem_case_of(case).unwrap())
        .collect();
    assert_eq!(back, proven.cases);
    assert_eq!(proof, json!({"referenceMs": {"scale-1": 7}}));
    assert_eq!(proof, proof_of(&proven.reference_ms).unwrap());
}

async fn wrong_note(
    runner_for_prove: FakeRunner,
    runner_for_establish: FakeRunner,
    reference: &str,
) {
    let Ok(Proof::Unproven(reason)) =
        proved(&runner_for_prove, reference, CancellationToken::new()).await
    else {
        panic!("the reference should be unproven");
    };
    let Ok(Established::Wrong(note)) =
        established(runner_for_establish, reference, CancellationToken::new()).await
    else {
        panic!("the reference should be wrong");
    };
    assert_eq!(note, reason);
}

#[tokio::test]
async fn a_reference_that_fails_a_case_gives_the_sentence_prove_gives() {
    wrong_note(runner(5), runner(5), "FAILS").await;
}

#[tokio::test]
async fn a_reference_that_raises_gives_the_sentence_prove_gives() {
    wrong_note(runner(5), runner(5), "BROKEN").await;
}

#[tokio::test]
async fn a_reference_over_the_scale_budget_gives_the_sentence_prove_gives() {
    let slow = scale_budget_ms() + 1;
    wrong_note(runner(slow), runner(slow), "REF").await;
}

#[tokio::test]
async fn a_runtime_that_is_not_there_is_an_error_as_in_prove() {
    let unavailable = FakeRunner::unavailable("no interpreter");
    let proof = proved(&unavailable, "REF", CancellationToken::new()).await;
    let truth = established(
        FakeRunner::unavailable("no interpreter"),
        "REF",
        CancellationToken::new(),
    )
    .await;
    assert!(matches!(proof, Err(AppError::ServiceNotAvailable(_))));
    assert!(matches!(truth, Err(AppError::ServiceNotAvailable(_))));
}

#[tokio::test]
async fn a_cancelled_build_is_an_error_as_in_prove() {
    let cancel = CancellationToken::new();
    cancel.cancel();
    let proof = proved(&runner(5), "REF", cancel.clone()).await;
    let truth = established(runner(5), "REF", cancel).await;
    assert!(matches!(proof, Err(AppError::InvalidState(_))));
    assert!(matches!(truth, Err(AppError::InvalidState(_))));
}

#[tokio::test]
async fn a_program_is_never_ambiguous() {
    let truth = ProgramTruth {
        tools: Arc::new(FakeToolchains::new(runner(5))),
    };
    let exercise = crate::practice::kinds::program::shape::exercise_of(
        &crate::practice::coding::proof::fakes::sample_problem(),
    )
    .unwrap();
    let ambiguity = truth
        .unambiguous(&exercise, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(ambiguity, None);
}
