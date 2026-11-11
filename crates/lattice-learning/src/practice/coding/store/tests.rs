//! What the engine reads of a session is what the coding tables hold, without a database.

use super::*;
use crate::lab_runtime::LearningLabFile;
use crate::practical_dto::{CaseCheckDetailDto, LearningPracticalRunStatus};
use crate::practice::coding::dto::{CodingRunScope, ExampleVerdict};
use crate::practice::coding::observation::test_support::problem;
use crate::practice::coding::problem::{CaseStatus, CaseVisibility};
use crate::practice::coding::records::{
    CodingRow, SessionBundle, StoredEvent, StoredExample, StoredRun,
};
use lattice_core::domain::learning::practice::work::{
    CaseVisibility as Visibility, CheckScope, CheckStatus, LearnerCaseVerdict, Setting,
};
use lattice_core::domain::learning::practice::work::{Form, KindId, Stage};

fn file(path: &str, content: &str) -> LearningLabFile {
    LearningLabFile {
        path: path.into(),
        content: content.into(),
    }
}

fn detail(
    case_id: &str,
    visibility: CaseVisibility,
    mine: bool,
    status: CaseStatus,
) -> CaseCheckDetailDto {
    CaseCheckDetailDto {
        case_id: case_id.into(),
        category: format!("category of {case_id}"),
        visibility,
        scale: false,
        mine,
        status,
        args: Some(vec!["3".into()]),
        expected: Some("6".into()),
        actual: Some("5".into()),
        printed: None,
        error: None,
        elapsed_ms: Some(2),
    }
}

fn bundle() -> SessionBundle {
    let draft = vec![file("solution.py", "def f(n):\n    return n\n")];
    SessionBundle {
        row: CodingRow {
            id: "s1".into(),
            program_id: "p1".into(),
            activity_id: "a1".into(),
            kind: KindId::new("program"),
            task: Form::Build,
            setting: Setting::Mock,
            stage: Stage::Make,
            budget_seconds: Some(600),
            active_ms: 5_000,
            opened_at: Some(10),
            draft: draft.clone(),
            pending_cue: None,
            submitted_at: None,
            solution_seen_at: None,
            finished_at: None,
            revision: 4,
            created_at: 1,
        },
        lesson_id: "l1".into(),
        title: "A problem".into(),
        activity_revision: 2,
        problem: problem(),
        events: vec![StoredEvent {
            ordinal: 0,
            kind: "setting".into(),
            payload: serde_json::json!({"from": "coached", "to": "mock"}),
            at: 5,
        }],
        examples: vec![StoredExample {
            ordinal: 1,
            args: vec!["3".into()],
            expected: "6".into(),
            verdict: ExampleVerdict::Right,
            reference_actual: Some("6".into()),
            why_invalid: None,
            catches: vec!["m1".into()],
        }],
        runs: vec![StoredRun {
            run_id: "r1".into(),
            scope: CodingRunScope::All,
            status: LearningPracticalRunStatus::Failed,
            stdout: String::new(),
            stderr: String::new(),
            duration_ms: Some(4),
            files: draft,
            created_at: 20,
            cases: vec![
                detail("e1", CaseVisibility::Example, false, CaseStatus::Passed),
                detail("mine-1", CaseVisibility::Example, true, CaseStatus::Passed),
                detail("h1", CaseVisibility::Hidden, false, CaseStatus::Failed),
            ],
        }],
        turns: Vec::new(),
        scorecard: None,
        debrief: None,
    }
}

#[test]
fn a_session_reads_its_row_as_the_engine_names_it() {
    let session = session_of(&bundle()).unwrap();
    assert_eq!(session.row.setting, Setting::Mock);
    assert_eq!(session.row.budget_seconds, Some(600));
    assert_eq!(session.row.revision, 4);
    assert_eq!(session.activity_revision, 2);
    assert_eq!(session.settings, vec![Setting::Coached, Setting::Mock]);
}

#[test]
fn a_stored_run_is_a_check_with_the_cases_it_ran() {
    let session = session_of(&bundle()).unwrap();
    let [check] = session.checks.as_slice() else {
        panic!("one check");
    };
    assert_eq!(check.id, "r1");
    assert_eq!(check.scope, CheckScope::All);
    assert_eq!(check.status, CheckStatus::Failed);
    assert_eq!(check.at, 20);
    let seen = check
        .cases
        .iter()
        .map(|checked| {
            (
                checked.case.id.as_str(),
                checked.own,
                checked.case.visibility,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        seen,
        vec![
            ("e1", false, Visibility::Shown),
            ("mine-1", true, Visibility::Shown),
            ("h1", false, Visibility::Hidden),
        ]
    );
    assert_eq!(check.report.results.len(), 3);
}

#[test]
fn a_learner_example_keeps_its_verdict_and_what_it_catches() {
    let session = session_of(&bundle()).unwrap();
    let [case] = session.learner_cases.as_slice() else {
        panic!("one learner case");
    };
    assert_eq!(case.ordinal, 1);
    assert_eq!(case.verdict, LearnerCaseVerdict::Right);
    assert_eq!(case.catches, vec!["m1".to_owned()]);
}
