#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use serde_json::json;

fn name<T: Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn build_stages_without_learner_cases_skip_examples() {
    assert_eq!(
        Form::Build.stages(false),
        vec![
            Stage::Understand,
            Stage::Plan,
            Stage::Make,
            Stage::Check,
            Stage::Review
        ]
    );
}

#[test]
fn build_stages_with_learner_cases_add_examples_after_understand() {
    assert_eq!(
        Form::Build.stages(true),
        vec![
            Stage::Understand,
            Stage::Examples,
            Stage::Plan,
            Stage::Make,
            Stage::Check,
            Stage::Review
        ]
    );
}

#[test]
fn repair_stages_ignore_learner_cases() {
    let expected = vec![Stage::Reproduce, Stage::Locate, Stage::Fix, Stage::Verify];
    assert_eq!(Form::Repair.stages(false), expected);
    assert_eq!(Form::Repair.stages(true), expected);
}

#[test]
fn complete_and_arrange_stages_ignore_learner_cases() {
    let expected = vec![Stage::Understand, Stage::Make, Stage::Check, Stage::Review];
    for form in [Form::Complete, Form::Arrange] {
        assert_eq!(form.stages(false), expected);
        assert_eq!(form.stages(true), expected);
    }
}

#[test]
fn only_repair_needs_a_misreading_of_its_own() {
    assert_eq!(Form::Repair.misreadings_needed(), 1);
    assert_eq!(Form::Build.misreadings_needed(), 0);
    assert_eq!(Form::Complete.misreadings_needed(), 0);
    assert_eq!(Form::Arrange.misreadings_needed(), 0);
}

#[test]
fn discloses_follows_the_table_cell_by_cell() {
    let cells = [
        (Setting::Coached, Hidden::FailingCase, false, true),
        (Setting::Coached, Hidden::FailingCase, true, true),
        (Setting::Coached, Hidden::UncaughtMisreading, false, true),
        (Setting::Coached, Hidden::UncaughtMisreading, true, true),
        (Setting::Coached, Hidden::WorkedSolution, false, false),
        (Setting::Coached, Hidden::WorkedSolution, true, true),
        (Setting::Mock, Hidden::FailingCase, false, false),
        (Setting::Mock, Hidden::FailingCase, true, false),
        (Setting::Mock, Hidden::UncaughtMisreading, false, false),
        (Setting::Mock, Hidden::UncaughtMisreading, true, false),
        (Setting::Mock, Hidden::WorkedSolution, false, false),
        (Setting::Mock, Hidden::WorkedSolution, true, false),
        (Setting::Watched, Hidden::FailingCase, false, false),
        (Setting::Watched, Hidden::FailingCase, true, false),
        (Setting::Watched, Hidden::UncaughtMisreading, false, false),
        (Setting::Watched, Hidden::UncaughtMisreading, true, false),
        (Setting::Watched, Hidden::WorkedSolution, false, false),
        (Setting::Watched, Hidden::WorkedSolution, true, false),
    ];
    for (setting, hidden, submitted, shown) in cells {
        assert_eq!(
            setting.discloses(hidden, submitted),
            shown,
            "{setting:?} {hidden:?} submitted={submitted}"
        );
    }
}

#[test]
fn the_rung_after_each_highest_given() {
    assert_eq!(Rung::after(None), Rung::Nudge);
    assert_eq!(Rung::after(Some(Rung::Nudge)), Rung::Idea);
    assert_eq!(Rung::after(Some(Rung::Idea)), Rung::Plan);
    assert_eq!(Rung::after(Some(Rung::Plan)), Rung::Point);
    assert_eq!(Rung::after(Some(Rung::Point)), Rung::Point);
}

#[test]
fn rungs_order_from_nudge_to_point() {
    assert!(Rung::Nudge < Rung::Idea && Rung::Idea < Rung::Plan && Rung::Plan < Rung::Point);
}

fn report(statuses: &[CaseStatus]) -> CheckReport {
    CheckReport {
        results: statuses
            .iter()
            .enumerate()
            .map(|(index, status)| CaseResult {
                case_id: format!("c{index}"),
                status: *status,
                detail: serde_json::Value::Null,
            })
            .collect(),
        marks: Vec::new(),
        detail: serde_json::Value::Null,
    }
}

#[test]
fn a_report_with_no_results_has_not_passed() {
    assert!(!report(&[]).all_passed());
}

#[test]
fn a_report_passes_when_every_result_passed() {
    assert!(report(&[CaseStatus::Passed, CaseStatus::Passed]).all_passed());
}

#[test]
fn one_result_that_did_not_pass_fails_the_report() {
    for other in [
        CaseStatus::Failed,
        CaseStatus::Error,
        CaseStatus::Ran,
        CaseStatus::TimedOut,
        CaseStatus::NotRun,
    ] {
        assert!(
            !report(&[CaseStatus::Passed, other]).all_passed(),
            "{other:?}"
        );
    }
}

#[test]
fn stored_names_of_forms() {
    let names: Vec<String> = [Form::Build, Form::Repair, Form::Complete, Form::Arrange]
        .into_iter()
        .map(name)
        .collect();
    assert_eq!(names, ["build", "repair", "complete", "arrange"]);
}

