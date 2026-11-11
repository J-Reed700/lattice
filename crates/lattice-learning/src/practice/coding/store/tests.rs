//! What the engine reads of a session is what the coding tables hold, without a database.

use super::*;
use crate::lab_runtime::LearningLabFile;
use crate::practical_dto::{CaseCheckDetailDto, LearningPracticalRunStatus};
use crate::practice::coding::observation::test_support::problem;
use crate::practice::coding::problem::{CaseStatus, CaseVisibility};
use crate::practice::coding::records::{
    CodingRow, SessionBundle, StoredDebrief, StoredEvent, StoredExample, StoredRating, StoredRun,
};
use crate::practice::kinds::program::dto::{CodingRunScope, ExampleVerdict};
use crate::practice::kinds::program::parts::part_of;
use crate::practice::kinds::program::shape::{learner_found_in, run_output_of, run_report_of};
use lattice_core::domain::learning::drills::PatternRating;
use lattice_core::domain::learning::practice::work::Rung;
use lattice_core::domain::learning::practice::work::{
    CaseVisibility as Visibility, CheckScope, CheckStatus, LearnerCaseVerdict, Setting,
};
use lattice_core::domain::learning::practice::work::{Form, KindId, Stage};
use lattice_core::domain::learning::practice::{Scorecard, ScorecardRow};

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
            form: Form::Build,
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

#[test]
fn what_the_reference_found_of_an_example_is_kept_with_it() {
    let mut bundle = bundle();
    bundle.examples[0].verdict = ExampleVerdict::Wrong;
    bundle.examples[0].reference_actual = Some("7".into());
    bundle.examples[0].why_invalid = Some("not a number".into());
    let session = session_of(&bundle).unwrap();
    let found = learner_found_in(&session.learner_cases[0].found).unwrap();
    assert_eq!(found.reference_actual.as_deref(), Some("7"));
    assert_eq!(found.why_invalid.as_deref(), Some("not a number"));
}

#[test]
fn the_session_carries_its_lesson() {
    assert_eq!(session_of(&bundle()).unwrap().lesson_id, "l1");
}

#[test]
fn a_run_keeps_its_duration_and_the_reference_answer_of_its_input() {
    let mut bundle = bundle();
    bundle.runs[0].stdout = "out".into();
    bundle.events.push(StoredEvent {
        ordinal: 1,
        kind: "check".into(),
        payload: serde_json::json!({"runId": "r1", "referenceActual": "9"}),
        at: 21,
    });
    let session = session_of(&bundle).unwrap();
    let detail = &session.checks[0].report.detail;
    assert_eq!(
        run_output_of(detail).unwrap(),
        ("out".to_owned(), String::new())
    );
    assert_eq!(
        run_report_of(detail).unwrap(),
        (Some(4), Some("9".to_owned()))
    );
}

#[test]
fn a_hint_stored_before_the_engine_kept_its_parts_reads_back_with_them() {
    let mut bundle = bundle();
    bundle.events.push(StoredEvent {
        ordinal: 1,
        kind: "hint".into(),
        payload: serde_json::json!({
            "ordinal": 1,
            "rung": "point",
            "text": "Look here.",
            "reason": "The loop never ends.",
            "lines": [3, 4],
            "path": "solution.py",
            "stage": "make",
            "runId": "r1",
            "inRole": true,
            "at": 30
        }),
        at: 30,
    });
    let session = session_of(&bundle).unwrap();
    let [hint] = session.hints.as_slice() else {
        panic!("one hint");
    };
    assert_eq!(hint.rung, Rung::Point);
    assert_eq!(hint.reason, "The loop never ends.");
    assert_eq!(
        hint.parts,
        vec![part_of("solution.py", 3), part_of("solution.py", 4)]
    );
    assert_eq!(hint.check_id.as_deref(), Some("r1"));
    assert!(hint.in_role);
    assert_eq!(hint.at, 30);
}

#[test]
fn a_hint_without_lines_has_no_parts() {
    let mut bundle = bundle();
    bundle.events.push(StoredEvent {
        ordinal: 1,
        kind: "hint".into(),
        payload: serde_json::json!({
            "ordinal": 1,
            "rung": "nudge",
            "text": "Think.",
            "reason": "",
            "lines": [],
            "path": null,
            "stage": "make",
            "runId": null,
            "inRole": false,
            "at": 31
        }),
        at: 31,
    });
    assert!(session_of(&bundle).unwrap().hints[0].parts.is_empty());
}

#[test]
fn the_stored_debrief_is_the_sessions_in_the_rubrics_keys() {
    let mut bundle = bundle();
    assert!(session_of(&bundle).unwrap().debrief.is_none());
    bundle.scorecard = Some(Scorecard {
        rows: Vec::<ScorecardRow>::new(),
    });
    bundle.debrief = Some(StoredDebrief {
        approach: "Hash the complements.".into(),
        complexity: "O(n) time.".into(),
        ratings: vec![StoredRating {
            pattern_id: "hash-map".into(),
            name: "Hash map".into(),
            family: "Lookup".into(),
            cue: "Seen it before?".into(),
            rating: PatternRating::Good,
            due_at: 99,
        }],
        model: "model-1".into(),
    });
    let debrief = session_of(&bundle).unwrap().debrief.unwrap();
    assert_eq!(
        debrief.stated,
        vec![
            ("approach".to_owned(), "Hash the complements.".to_owned()),
            ("complexity".to_owned(), "O(n) time.".to_owned()),
        ]
    );
    assert_eq!(debrief.model, "model-1");
    assert_eq!(debrief.techniques[0].id, "hash-map");
    assert_eq!(debrief.techniques[0].due_at, 99);
}
