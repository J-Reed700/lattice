//! The engine's proof of a program agrees with today's proof of a problem, on the same inputs.

#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use crate::practice::coding::problem::Misreading as ProblemMisreading;
use crate::practice::coding::proof::fakes::{cases, spec, Behaviour, FakeHarness, FakeRunner};
use crate::practice::coding::proof::{prove as prove_today, scale_budget_ms, Proof as TodaysProof};
use crate::practice::kinds::program::shape::{
    brief_of, case_of, misreading_of, proof_of, solution_piece,
};
use crate::practice::kinds::program::toolchain::FakeToolchains;
use crate::practice::kinds::program::{kind, KIND};
use crate::practice::session::proof::{prove, Proof};
use lattice_core::domain::learning::practice::work::{Draft, Exercise, Form, KindId};
use serde_json::json;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

const FORMS: [Form; 2] = [Form::Build, Form::Repair];

/// A runner whose code fails the cases listed against the text it contains and passes the rest; code containing
/// BROKEN raises on every case.
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

fn misreading(code: &str) -> ProblemMisreading {
    ProblemMisreading {
        id: format!("misreading-{code}"),
        description: format!("{code} reads the statement wrongly."),
        code: code.to_owned(),
        killed_by: Vec::new(),
    }
}

fn two_killed() -> Vec<ProblemMisreading> {
    vec![misreading("LATE"), misreading("EARLY")]
}

/// What both proofs found, once they agree.
#[derive(Debug)]
enum Agreed {
    Proven(Box<Exercise>),
    Unproven(String),
    Failed,
}

fn draft(form: Form, reference: &str, misreadings: &[ProblemMisreading]) -> Draft {
    Draft {
        kind: KindId::new(KIND),
        form,
        brief: brief_of(&spec()).unwrap(),
        reference: solution_piece(reference).unwrap(),
        misreadings: misreadings
            .iter()
            .map(|misreading| misreading_of(misreading).unwrap())
            .collect(),
        cases: cases().iter().map(|case| case_of(case).unwrap()).collect(),
        starter: (form == Form::Build).then(|| solution_piece(&spec().starter).unwrap()),
        asks: Vec::new(),
    }
}

/// Proves the same draft both ways and fails unless they agree.
async fn both(
    form: Form,
    scale_ms: u64,
    reference: &str,
    misreadings: &[ProblemMisreading],
) -> Agreed {
    let today = prove_today(
        &runner(scale_ms),
        &FakeHarness,
        &spec(),
        reference,
        misreadings,
        &cases(),
        CancellationToken::new(),
    )
    .await;
    let program = kind(Arc::new(FakeToolchains::new(runner(scale_ms))));
    let engine = prove(
        &program,
        draft(form, reference, misreadings),
        CancellationToken::new(),
    )
    .await;
    match (today, engine) {
        (Ok(TodaysProof::Proven(old)), Ok(Proof::Proven(new))) => {
            let old_cases: Vec<_> = old.cases.iter().map(|c| case_of(c).unwrap()).collect();
            let old_misreadings: Vec<_> = old
                .misreadings
                .iter()
                .map(|m| misreading_of(m).unwrap())
                .collect();
            assert_eq!(new.cases, old_cases, "{form:?}");
            assert_eq!(new.misreadings, old_misreadings, "{form:?}");
            assert_eq!(new.proof, proof_of(&old.reference_ms).unwrap(), "{form:?}");
            Agreed::Proven(new)
        }
        (Ok(TodaysProof::Unproven(old)), Ok(Proof::Unproven(new))) => {
            assert_eq!(new, old, "{form:?}");
            Agreed::Unproven(new)
        }
        (Err(old), Err(new)) => {
            assert_eq!(format!("{new:?}"), format!("{old:?}"), "{form:?}");
            Agreed::Failed
        }
        (old, new) => panic!("{form:?}: today {old:?}, the engine {new:?}"),
    }
}

fn proven(agreed: Agreed) -> Exercise {
    match agreed {
        Agreed::Proven(exercise) => *exercise,
        other => panic!("expected a proven exercise, got {other:?}"),
    }
}

fn unproven(agreed: Agreed) -> String {
    match agreed {
        Agreed::Unproven(reason) => reason,
        other => panic!("expected an unproven draft, got {other:?}"),
    }
}

#[tokio::test]
async fn a_reference_that_fails_a_case_is_unproven_with_the_same_sentence() {
    for form in FORMS {
        let reason = unproven(both(form, 5, "FAILS", &two_killed()).await);
        assert!(reason.contains("hidden-1"), "{reason}");
        assert!(reason.contains("duplicate numbers"), "{reason}");
    }
}

