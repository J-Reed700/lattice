#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::lab_runtime::LearningLabFile;
use crate::practical_dto::LearningPracticalRunStatus;
use crate::practice::coding::observation::test_support::{
    case, problem, HIDDEN_ARGS, MISREADING_CODE, MISREADING_DESCRIPTION, REFERENCE,
};
use crate::practice::coding::problem::{
    CaseResult as RunResult, CaseStatus as RunStatus, CaseVisibility as ProblemVisibility,
    SOLUTION_PATH,
};
use crate::practice::kinds::program::dto::CodingRunScope;
use crate::practice::kinds::program::parts::part_of;
use crate::practice::kinds::program::shape::{
    case_of, exercise_of, learner_found_of, piece_of, result_of, run_detail_of,
};
use crate::practice::session::record::{CheckedCase, SessionRow};
use lattice_core::domain::learning::practice::work::{
    CheckReport, CheckStatus, Form, LearnerCaseVerdict, PartRef, Rung, Setting, Stage,
};
use serde_json::json;

const NOTHING: Seen = Seen {
    failing_case: false,
    uncaught_misreading: false,
    reference: false,
};

const EVERYTHING: Seen = Seen {
    failing_case: true,
    uncaught_misreading: true,
    reference: true,
};

fn files(code: &str) -> Vec<LearningLabFile> {
    vec![LearningLabFile {
        path: SOLUTION_PATH.to_owned(),
        content: code.to_owned(),
    }]
}

fn row() -> SessionRow {
    SessionRow {
        id: "session-1".to_owned(),
        program_id: "program-1".to_owned(),
        activity_id: "activity-1".to_owned(),
        form: Form::Build,
        setting: Setting::Mock,
        stage: Stage::Make,
        budget_seconds: None,
        active_ms: 0,
        opened_at: None,
        draft: piece_of(&files("def total(values):\n    return sum(values)\n")).unwrap(),
        pending_cue: None,
        submitted_at: None,
        solution_seen_at: None,
        finished_at: None,
        revision: 1,
        created_at: 0,
    }
}

fn session() -> Session {
    Session {
        row: row(),
        activity_revision: 1,
        exercise: exercise_of(&problem()).unwrap(),
        settings: vec![Setting::Mock],
        turns: Vec::new(),
        checks: Vec::new(),
        learner_cases: Vec::new(),
        hints: Vec::new(),
        events: Vec::new(),
        draft_seen: None,
        lesson_id: "lesson-1".to_owned(),
        debrief: None,
    }
}

fn outcome(case_id: &str, status: RunStatus) -> RunResult {
    RunResult {
        case_id: case_id.to_owned(),
        status,
        actual: Some("99".to_owned()),
        printed: "secret print".to_owned(),
        error: None,
        elapsed_ms: Some(1),
    }
}

/// A check of the problem's two cases, `extra` ones the learner wrote after them: the hidden case fails.
fn check(id: &str, scope: CheckScope, extra: Vec<(ProblemCase, bool)>) -> SessionCheck {
    let mut cases = vec![
        (
            case("case-example-1", ProblemVisibility::Example, "[1, 2]"),
            false,
        ),
        (
            case("case-hidden-1", ProblemVisibility::Hidden, HIDDEN_ARGS),
            false,
        ),
    ];
    cases.extend(extra);
    SessionCheck {
        id: id.to_owned(),
        scope,
        piece: piece_of(&files("def total(values):\n    return 99\n")).unwrap(),
        report: CheckReport {
            results: cases
                .iter()
                .map(|(each, _)| {
                    let status = if each.id == "case-hidden-1" {
                        RunStatus::Failed
                    } else {
                        RunStatus::Passed
                    };
                    result_of(&outcome(&each.id, status)).unwrap()
                })
                .collect(),
            marks: Vec::new(),
            detail: run_detail_of("out", "err", Some(12), Some("3")).unwrap(),
        },
        at: 500,
        status: CheckStatus::Failed,
        cases: cases
            .into_iter()
            .map(|(each, own)| CheckedCase {
                case: case_of(&each).unwrap(),
                own,
            })
            .collect(),
    }
}

fn example(
    ordinal: u32,
    verdict: LearnerCaseVerdict,
    found: serde_json::Value,
) -> SessionLearnerCase {
    SessionLearnerCase {
        ordinal,
        body: json!({"args": ["[3]"], "expected": "4"}),
        verdict,
        catches: Vec::new(),
        found,
    }
}

