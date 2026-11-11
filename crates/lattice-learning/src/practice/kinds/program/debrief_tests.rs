//! What a program is scored on, how its session is told to the debrief's model, and how the model cites it back.

#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use crate::lab_runtime::LearningLabFile;
use crate::practice::coding::observation::test_support::problem;
use crate::practice::coding::problem::{CaseResult as RunResult, CaseStatus as RunStatus};
use crate::practice::kinds::program::account::ProgramAccount;
use crate::practice::kinds::program::learner_cases::ProgramLearnerCases;
use crate::practice::kinds::program::rubric::ProgramRubric;
use crate::practice::kinds::program::shape::{exercise_of, piece_of, result_of};
use crate::practice::kinds::program::wording::ProgramWording;
use crate::practice::session::kind::{Account, LearnerCases, Rubric, Wording};
use crate::practice::session::record::{SessionCheck, SessionLearnerCase};
use lattice_core::domain::learning::practice::work::{
    CheckReport, CheckScope, Exercise, Form, LearnerCaseVerdict, PartRef,
};
use lattice_core::shared::error::AppError;
use serde_json::{json, Map, Value};

const CODE: &str = "def total(values):\n    return 0\n";

fn exercise() -> Exercise {
    exercise_of(&problem()).unwrap()
}

fn check(id: &str, scope: CheckScope, code: &str, status: RunStatus) -> SessionCheck {
    let result = RunResult {
        case_id: "case-example-1".into(),
        status,
        actual: Some("3".into()),
        printed: String::new(),
        error: None,
        elapsed_ms: Some(1),
    };
    SessionCheck {
        id: id.into(),
        scope,
        piece: piece_of(&[LearningLabFile {
            path: "solution.py".into(),
            content: code.into(),
        }])
        .unwrap(),
        report: CheckReport {
            results: vec![result_of(&result).unwrap()],
            marks: vec![],
            detail: Value::Null,
        },
        at: 0,
    }
}

fn fields(path: &str, start: i64, end: i64) -> Map<String, Value> {
    json!({"path": path, "start": start, "end": end})
        .as_object()
        .unwrap()
        .clone()
}

fn refusal(result: Result<Vec<PartRef>, AppError>) -> String {
    match result {
        Err(AppError::InvalidInput(message)) => message,
        other => panic!("expected a refusal worded for the model, got {other:?}"),
    }
}

#[test]
fn a_program_is_scored_on_four_criteria_in_the_order_of_the_scorecard() {
    let criteria = ProgramRubric.criteria(Form::Build);

    let ids: Vec<&str> = criteria
        .iter()
        .map(|criterion| criterion.id.as_str())
        .collect();
    assert_eq!(ids, ["problem_solving", "code", "testing", "communication"]);
    let titles: Vec<&str> = criteria
        .iter()
        .map(|criterion| criterion.title.as_str())
        .collect();
    assert_eq!(
        titles,
        ["Problem solving", "Code", "Testing", "Communication"]
    );
    assert_eq!(
        criteria[0].earns,
        "Understood and clarified the problem, chose a sound approach, and reached the target complexity or explained the trade-off."
    );
    assert_eq!(criteria[1].earns, "Correct on the cases, clear, idiomatic.");
    assert_eq!(
        criteria[2].earns,
        "Examples before code, edge cases, ran the code, read the failures and fixed them."
    );
    assert_eq!(
        criteria[3].earns,
        "Explained the plan and the decisions, answered probes, and used hints well."
    );
    assert_eq!(ProgramRubric.criteria(Form::Repair), criteria);
}

#[test]
fn a_program_states_the_approach_and_complexity_of_a_strong_answer() {
    let statement = ProgramRubric.statement(Form::Build).expect("a statement");

    assert_eq!(
        statement.names,
        "the approach and complexity of a strong answer"
    );
    assert_eq!(statement.keys, ["approach", "complexity"]);
    assert_eq!(
        statement.says,
        "The approach is the strong answer's approach in a few sentences, and the complexity states its time and space cost."
    );
}

#[test]
fn the_exercise_is_told_with_its_reference_under_problem() {
    let told = ProgramAccount.exercise(&exercise()).unwrap();

    assert_eq!(ProgramAccount.exercise_key(), "problem");
    let keys: Vec<&str> = told
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "title",
            "statement",
            "function",
            "parameters",
            "returns",
            "constraints",
            "clarifications",
            "targetComplexity",
            "referenceSolution"
        ]
    );
    assert_eq!(told["title"], json!(problem().spec.title));
    assert_eq!(told["referenceSolution"], json!(problem().reference));
    assert_eq!(
        told["parameters"],
        json!([{"name": "values", "annotation": "list[int]"}])
    );
}

#[test]
fn the_form_is_told_as_the_task_it_was() {
    assert_eq!(ProgramAccount.form_key(), "task");
    assert_eq!(ProgramAccount.form(Form::Build).unwrap(), json!("problem"));
    assert_eq!(ProgramAccount.form(Form::Repair).unwrap(), json!("bug_fix"));
}

