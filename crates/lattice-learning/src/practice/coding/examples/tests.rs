use super::*;
use crate::practice::coding::harness::fake::{
    sample_problem, ScriptedRunner, NOT_A_LITERAL, UNLUCKY,
};
use crate::practice::coding::harness::PythonHarness;

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

async fn check(args: &[&str], expected: &str) -> ExampleCheck {
    check_example(
        &ScriptedRunner::new(),
        &PythonHarness::new(),
        &sample_problem(),
        &strings(args),
        expected,
        CancellationToken::new(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn an_example_the_reference_disagrees_with_is_wrong_with_the_references_answer() {
    let checked = check(&["2", "3"], "6").await;
    assert_eq!(checked.verdict, ExampleVerdict::Wrong);
    assert_eq!(checked.reference_actual.as_deref(), Some("5"));
    assert!(checked.why_invalid.is_none());
    assert!(checked.catches.is_empty());
}

#[tokio::test]
async fn an_example_catches_exactly_the_misreadings_that_disagree_with_it() {
    let only_zero = check(&["5", "0"], "5").await;
    assert_eq!(only_zero.verdict, ExampleVerdict::Right);
    assert_eq!(only_zero.catches, ["zero"]);

    let all = check(&["4", "4"], "8").await;
    assert_eq!(all.verdict, ExampleVerdict::Right);
    assert_eq!(all.catches, ["subtracts", "first", "zero"]);
}

#[tokio::test]
async fn an_example_with_the_wrong_arity_is_invalid_and_runs_nothing() {
    let runner = ScriptedRunner::new();
    let checked = check_example(
        &runner,
        &PythonHarness::new(),
        &sample_problem(),
        &strings(&["1"]),
        "1",
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(checked.verdict, ExampleVerdict::Invalid);
    assert!(checked
        .why_invalid
        .as_deref()
        .is_some_and(|why| why.contains("2 argument")));
    assert!(runner.files_seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_example_that_is_not_a_literal_is_invalid_and_names_the_value() {
    let argument = check(&["2", NOT_A_LITERAL], "2").await;
    assert_eq!(argument.verdict, ExampleVerdict::Invalid);
    assert!(argument
        .why_invalid
        .as_deref()
        .is_some_and(|why| why.contains("for b")));

    let expected = check(&["2", "3"], NOT_A_LITERAL).await;
    assert_eq!(expected.verdict, ExampleVerdict::Invalid);
    assert!(expected
        .why_invalid
        .as_deref()
        .is_some_and(|why| why.contains("expected result")));
}

#[tokio::test]
async fn an_example_the_reference_raises_on_is_invalid() {
    let checked = check(&[&UNLUCKY.to_string(), "1"], "14").await;
    assert_eq!(checked.verdict, ExampleVerdict::Invalid);
    assert!(checked
        .why_invalid
        .as_deref()
        .is_some_and(|why| why.contains("ValueError")));
    assert!(checked.catches.is_empty());
}

#[tokio::test]
async fn the_reference_answers_an_input_without_an_expectation() {
    let result = reference_answer(
        &ScriptedRunner::new(),
        &PythonHarness::new(),
        &sample_problem(),
        &strings(&["10", "20"]),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(result.status, CaseStatus::Ran);
    assert_eq!(result.actual.as_deref(), Some("30"));
}