#[test]
fn stored_names_of_settings() {
    let names: Vec<String> = [Setting::Coached, Setting::Mock, Setting::Watched]
        .into_iter()
        .map(name)
        .collect();
    assert_eq!(names, ["coached", "mock", "watched"]);
}

#[test]
fn stored_names_of_stages() {
    let names: Vec<String> = [
        Stage::Understand,
        Stage::Examples,
        Stage::Plan,
        Stage::Make,
        Stage::Check,
        Stage::Review,
        Stage::Reproduce,
        Stage::Locate,
        Stage::Fix,
        Stage::Verify,
    ]
    .into_iter()
    .map(name)
    .collect();
    assert_eq!(
        names,
        [
            "understand",
            "examples",
            "plan",
            "make",
            "check",
            "review",
            "reproduce",
            "locate",
            "fix",
            "verify"
        ]
    );
}

#[test]
fn stored_names_of_rungs() {
    let names: Vec<String> = [Rung::Nudge, Rung::Idea, Rung::Plan, Rung::Point]
        .into_iter()
        .map(name)
        .collect();
    assert_eq!(names, ["nudge", "idea", "plan", "point"]);
}

#[test]
fn stored_names_of_scopes() {
    assert_eq!(name(CheckScope::Shown), "shown");
    assert_eq!(name(CheckScope::All), "all");
    assert_eq!(name(CheckScope::Own), "own");
}

#[test]
fn stored_names_of_check_statuses() {
    let names: Vec<String> = [
        CheckStatus::Pending,
        CheckStatus::Running,
        CheckStatus::Passed,
        CheckStatus::Failed,
        CheckStatus::TimedOut,
        CheckStatus::Cancelled,
        CheckStatus::Interrupted,
        CheckStatus::Unavailable,
    ]
    .into_iter()
    .map(name)
    .collect();
    assert_eq!(
        names,
        [
            "pending",
            "running",
            "passed",
            "failed",
            "timed_out",
            "cancelled",
            "interrupted",
            "unavailable"
        ]
    );
}

#[test]
fn stored_names_of_learner_case_verdicts() {
    let names: Vec<String> = [
        LearnerCaseVerdict::Right,
        LearnerCaseVerdict::Wrong,
        LearnerCaseVerdict::Invalid,
    ]
    .into_iter()
    .map(name)
    .collect();
    assert_eq!(names, ["right", "wrong", "invalid"]);
}

#[test]
fn a_report_round_trips_with_its_detail() {
    let mut with_detail = report(&[CaseStatus::Passed]);
    with_detail.detail = json!({"note": "x"});
    let text = serde_json::to_string(&with_detail).unwrap();
    let back: CheckReport = serde_json::from_str(&text).unwrap();
    assert_eq!(back, with_detail);
}

#[test]
fn stored_names_of_case_statuses() {
    let names: Vec<String> = [
        CaseStatus::Passed,
        CaseStatus::Failed,
        CaseStatus::Error,
        CaseStatus::Ran,
        CaseStatus::TimedOut,
        CaseStatus::NotRun,
    ]
    .into_iter()
    .map(name)
    .collect();
    assert_eq!(
        names,
        ["passed", "failed", "error", "ran", "timed_out", "not_run"]
    );
}

#[test]
fn ids_and_pieces_serialise_as_their_content() {
    assert_eq!(
        serde_json::to_value(KindId::new("letters")).unwrap(),
        json!("letters")
    );
    assert_eq!(serde_json::to_value(PartRef::new("2")).unwrap(), json!("2"));
    assert_eq!(
        serde_json::to_value(Piece::new(json!(["a", "b"]))).unwrap(),
        json!(["a", "b"])
    );
    let back: KindId = serde_json::from_value(json!("letters")).unwrap();
    assert_eq!(back.as_str(), "letters");
}

#[test]
fn an_exercise_round_trips_through_json() {
    let exercise = Exercise {
        kind: KindId::new("letters"),
        form: Form::Repair,
        brief: Brief {
            title: "Spell it".to_owned(),
            difficulty: Difficulty::Medium,
            spec: json!({"word": "abc"}),
        },
        reference: Piece::new(json!(["a", "b", "c"])),
        misreadings: vec![Misreading {
            id: "m1".to_owned(),
            description: "Swaps the last two.".to_owned(),
            piece: Piece::new(json!(["a", "c", "b"])),
            killed_by: vec!["c1".to_owned()],
        }],
        cases: vec![Case {
            id: "c1".to_owned(),
            title: "Second letter".to_owned(),
            visibility: CaseVisibility::Hidden,
            body: json!({"index": 1, "expected": "b"}),
        }],
        start: Piece::new(json!(["a", "c", "b"])),
        planted: Some("m1".to_owned()),
        proof: json!(null),
    };
    let text = serde_json::to_string(&exercise).unwrap();
    assert!(text.contains("\"killedBy\""));
    let back: Exercise = serde_json::from_str(&text).unwrap();
    assert_eq!(back, exercise);
}
