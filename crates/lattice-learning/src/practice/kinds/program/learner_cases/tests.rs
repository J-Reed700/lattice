#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::practice::coding::proof::fakes::{sample_problem, spec};
use crate::practice::kinds::program::shape::{brief_of, exercise_of, problem_case_of};
use lattice_core::domain::learning::practice::work::CaseVisibility;
use serde_json::json;

#[test]
fn a_case_reads_as_a_call_and_what_it_expects() {
    let brief = brief_of(&spec()).unwrap();
    let text = ProgramLearnerCases
        .show(
            &brief,
            &json!({"args": ["[1, 2]", "3"], "expected": "[0, 1]"}),
        )
        .unwrap();
    assert_eq!(text, "([1, 2], 3) expects [0, 1]");
}

#[test]
fn a_case_with_no_arguments_reads_as_an_empty_call() {
    let brief = brief_of(&spec()).unwrap();
    let text = ProgramLearnerCases
        .show(&brief, &json!({"args": [], "expected": "0"}))
        .unwrap();
    assert_eq!(text, "() expects 0");
}

#[test]
fn a_body_that_is_not_a_program_case_is_invalid() {
    let brief = brief_of(&spec()).unwrap();
    for body in [
        json!(null),
        json!({"args": ["1"]}),
        json!({"expected": "1"}),
    ] {
        assert!(matches!(
            ProgramLearnerCases.show(&brief, &body),
            Err(AppError::InvalidState(_))
        ));
    }
}

fn learner_case(ordinal: u32, args: &[&str], expected: &str) -> SessionLearnerCase {
    SessionLearnerCase {
        ordinal,
        body: json!({"args": args, "expected": expected}),
        verdict: lattice_core::domain::learning::practice::work::LearnerCaseVerdict::Right,
        catches: Vec::new(),
        found: serde_json::Value::Null,
    }
}

#[test]
fn a_learners_case_is_a_shown_case_a_check_runs() {
    let exercise = exercise_of(&sample_problem()).unwrap();
    let case = ProgramLearnerCases
        .case(&exercise, &learner_case(3, &["[1, 2]"], "3"))
        .unwrap();
    assert_eq!(case.id, "mine-3");
    assert_eq!(case.title, "Your example 3");
    assert_eq!(case.visibility, CaseVisibility::Shown);
    let run = problem_case_of(&case).unwrap();
    assert_eq!(run.args, ["[1, 2]"]);
    assert_eq!(run.expected.as_deref(), Some("3"));
    assert!(!run.scale);
    assert_eq!(run.digest, None);
}

#[test]
fn a_learners_case_compares_the_way_the_statements_examples_do() {
    let problem = sample_problem();
    let exercise = exercise_of(&problem).unwrap();
    let case = ProgramLearnerCases
        .case(&exercise, &learner_case(1, &["1"], "2"))
        .unwrap();
    assert_eq!(
        problem_case_of(&case).unwrap().compare,
        problem.default_compare()
    );
}

#[test]
fn a_learners_case_whose_body_is_not_a_programs_is_invalid() {
    let exercise = exercise_of(&sample_problem()).unwrap();
    let mut bad = learner_case(1, &["1"], "2");
    bad.body = json!({"args": ["1"]});
    assert!(matches!(
        ProgramLearnerCases.case(&exercise, &bad),
        Err(AppError::InvalidState(_))
    ));
}