#[test]
fn a_run_is_told_with_its_failures_and_its_numbered_files() {
    let run = check("r1", CheckScope::All, CODE, RunStatus::Failed);
    let told = ProgramAccount.check(&exercise(), &run, None).unwrap();

    assert_eq!(ProgramAccount.checks_key(), "runs");
    let keys: Vec<&str> = told
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        ["runId", "scope", "passed", "total", "failing", "files"]
    );
    assert_eq!(told["runId"], json!("r1"));
    assert_eq!(told["scope"], json!("all"));
    assert_eq!(told["passed"], json!(0));
    assert_eq!(told["total"], json!(1));
    let failing = &told["failing"][0];
    assert_eq!(failing["status"], json!("failed"));
    assert_eq!(failing["actual"], json!("3"));
    assert_eq!(failing["error"], Value::Null);
    assert_eq!(
        told["files"],
        json!([{"path": "solution.py", "numbered": "1: def total(values):\n2:     return 0"}])
    );
}

#[test]
fn a_passing_case_is_not_listed_as_failing() {
    let run = check("r1", CheckScope::Shown, CODE, RunStatus::Passed);
    let told = ProgramAccount.check(&exercise(), &run, None).unwrap();

    assert_eq!(told["scope"], json!("examples"));
    assert_eq!(told["passed"], json!(1));
    assert_eq!(told["failing"], json!([]));
}

#[test]
fn a_run_of_the_same_files_as_the_one_before_says_so() {
    let first = check("r1", CheckScope::All, CODE, RunStatus::Passed);
    let second = check("r2", CheckScope::Own, CODE, RunStatus::Passed);
    let told = ProgramAccount
        .check(&exercise(), &second, Some(&first))
        .unwrap();

    assert_eq!(told["scope"], json!("input"));
    assert_eq!(told["files"], json!("the same files as run r1"));

    let other = check(
        "r3",
        CheckScope::All,
        "def total(values):\n    return 1\n",
        RunStatus::Passed,
    );
    let told = ProgramAccount
        .check(&exercise(), &other, Some(&second))
        .unwrap();
    assert_eq!(
        told["files"],
        json!([{"path": "solution.py", "numbered": "1: def total(values):\n2:     return 1"}])
    );
}

#[test]
fn a_piece_no_run_holds_is_told_as_numbered_files() {
    let run = check("r1", CheckScope::All, CODE, RunStatus::Passed);

    let told = ProgramAccount.piece(&exercise(), &run.piece).unwrap();

    assert_eq!(
        told,
        json!([{"path": "solution.py", "numbered": "1: def total(values):\n2:     return 0"}])
    );
}

#[test]
fn the_model_cites_lines_runs_and_examples() {
    let lines = ProgramAccount.cites_parts();
    assert_eq!(lines.tag, "lines");
    assert_eq!(
        lines.sentence,
        "Lines: ref is a run id, path is a file of that run, start and end are the first and last line, counted from 1 as the files are numbered below."
    );
    let run = ProgramAccount.cites_check();
    assert_eq!(run.tag, "run");
    assert_eq!(run.sentence, "A run: ref is a run id.");
    let example = ProgramLearnerCases.cited();
    assert_eq!(example.tag, "example");
    assert_eq!(
        example.sentence,
        "An example: ref is the example's ordinal."
    );
    assert_eq!(
        ProgramAccount.leave_empty(),
        "Leave the fields a kind does not use empty, or 0 for start and end."
    );
    assert_eq!(
        ProgramAccount.fields(),
        [
            ("path".to_owned(), json!({"type": "string"})),
            ("start".to_owned(), json!({"type": "integer"})),
            ("end".to_owned(), json!({"type": "integer"})),
        ]
    );
}

#[test]
fn a_cited_range_of_lines_is_one_part_for_each_line() {
    let run = check("r1", CheckScope::All, "a\nb\nc\n", RunStatus::Passed);

    let parts = ProgramAccount
        .parts(&exercise(), &run, &fields("solution.py", 2, 3))
        .unwrap();

    let named: Vec<&str> = parts.iter().map(|part| part.as_str()).collect();
    assert_eq!(named, ["solution.py:2", "solution.py:3"]);
}

#[test]
fn lines_of_a_file_the_run_does_not_have_are_refused_with_the_reason() {
    let run = check("r1", CheckScope::All, CODE, RunStatus::Passed);

    let message = refusal(ProgramAccount.parts(&exercise(), &run, &fields("other.py", 1, 1)));

    assert_eq!(message, "lines: run r1 has no file other.py.");
}

#[test]
fn lines_outside_the_file_are_refused_with_the_reason() {
    let run = check("r1", CheckScope::All, CODE, RunStatus::Passed);

    for (start, end) in [(0, 1), (2, 1), (1, 3), (-1, 1)] {
        let message =
            refusal(ProgramAccount.parts(&exercise(), &run, &fields("solution.py", start, end)));
        assert_eq!(
            message,
            format!("lines: {start} to {end} is not within the 2 lines of solution.py in run r1.")
        );
    }
}

#[test]
fn the_cases_a_learner_wrote_are_told_with_their_verdict() {
    let case = SessionLearnerCase {
        ordinal: 2,
        body: json!({"args": ["[1]", "2"], "expected": "3"}),
        verdict: LearnerCaseVerdict::Right,
    };

    let told = ProgramLearnerCases
        .account(&exercise().brief, &case)
        .unwrap();

    assert_eq!(ProgramLearnerCases.key(), "examples");
    assert_eq!(
        told,
        json!({"ordinal": 2, "args": ["[1]", "2"], "expected": "3", "verdict": "right"})
    );
    let keys: Vec<&str> = told
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["ordinal", "args", "expected", "verdict"]);
}

#[test]
fn a_program_names_its_session_and_its_checks() {
    assert_eq!(ProgramWording.session_noun(), "coding practice session");
    assert_eq!(ProgramWording.check_nouns(), "runs");
}
