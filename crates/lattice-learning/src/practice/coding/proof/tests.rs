#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::fakes::{cases, spec, Behaviour, FakeHarness, FakeRunner};
use super::*;
use crate::persistence::hash_text;

/// A runner whose code fails the cases listed against the text it contains and passes the rest; code containing
/// BROKEN raises on every case.
fn runner(fails: Vec<(&'static str, Vec<&'static str>)>, scale_ms: u64) -> FakeRunner {
    FakeRunner::new(move |code, case| {
        if code.contains("BROKEN") {
            return Behaviour::Raises("SyntaxError: invalid syntax");
        }
        let failing = fails
            .iter()
            .any(|(text, ids)| code.contains(text) && ids.contains(&case.id.as_str()));
        if failing {
            Behaviour::Fails
        } else {
            Behaviour::Passes {
                ms: if case.scale { scale_ms } else { 1 },
            }
        }
    })
}

fn misreading(id: &str, code: &str) -> Misreading {
    Misreading {
        id: id.to_owned(),
        description: format!("{id} reads the statement wrongly."),
        code: code.to_owned(),
        killed_by: Vec::new(),
    }
}

async fn prove_with(
    runner: &FakeRunner,
    reference: &str,
    misreadings: &[Misreading],
) -> Result<Proof> {
    prove(
        runner,
        &FakeHarness,
        &spec(),
        reference,
        misreadings,
        &cases(),
        CancellationToken::new(),
    )
    .await
}

fn proven(proof: Result<Proof>) -> Proven {
    match proof {
        Ok(Proof::Proven(proven)) => proven,
        other => panic!("expected a proven problem, got {other:?}"),
    }
}

fn unproven(proof: Result<Proof>) -> String {
    match proof {
        Ok(Proof::Unproven(reason)) => reason,
        other => panic!("expected an unproven draft, got {other:?}"),
    }
}

fn two_killed() -> Vec<Misreading> {
    vec![
        misreading("misreading-1", "LATE"),
        misreading("misreading-2", "EARLY"),
    ]
}

fn killers() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("LATE", vec!["hidden-1", "hidden-3"]),
        ("EARLY", vec!["example-2"]),
    ]
}

#[tokio::test]
async fn a_reference_that_fails_a_case_is_unproven_and_the_failure_is_named() {
    let runner = runner(vec![("REF", vec!["hidden-1"])], 5);
    let reason = unproven(prove_with(&runner, "REF", &two_killed()).await);
    assert!(reason.contains("hidden-1"), "{reason}");
    assert!(reason.contains("duplicate numbers"), "{reason}");
    assert!(!reason.contains("example-1"), "{reason}");
}

#[tokio::test]
async fn a_reference_that_raises_is_unproven_with_the_error() {
    let runner = runner(Vec::new(), 5);
    let reason = unproven(prove_with(&runner, "BROKEN", &two_killed()).await);
    assert!(reason.contains("SyntaxError"), "{reason}");
}

#[tokio::test]
async fn scale_cases_take_their_digest_and_time_from_the_reference_run() {
    let runner = runner(killers(), 7);
    let proven = proven(prove_with(&runner, "REF", &two_killed()).await);
    let scale = proven.cases.iter().find(|case| case.scale).unwrap();
    assert_eq!(scale.digest, Some(hash_text("'big'")));
    assert_eq!(scale.expected, None);
    assert_eq!(proven.reference_ms.get("scale-1"), Some(&7));
    assert!(proven
        .cases
        .iter()
        .filter(|case| !case.scale)
        .all(|case| case.digest.is_none()));
}

#[tokio::test]
async fn a_reference_slower_than_its_share_of_the_time_limit_is_unproven() {
    let runner = runner(killers(), scale_budget_ms() + 1);
    let reason = unproven(prove_with(&runner, "REF", &two_killed()).await);
    assert!(reason.contains("scale"), "{reason}");
}

#[tokio::test]
async fn a_misreading_that_passes_every_case_is_dropped() {
    let runner = runner(killers(), 5);
    let mut misreadings = two_killed();
    misreadings.push(misreading("misreading-3", "SAME"));
    let proven = proven(prove_with(&runner, "REF", &misreadings).await);
    let ids: Vec<&str> = proven
        .misreadings
        .iter()
        .map(|misreading| misreading.id.as_str())
        .collect();
    assert_eq!(ids, ["misreading-1", "misreading-2"]);
}

#[tokio::test]
async fn killed_by_records_the_cases_that_kill_each_misreading() {
    let runner = runner(killers(), 5);
    let proven = proven(prove_with(&runner, "REF", &two_killed()).await);
    assert_eq!(proven.misreadings[0].killed_by, ["hidden-1", "hidden-3"]);
    assert_eq!(proven.misreadings[1].killed_by, ["example-2"]);
}

#[tokio::test]
async fn fewer_than_two_killed_misreadings_is_unproven() {
    let runner = runner(killers(), 5);
    let misreadings = vec![
        misreading("misreading-1", "LATE"),
        misreading("misreading-2", "SAME"),
        misreading("misreading-3", "ALSO SAME"),
    ];
    let reason = unproven(prove_with(&runner, "REF", &misreadings).await);
    assert!(reason.contains("misreading-2"), "{reason}");
    assert!(reason.contains("misreading-3"), "{reason}");
    assert!(!reason.contains("misreading-1 passes"), "{reason}");
}

#[tokio::test]
async fn a_misreading_that_raises_on_every_case_does_not_count() {
    let runner = runner(killers(), 5);
    let misreadings = vec![
        misreading("misreading-1", "LATE"),
        misreading("misreading-2", "BROKEN"),
    ];
    let reason = unproven(prove_with(&runner, "REF", &misreadings).await);
    assert!(
        reason.contains("misreading-2 raises on every case"),
        "{reason}"
    );
}

#[tokio::test]
async fn a_runtime_that_is_not_there_is_an_error_and_not_a_verdict_on_the_draft() {
    let runner = FakeRunner::unavailable("no interpreter");
    let result = prove_with(&runner, "REF", &two_killed()).await;
    assert!(
        matches!(result, Err(AppError::ServiceNotAvailable(_))),
        "{result:?}"
    );
}

#[tokio::test]
async fn a_cancelled_build_stops_before_running_anything() {
    let runner = runner(killers(), 5);
    let cancel = CancellationToken::new();
    cancel.cancel();
    let result = prove(
        &runner,
        &FakeHarness,
        &spec(),
        "REF",
        &two_killed(),
        &cases(),
        cancel,
    )
    .await;
    assert!(
        matches!(result, Err(AppError::InvalidState(_))),
        "{result:?}"
    );
    assert!(runner.codes().is_empty());
}