fn hint(ordinal: u32, parts: Vec<PartRef>) -> SessionHint {
    SessionHint {
        ordinal,
        rung: Rung::Point,
        text: "Look here.".to_owned(),
        stage: Stage::Make,
        check_id: None,
        reason: "Why.".to_owned(),
        parts,
        in_role: false,
        at: 0,
    }
}

fn work_of(session: &Session, seen: &Seen) -> ProgramWorkDto {
    serde_json::from_value(ProgramScreen.work(session, seen).unwrap()).unwrap()
}

fn debrief() -> SessionDebrief {
    SessionDebrief {
        scorecard: lattice_core::domain::learning::practice::Scorecard { rows: Vec::new() },
        stated: vec![
            (APPROACH.to_owned(), "Add them up.".to_owned()),
            (COMPLEXITY.to_owned(), "O(n) time".to_owned()),
        ],
        model: "fake-model".to_owned(),
        techniques: Vec::new(),
    }
}

fn debrief_of(session: &Session) -> ProgramDebriefWorkDto {
    serde_json::from_value(ProgramScreen.debrief(session, &debrief()).unwrap()).unwrap()
}

fn case_in<'a>(cases: &'a [CaseViewDto], id: &str) -> &'a CaseViewDto {
    cases.iter().find(|each| each.case_id == id).unwrap()
}

#[test]
fn the_work_holds_the_problem_without_its_hidden_cases_and_the_draft() {
    let held = work_of(&session(), &NOTHING);
    assert_eq!(held.problem.title, "Total of values");
    assert_eq!(held.problem.starter, "def total(values):\n    pass\n");
    assert_eq!(
        held.problem
            .examples
            .iter()
            .map(|each| each.case_id.as_str())
            .collect::<Vec<_>>(),
        ["case-example-1"]
    );
    assert_eq!(
        held.problem.examples[0].args,
        Some(vec!["[1, 2]".to_owned()])
    );
    assert_eq!(held.problem.examples[0].status, RunStatus::NotRun);
    assert_eq!(
        held.draft,
        files("def total(values):\n    return sum(values)\n")
    );
    assert!(held.runs.is_empty());
    assert!(held.hint_marks.is_empty());
    assert_eq!(held.worked_solution, None);
}

#[test]
fn nothing_a_setting_keeps_back_is_in_the_work_the_screen_is_sent() {
    let mut held = session();
    held.checks = vec![check("check-a", CheckScope::All, Vec::new())];
    held.learner_cases = vec![example(
        1,
        LearnerCaseVerdict::Wrong,
        learner_found_of(Some("EXPECTED_SECRET"), None),
    )];
    let sent = ProgramScreen.work(&held, &NOTHING).unwrap().to_string();
    for secret in [
        REFERENCE,
        HIDDEN_ARGS,
        MISREADING_CODE,
        MISREADING_DESCRIPTION,
        "EXPECTED_SECRET",
    ] {
        let escaped = serde_json::to_string(secret).unwrap();
        assert!(
            !sent.contains(&escaped[1..escaped.len() - 1]),
            "{secret} reached the screen"
        );
    }
}

#[test]
fn a_run_is_listed_with_what_it_printed_and_how_long_it_took_and_its_id_is_the_checks() {
    let mut held = session();
    held.checks = vec![check("check-a", CheckScope::Shown, Vec::new())];
    let run = &work_of(&held, &NOTHING).runs[0];
    assert_eq!(run.run_id, "check-a");
    assert_eq!(run.scope, CodingRunScope::Shown);
    assert_eq!(run.status, LearningPracticalRunStatus::Failed);
    assert_eq!(run.stdout, "out");
    assert_eq!(run.stderr, "err");
    assert_eq!(run.duration_ms, Some(12));
    assert_eq!(run.created_at, 500);
    let order: Vec<&str> = run.cases.iter().map(|each| each.case_id.as_str()).collect();
    assert_eq!(order, ["case-example-1", "case-hidden-1"]);
}

#[test]
fn a_hidden_case_the_setting_keeps_back_shows_its_verdict_and_nothing_inside_it() {
    let mut held = session();
    held.checks = vec![check("check-a", CheckScope::All, Vec::new())];
    let run = &work_of(&held, &NOTHING).runs[0];
    let hidden = case_in(&run.cases, "case-hidden-1");
    assert_eq!(hidden.status, RunStatus::Failed);
    assert_eq!(hidden.category, "an ordinary list");
    assert_eq!(hidden.args, None);
    assert_eq!(hidden.expected, None);
    assert_eq!(hidden.actual, None);
    assert_eq!(hidden.printed, None);
    assert_eq!(hidden.error, None);
    assert_eq!(hidden.elapsed_ms, Some(1), "how long it took is no secret");
    let shown = case_in(&run.cases, "case-example-1");
    assert_eq!(shown.args, Some(vec!["[1, 2]".to_owned()]));
    assert_eq!(shown.expected.as_deref(), Some("0"));
    assert_eq!(shown.actual.as_deref(), Some("99"));
    assert_eq!(shown.printed.as_deref(), Some("secret print"));
}