#[tokio::test]
async fn a_reference_that_raises_is_unproven_with_the_same_sentence() {
    for form in FORMS {
        let reason = unproven(both(form, 5, "BROKEN", &two_killed()).await);
        assert!(reason.contains("SyntaxError"), "{reason}");
    }
}

#[tokio::test]
async fn a_proven_draft_has_the_same_cases_misreadings_and_timings() {
    for form in FORMS {
        let exercise = proven(both(form, 7, "REF", &two_killed()).await);
        let scale = exercise
            .cases
            .iter()
            .find(|case| case.id == "scale-1")
            .unwrap();
        assert!(scale.body["digest"].is_string());
        assert_eq!(exercise.proof, json!({"referenceMs": {"scale-1": 7}}));
        assert_eq!(exercise.misreadings[0].killed_by, ["hidden-1", "hidden-3"]);
        assert_eq!(exercise.misreadings[1].killed_by, ["example-2"]);
    }
}

#[tokio::test]
async fn a_reference_slower_than_its_share_of_the_time_limit_is_unproven_with_the_same_sentence() {
    for form in FORMS {
        let reason = unproven(both(form, scale_budget_ms() + 1, "REF", &two_killed()).await);
        assert!(reason.contains("scale"), "{reason}");
    }
}

#[tokio::test]
async fn a_misreading_that_passes_every_case_is_dropped_by_both() {
    for form in FORMS {
        let mut misreadings = two_killed();
        misreadings.push(misreading("SAME"));
        let exercise = proven(both(form, 5, "REF", &misreadings).await);
        let ids: Vec<&str> = exercise
            .misreadings
            .iter()
            .map(|misreading| misreading.id.as_str())
            .collect();
        assert_eq!(ids, ["misreading-LATE", "misreading-EARLY"]);
    }
}

#[tokio::test]
async fn fewer_than_two_killed_misreadings_is_unproven_with_the_same_sentence() {
    for form in FORMS {
        let misreadings = vec![
            misreading("LATE"),
            misreading("SAME"),
            misreading("ALSO SAME"),
        ];
        let reason = unproven(both(form, 5, "REF", &misreadings).await);
        assert!(reason.contains("misreading-SAME"), "{reason}");
        assert!(reason.contains("misreading-ALSO SAME"), "{reason}");
    }
}

#[tokio::test]
async fn a_misreading_that_raises_on_every_case_does_not_count_for_either() {
    for form in FORMS {
        let misreadings = vec![misreading("LATE"), misreading("BROKEN")];
        let reason = unproven(both(form, 5, "REF", &misreadings).await);
        assert!(
            reason.contains("misreading-BROKEN raises on every case"),
            "{reason}"
        );
    }
}

#[tokio::test]
async fn a_runtime_that_is_not_there_is_the_same_error_for_both() {
    for form in FORMS {
        let today = prove_today(
            &FakeRunner::unavailable("no interpreter"),
            &FakeHarness,
            &spec(),
            "REF",
            &two_killed(),
            &cases(),
            CancellationToken::new(),
        )
        .await;
        let program = kind(Arc::new(FakeToolchains::new(FakeRunner::unavailable(
            "no interpreter",
        ))));
        let engine = prove(
            &program,
            draft(form, "REF", &two_killed()),
            CancellationToken::new(),
        )
        .await;
        let (Err(old), Err(new)) = (today, engine) else {
            panic!("{form:?}: both should fail");
        };
        assert_eq!(format!("{new:?}"), format!("{old:?}"), "{form:?}");
        assert!(format!("{new:?}").contains("no interpreter"));
    }
}

#[tokio::test]
async fn a_repair_starts_from_the_misreading_that_fails_the_most_cases() {
    let exercise = proven(both(Form::Repair, 5, "REF", &two_killed()).await);
    assert_eq!(exercise.planted.as_deref(), Some("misreading-LATE"));
    assert_eq!(exercise.start, exercise.misreadings[0].piece);
}

#[tokio::test]
async fn a_build_starts_from_the_starter() {
    let exercise = proven(both(Form::Build, 5, "REF", &two_killed()).await);
    assert_eq!(exercise.planted, None);
    assert_eq!(exercise.start, solution_piece(&spec().starter).unwrap());
}

#[test]
fn a_programs_misreadings_are_needed_for_the_examples_coverage_line() {
    let program = kind(Arc::new(FakeToolchains::new(runner(5))));
    assert_eq!(program.misreadings_needed.count, 2);
    assert!(
        program.misreadings_needed.reason.contains("coverage line"),
        "{}",
        program.misreadings_needed.reason
    );
}
