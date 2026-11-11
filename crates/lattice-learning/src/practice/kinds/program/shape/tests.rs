#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::practice::coding::problem::{CaseResult as RunResult, CaseStatus as RunStatus};
use crate::practice::coding::proof::fakes::{sample_problem, spec};
use lattice_core::domain::learning::practice::work::{CaseStatus, CaseVisibility};
use serde_json::json;

fn files() -> Vec<LearningLabFile> {
    vec![
        LearningLabFile {
            path: "solution.py".into(),
            content: "a\nb\n".into(),
        },
        LearningLabFile {
            path: "other.py".into(),
            content: String::new(),
        },
    ]
}

fn repair_problem() -> Problem {
    let mut problem = sample_problem();
    problem.bug = Some("misreading-2".into());
    problem
}

fn run_result(status: RunStatus) -> RunResult {
    RunResult {
        case_id: "example-1".into(),
        status,
        actual: Some("'x'".into()),
        printed: "out".into(),
        error: Some("boom".into()),
        elapsed_ms: Some(4),
    }
}

#[test]
fn a_piece_is_the_file_list_as_serde_writes_it() {
    let piece = piece_of(&files()).unwrap();
    assert_eq!(piece.value(), &serde_json::to_value(files()).unwrap());
    assert_eq!(files_of(&piece).unwrap(), files());
}

#[test]
fn a_solution_piece_round_trips() {
    let piece = solution_piece("code\n").unwrap();
    assert_eq!(solution_of(&piece).unwrap(), "code\n");
    assert_eq!(files_of(&piece).unwrap().len(), 1);
}

#[test]
fn a_piece_without_the_solution_file_is_invalid() {
    let piece = piece_of(&files()[1..]).unwrap();
    assert!(matches!(
        solution_of(&piece),
        Err(AppError::InvalidState(_))
    ));
}

#[test]
fn a_piece_that_is_not_a_file_list_is_invalid() {
    let piece = Piece::new(serde_json::json!({"path": "solution.py"}));
    assert!(matches!(files_of(&piece), Err(AppError::InvalidState(_))));
    assert!(matches!(
        solution_of(&piece),
        Err(AppError::InvalidState(_))
    ));
}

#[test]
fn a_brief_round_trips_to_the_spec() {
    let brief = brief_of(&spec()).unwrap();
    assert_eq!(brief.title, spec().title);
    assert_eq!(brief.difficulty, work::Difficulty::Easy);
    assert_eq!(spec_of(&brief).unwrap(), spec());
}

#[test]
fn a_brief_holds_its_title_and_difficulty_once() {
    let brief = brief_of(&spec()).unwrap();
    let stored = brief.spec.as_object().unwrap();
    assert!(!stored.contains_key("title"));
    assert!(!stored.contains_key("difficulty"));
    assert!(stored.contains_key("statement"));
}

#[test]
fn every_difficulty_survives_a_brief() {
    for difficulty in [
        coding::ProblemDifficulty::Easy,
        coding::ProblemDifficulty::Medium,
        coding::ProblemDifficulty::Hard,
    ] {
        let mut changed = spec();
        changed.difficulty = difficulty;
        assert_eq!(spec_of(&brief_of(&changed).unwrap()).unwrap(), changed);
    }
}

#[test]
fn the_spec_takes_the_title_the_brief_holds() {
    let mut brief = brief_of(&spec()).unwrap();
    brief.title = "Another title".into();
    brief.difficulty = work::Difficulty::Hard;
    let read = spec_of(&brief).unwrap();
    assert_eq!(read.title, "Another title");
    assert_eq!(read.difficulty, coding::ProblemDifficulty::Hard);
}

#[test]
fn a_brief_that_is_not_a_spec_is_invalid() {
    let mut brief = brief_of(&spec()).unwrap();
    brief.spec = serde_json::json!([1, 2]);
    assert!(matches!(spec_of(&brief), Err(AppError::InvalidState(_))));
}