#[test]
fn a_failing_hidden_case_is_shown_in_full_when_the_setting_discloses_it() {
    let mut held = session();
    held.checks = vec![check("check-a", CheckScope::All, Vec::new())];
    let seen = Seen {
        failing_case: true,
        ..NOTHING
    };
    let run = &work_of(&held, &seen).runs[0];
    let hidden = case_in(&run.cases, "case-hidden-1");
    assert_eq!(hidden.args, Some(vec![HIDDEN_ARGS.to_owned()]));
    assert_eq!(hidden.expected.as_deref(), Some("0"));
    assert_eq!(hidden.actual.as_deref(), Some("99"));
    assert_eq!(hidden.printed.as_deref(), Some("secret print"));
}

#[test]
fn a_case_the_learner_wrote_is_open_even_where_a_hidden_one_is_not() {
    let mut held = session();
    let mut mine = case("mine-1", ProblemVisibility::Hidden, "[7]");
    mine.category = "Your example 1".to_owned();
    held.checks = vec![check("check-a", CheckScope::Shown, vec![(mine, true)])];
    let run = &work_of(&held, &NOTHING).runs[0];
    let own = case_in(&run.cases, "mine-1");
    assert!(own.mine);
    assert_eq!(own.args, Some(vec!["[7]".to_owned()]));
    assert_eq!(own.expected.as_deref(), Some("0"));
    assert!(!case_in(&run.cases, "case-hidden-1").mine);
}

#[test]
fn a_scale_case_never_shows_its_arguments_or_its_output() {
    let mut held = session();
    let mut scale = case("case-scale", ProblemVisibility::Example, "[BIG]");
    scale.scale = true;
    held.checks = vec![check("check-a", CheckScope::All, vec![(scale, false)])];
    let run = &work_of(&held, &EVERYTHING).runs[0];
    let big = case_in(&run.cases, "case-scale");
    assert!(big.scale);
    assert_eq!(big.args, None);
    assert_eq!(big.actual, None);
    assert_eq!(big.expected.as_deref(), Some("0"));
}

#[test]
fn what_the_reference_answered_an_input_is_shown_only_when_the_failing_case_is() {
    let mut held = session();
    held.checks = vec![check("check-a", CheckScope::Own, Vec::new())];
    let hidden = work_of(&held, &NOTHING);
    assert_eq!(hidden.runs[0].reference_actual, None);
    assert_eq!(hidden.runs[0].scope, CodingRunScope::Input);
    let seen = Seen {
        failing_case: true,
        ..NOTHING
    };
    assert_eq!(
        work_of(&held, &seen).runs[0].reference_actual.as_deref(),
        Some("3")
    );
}

#[test]
fn a_check_with_no_detail_has_no_output_no_duration_and_no_reference_answer() {
    let mut held = session();
    let mut bare = check("check-a", CheckScope::All, Vec::new());
    bare.report.detail = serde_json::Value::Null;
    held.checks = vec![bare];
    let run = &work_of(&held, &EVERYTHING).runs[0];
    assert_eq!((run.stdout.as_str(), run.stderr.as_str()), ("", ""));
    assert_eq!(run.duration_ms, None);
    assert_eq!(run.reference_actual, None);
}

#[test]
fn what_a_wrong_example_expects_is_shown_only_for_a_wrong_example_when_the_failing_case_is() {
    let mut held = session();
    held.learner_cases = vec![
        example(
            1,
            LearnerCaseVerdict::Wrong,
            learner_found_of(Some("4"), None),
        ),
        example(
            2,
            LearnerCaseVerdict::Right,
            learner_found_of(Some("4"), None),
        ),
        example(
            3,
            LearnerCaseVerdict::Invalid,
            learner_found_of(None, Some("That is not a literal.")),
        ),
    ];
    let kept_back = work_of(&held, &NOTHING).examples;
    assert!(kept_back.iter().all(|each| each.expects.is_none()));

    let seen = Seen {
        failing_case: true,
        ..NOTHING
    };
    let shown = work_of(&held, &seen).examples;
    assert_eq!(shown[0].verdict, ExampleVerdict::Wrong);
    assert_eq!(shown[0].expects.as_deref(), Some("4"));
    assert_eq!(shown[1].verdict, ExampleVerdict::Right);
    assert_eq!(
        shown[1].expects, None,
        "a right example expects nothing more"
    );
    assert_eq!(shown[2].verdict, ExampleVerdict::Invalid);
    assert_eq!(shown[2].expects, None);
    assert_eq!(
        shown[2].why_invalid.as_deref(),
        Some("That is not a literal."),
        "the reason an example is invalid is never kept back"
    );
    assert_eq!(shown[0].args, ["[3]"]);
    assert_eq!(shown[0].expected, "4");
}

