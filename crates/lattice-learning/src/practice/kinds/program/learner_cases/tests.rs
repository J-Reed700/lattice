#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::practice::coding::proof::fakes::spec;
use crate::practice::kinds::program::shape::brief_of;
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