#[test]
fn a_case_round_trips_and_stores_nothing_twice() {
    for case in sample_problem().cases {
        let converted = case_of(&case).unwrap();
        assert_eq!(converted.id, case.id);
        assert_eq!(converted.title, case.category);
        let mut keys: Vec<&str> = converted
            .body
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["args", "compare", "digest", "expected", "scale"]);
        assert_eq!(problem_case_of(&converted).unwrap(), case);
    }
}

#[test]
fn case_visibility_maps_by_meaning() {
    let cases = sample_problem().cases;
    assert_eq!(
        case_of(&cases[0]).unwrap().visibility,
        CaseVisibility::Shown
    );
    assert_eq!(
        case_of(&cases[2]).unwrap().visibility,
        CaseVisibility::Hidden
    );
}

#[test]
fn a_case_body_that_is_not_a_program_case_is_invalid() {
    let mut case = case_of(&sample_problem().cases[0]).unwrap();
    case.body = serde_json::json!({"args": 3});
    assert!(matches!(
        problem_case_of(&case),
        Err(AppError::InvalidState(_))
    ));
}

#[test]
fn a_result_round_trips_in_every_status() {
    for status in [
        RunStatus::Passed,
        RunStatus::Failed,
        RunStatus::Error,
        RunStatus::Ran,
        RunStatus::TimedOut,
        RunStatus::NotRun,
    ] {
        let result = run_result(status);
        assert_eq!(run_result_of(&result_of(&result).unwrap()).unwrap(), result);
    }
    let passed = result_of(&run_result(RunStatus::Passed)).unwrap();
    assert_eq!(passed.status, CaseStatus::Passed);
    assert_eq!(passed.detail["elapsedMs"], 4);
    assert_eq!(passed.detail["actual"], "'x'");
}

#[test]
fn a_result_detail_that_is_not_a_programs_is_invalid() {
    let mut result = result_of(&run_result(RunStatus::Passed)).unwrap();
    result.detail = serde_json::json!("nothing");
    assert!(matches!(
        run_result_of(&result),
        Err(AppError::InvalidState(_))
    ));
}

#[test]
fn a_misreading_round_trips_as_a_solution_piece() {
    for misreading in sample_problem().misreadings {
        let converted = misreading_of(&misreading).unwrap();
        assert_eq!(solution_of(&converted.piece).unwrap(), misreading.code);
        assert_eq!(problem_misreading_of(&converted).unwrap(), misreading);
    }
}

#[test]
fn a_run_detail_round_trips_what_was_printed() {
    let detail = run_detail_of("out", "err").unwrap();
    assert_eq!(
        detail,
        serde_json::json!({"stdout": "out", "stderr": "err"})
    );
    assert_eq!(
        run_output_of(&detail).unwrap(),
        ("out".to_owned(), "err".to_owned())
    );
}

#[test]
fn a_null_detail_prints_nothing_and_a_foreign_one_is_invalid() {
    assert_eq!(
        run_output_of(&serde_json::Value::Null).unwrap(),
        (String::new(), String::new())
    );
    assert_eq!(
        run_output_of(&serde_json::json!({"stdout": "only"})).unwrap(),
        ("only".to_owned(), String::new())
    );
    assert!(matches!(
        run_output_of(&serde_json::json!(["not", "an", "object"])),
        Err(AppError::InvalidState(_))
    ));
}

#[test]
fn a_build_problem_round_trips_through_its_exercise() {
    let problem = sample_problem();
    let exercise = exercise_of(&problem).unwrap();
    assert_eq!(exercise.kind.as_str(), KIND);
    assert_eq!(exercise.form, Form::Build);
    assert_eq!(exercise.planted, None);
    assert_eq!(solution_of(&exercise.start).unwrap(), problem.spec.starter);
    assert_eq!(solution_of(&exercise.reference).unwrap(), problem.reference);
    assert_eq!(
        exercise.proof,
        serde_json::json!({"referenceMs": {"scale-1": 12}})
    );
    assert_eq!(problem_of(&exercise).unwrap(), problem);
}