#[test]
fn an_example_stored_without_what_the_reference_found_has_neither() {
    let mut held = session();
    held.learner_cases = vec![example(
        1,
        LearnerCaseVerdict::Wrong,
        serde_json::Value::Null,
    )];
    let shown = work_of(&held, &EVERYTHING).examples;
    assert_eq!(shown[0].expects, None);
    assert_eq!(shown[0].why_invalid, None);
}

#[test]
fn the_worked_solution_is_in_the_work_only_when_the_reference_is_seen() {
    let held = session();
    assert_eq!(work_of(&held, &NOTHING).worked_solution, None);
    let seen = Seen {
        reference: true,
        ..NOTHING
    };
    assert_eq!(
        work_of(&held, &seen).worked_solution.as_deref(),
        Some(REFERENCE)
    );
}

#[test]
fn a_hint_that_points_at_lines_has_a_mark_with_its_file_and_lines_and_one_that_does_not_has_none() {
    let mut held = session();
    held.hints = vec![
        hint(1, Vec::new()),
        hint(
            2,
            vec![part_of(SOLUTION_PATH, 2), part_of(SOLUTION_PATH, 3)],
        ),
        hint(3, vec![part_of("helper.py", 9)]),
    ];
    assert_eq!(
        work_of(&held, &NOTHING).hint_marks,
        [
            ProgramHintMarkDto {
                ordinal: 2,
                path: Some(SOLUTION_PATH.to_owned()),
                lines: vec![2, 3],
            },
            ProgramHintMarkDto {
                ordinal: 3,
                path: Some("helper.py".to_owned()),
                lines: vec![9],
            },
        ]
    );
}

#[test]
fn a_hint_part_that_names_no_line_is_refused() {
    let mut held = session();
    held.hints = vec![hint(1, vec![PartRef::new("no-line-here")])];
    assert!(ProgramScreen.work(&held, &NOTHING).is_err());
}

#[test]
fn the_debrief_lists_every_case_of_the_last_submit_with_nothing_kept_back() {
    let mut held = session();
    held.checks = vec![
        check("check-a", CheckScope::All, Vec::new()),
        check("check-b", CheckScope::Shown, Vec::new()),
    ];
    let work = debrief_of(&held);
    let cases: Vec<&str> = work
        .cases
        .iter()
        .map(|each| each.case_id.as_str())
        .collect();
    assert_eq!(cases, ["case-example-1", "case-hidden-1"]);
    let hidden = case_in(&work.cases, "case-hidden-1");
    assert_eq!(hidden.args, Some(vec![HIDDEN_ARGS.to_owned()]));
    assert_eq!(hidden.expected.as_deref(), Some("0"));
    assert_eq!(hidden.actual.as_deref(), Some("99"));
    assert_eq!(hidden.printed.as_deref(), Some("secret print"));
}

#[test]
fn the_debrief_of_a_session_never_submitted_lists_no_case() {
    let mut held = session();
    held.checks = vec![check("check-a", CheckScope::Shown, Vec::new())];
    assert!(debrief_of(&held).cases.is_empty());
}

#[test]
fn the_debrief_states_the_approach_the_complexity_and_the_reference_of_a_strong_answer() {
    let strong = debrief_of(&session()).strong_answer;
    assert_eq!(strong.approach, "Add them up.");
    assert_eq!(strong.complexity, "O(n) time");
    assert_eq!(strong.reference, REFERENCE);
}

#[test]
fn a_debrief_that_states_no_approach_is_refused() {
    let mut stated = debrief();
    stated.stated.retain(|(key, _)| key != APPROACH);
    let refused = ProgramScreen.debrief(&session(), &stated);
    assert!(
        matches!(refused, Err(AppError::InvalidState(_))),
        "{refused:?}"
    );
}

#[test]
fn a_session_whose_exercise_is_not_a_program_is_refused() {
    let mut held = session();
    held.exercise.brief.spec = serde_json::Value::Null;
    assert!(ProgramScreen.work(&held, &NOTHING).is_err());
}
