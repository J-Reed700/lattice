#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::practice::coding::problem::{CaseResult as RunResult, CaseStatus as RunStatus};
use crate::practice::coding::problem::{CaseVisibility, ProblemCase};
use crate::practice::coding::proof::fakes::{cases, spec};
use crate::practice::kinds::program::shape::{brief_of, case_of, piece_of, result_of};
use lattice_core::shared::error::AppError;
use serde_json::json;

fn brief() -> Brief {
    brief_of(&spec()).unwrap()
}

fn shown() -> Vec<Case> {
    cases()
        .iter()
        .filter(|case| case.visibility == CaseVisibility::Example)
        .map(|case| case_of(case).unwrap())
        .collect()
}

fn problem_case(id: &str) -> ProblemCase {
    cases().into_iter().find(|case| case.id == id).unwrap()
}

fn run(status: RunStatus) -> RunResult {
    RunResult {
        case_id: "hidden-1".into(),
        status,
        actual: Some("[1, 0]".into()),
        printed: "hi".into(),
        error: None,
        elapsed_ms: Some(3),
    }
}

fn line(id: &str, result: Option<RunResult>, open: bool) -> String {
    let case = case_of(&problem_case(id)).unwrap();
    let result = result.map(|result| result_of(&result).unwrap());
    ProgramShowing
        .case(&brief(), &case, result.as_ref(), open)
        .unwrap()
}

#[test]
fn the_brief_states_the_task_its_examples_and_its_techniques() {
    let name = problem_pattern("hash_map_lookup").unwrap().name;
    assert_eq!(
        ProgramShowing.brief(&brief(), &shown()).unwrap(),
        format!(
            "Problem: Pair with a given sum\n\
             Return the positions of the two numbers that add up to the target.\n\
             Signature: def pair_positions(numbers: list[int], target: int) -> list[int]\n\
             Constraint: There is exactly one pair.\n\
             Example in the statement: a pair at the start; input ([1, 2], 3); expected [0, 1]\n\
             Example in the statement: a pair in the middle; input ([1, 2], 3); expected [0, 1]\n\
             Clarification 0: asked \"May a number be used twice?\", answered \"No, each position is used at most once.\"\n\
             Target complexity: O(n) time, O(n) space\n\
             Techniques the problem is built around: {name}\n"
        )
    );
}

#[test]
fn a_brief_with_no_patterns_names_no_techniques() {
    let mut bare = spec();
    bare.patterns = Vec::new();
    bare.constraints = Vec::new();
    bare.clarifications = Vec::new();
    let text = ProgramShowing
        .brief(&brief_of(&bare).unwrap(), &[])
        .unwrap();
    assert_eq!(
        text,
        "Problem: Pair with a given sum\n\
         Return the positions of the two numbers that add up to the target.\n\
         Signature: def pair_positions(numbers: list[int], target: int) -> list[int]\n\
         Target complexity: O(n) time, O(n) space\n"
    );
}

#[test]
fn a_brief_whose_spec_is_not_a_programs_is_invalid() {
    let mut foreign = brief();
    foreign.spec = json!(["not", "a", "spec"]);
    assert!(matches!(
        ProgramShowing.brief(&foreign, &[]),
        Err(AppError::InvalidState(_))
    ));
}

#[test]
fn a_piece_is_every_file_with_its_lines_numbered() {
    let piece = piece_of(&[
        LearningLabFile {
            path: "solution.py".into(),
            content: "def f():\n    pass\n".into(),
        },
        LearningLabFile {
            path: "empty.py".into(),
            content: String::new(),
        },
    ])
    .unwrap();
    assert_eq!(
        ProgramShowing.piece(&brief(), &piece).unwrap(),
        "=== solution.py ===\n1 | def f():\n2 |     pass\n=== empty.py ===\n"
    );
}

#[test]
fn a_closed_case_shows_only_its_category() {
    let result = Some(run(RunStatus::Failed));
    assert_eq!(line("hidden-1", result, false), "duplicate numbers");
    assert_eq!(line("hidden-1", None, false), "duplicate numbers");
}

#[test]
fn an_open_case_shows_its_input_expected_and_result() {
    assert_eq!(
        line("hidden-1", Some(run(RunStatus::Failed)), true),
        "duplicate numbers; input ([1, 2], 3); expected [0, 1]; got [1, 0]; printed hi"
    );
}

#[test]
fn an_open_case_that_raised_shows_the_error() {
    let mut raised = run(RunStatus::Error);
    raised.actual = None;
    raised.printed = String::new();
    raised.error = Some("ValueError: bad".into());
    assert_eq!(
        line("hidden-1", Some(raised), true),
        "duplicate numbers; input ([1, 2], 3); expected [0, 1]; raised ValueError: bad"
    );
}

#[test]
fn an_open_case_not_yet_checked_shows_its_input_and_expected() {
    assert_eq!(
        line("example-1", None, true),
        "a pair at the start; input ([1, 2], 3); expected [0, 1]"
    );
}

#[test]
fn a_scale_case_shows_no_input_and_no_actual_value_even_when_open() {
    let mut big = run(RunStatus::Ran);
    big.case_id = "scale-1".into();
    big.actual = Some("'big'".into());
    big.printed = String::new();
    assert_eq!(line("scale-1", Some(big.clone()), true), "a long list");
    assert_eq!(line("scale-1", Some(big), false), "a long list");
}

#[test]
fn a_result_whose_detail_is_not_a_programs_is_invalid() {
    let case = case_of(&problem_case("hidden-1")).unwrap();
    let foreign = lattice_core::domain::learning::practice::work::CaseResult {
        case_id: "hidden-1".into(),
        status: lattice_core::domain::learning::practice::work::CaseStatus::Failed,
        detail: json!("nope"),
    };
    assert!(matches!(
        ProgramShowing.case(&brief(), &case, Some(&foreign), true),
        Err(AppError::InvalidState(_))
    ));
}

#[test]
fn a_run_notes_what_it_printed_outside_any_case_and_its_error_output() {
    assert_eq!(
        run_notes("out", "err"),
        ["Printed outside any case: out", "Error output: err"]
    );
    assert_eq!(run_notes("out", ""), ["Printed outside any case: out"]);
    assert_eq!(run_notes("", "err"), ["Error output: err"]);
    assert!(run_notes("", "").is_empty());
}

#[test]
fn the_notes_of_a_check_come_from_its_detail() {
    let notes = |detail| ProgramShowing.check_notes(&brief(), &detail);
    assert!(notes(serde_json::Value::Null).unwrap().is_empty());
    assert_eq!(
        notes(json!({"stdout": "out", "stderr": "err"})).unwrap(),
        ["Printed outside any case: out", "Error output: err"]
    );
    assert!(notes(json!({"stdout": "", "stderr": ""}))
        .unwrap()
        .is_empty());
    assert!(matches!(
        notes(json!("nope")),
        Err(AppError::InvalidState(_))
    ));
}