#[test]
fn a_repair_problem_round_trips_through_its_exercise() {
    let problem = repair_problem();
    let exercise = exercise_of(&problem).unwrap();
    assert_eq!(exercise.form, Form::Repair);
    assert_eq!(exercise.planted.as_deref(), Some("misreading-2"));
    assert_eq!(
        solution_of(&exercise.start).unwrap(),
        problem.misreadings[1].code
    );
    assert_eq!(problem_of(&exercise).unwrap(), problem);
}

#[test]
fn a_bug_that_names_no_misreading_is_invalid() {
    let mut problem = sample_problem();
    problem.bug = Some("missing".into());
    assert!(matches!(
        exercise_of(&problem),
        Err(AppError::InvalidState(_))
    ));
}

#[test]
fn an_exercise_of_another_kind_is_invalid() {
    let mut exercise = exercise_of(&sample_problem()).unwrap();
    exercise.kind = KindId::new("letters");
    assert!(matches!(
        problem_of(&exercise),
        Err(AppError::InvalidState(_))
    ));
}

#[test]
fn every_cue_survives_the_coding_cue_and_back() {
    let cues = [
        Cue::Opening,
        Cue::Reply,
        Cue::StageOpened(Stage::Plan),
        Cue::StageOpened(Stage::Make),
        Cue::AskForSkipped(vec![Stage::Examples, Stage::Plan]),
        Cue::AskForSkipped(Vec::new()),
        Cue::CheckReview(CheckScope::Shown),
        Cue::CheckReview(CheckScope::All),
        Cue::CheckReview(CheckScope::Own),
        Cue::Submitted,
        Cue::Checkpoint,
        Cue::Idle,
    ];
    for cue in cues {
        assert_eq!(cue_of(&coding_cue_of(&cue)), cue, "{cue:?}");
    }
}

#[test]
fn a_cue_is_stored_under_the_name_the_coding_tables_know() {
    let stored = |cue: Cue| serde_json::to_value(coding_cue_of(&cue)).unwrap();
    assert_eq!(
        stored(Cue::CheckReview(CheckScope::All)),
        json!({"run_review": "all"})
    );
    assert_eq!(
        stored(Cue::StageOpened(Stage::Make)),
        json!({"stage_opened": "code"})
    );
    assert_eq!(stored(Cue::Submitted), json!("submitted"));
}

#[test]
fn a_cue_ranks_as_the_coding_cue_it_is_stored_as() {
    let cues = [
        Cue::Opening,
        Cue::Reply,
        Cue::Submitted,
        Cue::AskForSkipped(Vec::new()),
        Cue::StageOpened(Stage::Plan),
        Cue::CheckReview(CheckScope::All),
        Cue::Checkpoint,
        Cue::Idle,
    ];
    for first in &cues {
        for second in &cues {
            assert_eq!(
                first.outranks(second),
                coding_cue_of(first).outranks(&coding_cue_of(second)),
                "{first:?} over {second:?}"
            );
        }
    }
}

#[test]
fn a_run_stands_as_the_check_it_is() {
    use crate::practice::practical_dto::LearningPracticalRunStatus as Run;
    let pairs = [
        (Run::Pending, CheckStatus::Pending),
        (Run::Running, CheckStatus::Running),
        (Run::Passed, CheckStatus::Passed),
        (Run::Failed, CheckStatus::Failed),
        (Run::TimedOut, CheckStatus::TimedOut),
        (Run::Cancelled, CheckStatus::Cancelled),
        (Run::Interrupted, CheckStatus::Interrupted),
        (Run::RuntimeUnavailable, CheckStatus::Unavailable),
    ];
    for (run, check) in pairs {
        assert_eq!(check_status_of(run), check);
    }
}
