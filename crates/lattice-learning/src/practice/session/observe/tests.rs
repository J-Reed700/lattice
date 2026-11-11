#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::practice::session::fake_kind::{fake_kind, letters_exercise};
use crate::practice::session::record::{
    CheckedCase, SessionCheck, SessionHint, SessionLearnerCase, SessionRow,
};
use lattice_core::domain::learning::practice::work::{
    CaseResult, CaseStatus, CaseVisibility, CheckReport, CheckStatus, Form, LearnerCaseVerdict,
    Rung, Setting, Stage,
};
use lattice_core::shared::error::AppError;
use serde_json::{json, Value};

fn piece(slots: &[&str]) -> Piece {
    Piece::new(json!(slots))
}

fn kind() -> Kind {
    fake_kind(&["a", "b", "c"]).with_learner_cases().build()
}

fn row(setting: Setting) -> SessionRow {
    SessionRow {
        id: "session-1".to_owned(),
        program_id: "program-1".to_owned(),
        activity_id: "activity-1".to_owned(),
        form: Form::Build,
        setting,
        stage: Stage::Make,
        budget_seconds: Some(900),
        active_ms: 61_000,
        opened_at: None,
        draft: piece(&["a", "x", "c"]),
        pending_cue: None,
        submitted_at: None,
        solution_seen_at: None,
        finished_at: None,
        revision: 3,
        created_at: 0,
    }
}

fn shown_case() -> Case {
    Case {
        id: "shown-1".to_owned(),
        title: "Second slot".to_owned(),
        visibility: CaseVisibility::Shown,
        body: json!({"index": 1, "expected": "b"}),
    }
}

fn own_case() -> Case {
    Case {
        id: "mine-1".to_owned(),
        title: "Your case 1".to_owned(),
        visibility: CaseVisibility::Shown,
        body: json!({"index": 1, "expected": "b"}),
    }
}

fn session(setting: Setting) -> Session {
    let mut exercise = letters_exercise();
    exercise.cases.push(shown_case());
    Session {
        row: row(setting),
        activity_revision: 1,
        exercise,
        settings: vec![setting],
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

fn result(id: &str, status: CaseStatus) -> CaseResult {
    CaseResult {
        case_id: id.to_owned(),
        status,
        detail: Value::Null,
    }
}

fn check(id: &str, scope: CheckScope, at: i64, results: &[(&str, CaseStatus)]) -> SessionCheck {
    SessionCheck {
        id: id.to_owned(),
        scope,
        piece: piece(&["a", "x", "c"]),
        report: CheckReport {
            results: results
                .iter()
                .map(|(case, status)| result(case, *status))
                .collect(),
            marks: Vec::new(),
            detail: Value::Null,
        },
        at,
        status: CheckStatus::Failed,
        cases: Vec::new(),
    }
}

/// A check of all three cases: the hidden one fails, the shown one passes, the learner's own one fails.
fn mixed_check() -> SessionCheck {
    let mut each = check(
        "c1",
        CheckScope::All,
        10,
        &[
            ("case-1", CaseStatus::Failed),
            ("shown-1", CaseStatus::Passed),
            ("mine-1", CaseStatus::Failed),
        ],
    );
    each.cases = vec![
        CheckedCase {
            case: letters_exercise().cases[0].clone(),
            own: false,
        },
        CheckedCase {
            case: shown_case(),
            own: false,
        },
        CheckedCase {
            case: own_case(),
            own: true,
        },
    ];
    each
}

fn open_of(observation: &Observation, case_id: &str) -> bool {
    observation
        .last_check
        .as_ref()
        .unwrap()
        .cases
        .iter()
        .find(|view| view.case.id == case_id)
        .unwrap()
        .open
}

fn observed(session: &Session) -> Observation {
    observe(&kind(), session, &session.row.draft, None).unwrap()
}

fn learner_case(ordinal: u32, verdict: LearnerCaseVerdict, catches: &[&str]) -> SessionLearnerCase {
    SessionLearnerCase {
        ordinal,
        body: json!({"letter": "b"}),
        verdict,
        catches: catches.iter().map(|id| (*id).to_owned()).collect(),
        found: Value::Null,
    }
}

fn ids(cases: &[Case]) -> Vec<&str> {
    cases.iter().map(|case| case.id.as_str()).collect()
}

#[test]
fn a_hidden_failing_case_is_closed_in_mock_and_open_in_coached() {
    let mut mock = session(Setting::Mock);
    mock.checks.push(mixed_check());
    assert!(!open_of(&observed(&mock), "case-1"));

    let mut coached = session(Setting::Coached);
    coached.checks.push(mixed_check());
    assert!(open_of(&observed(&coached), "case-1"));
}

#[test]
fn a_finished_session_opens_every_case() {
    let mut finished = session(Setting::Mock);
    finished.checks.push(mixed_check());
    finished.row.finished_at = Some(99);
    assert!(open_of(&observed(&finished), "case-1"));
}

#[test]
fn a_shown_case_and_the_learners_own_are_open_in_every_setting() {
    for setting in [Setting::Coached, Setting::Mock, Setting::Watched] {
        let mut each = session(setting);
        each.checks.push(mixed_check());
        let observation = observed(&each);
        assert!(open_of(&observation, "shown-1"), "{setting:?}");
        assert!(open_of(&observation, "mine-1"), "{setting:?}");
    }
}

#[test]
fn the_last_check_is_the_one_the_counterpart_reads() {
    let mut each = session(Setting::Mock);
    assert!(observed(&each).last_check.is_none());
    each.checks.push(check("c0", CheckScope::Shown, 5, &[]));
    each.checks.push(mixed_check());
    let last = observed(&each).last_check.unwrap();
    assert_eq!(last.id, "c1");
    assert_eq!(last.scope, CheckScope::All);
    assert_eq!(last.status, CheckStatus::Failed);
    assert_eq!(last.cases.len(), 3);
    assert_eq!(
        last.cases[0].result.as_ref().map(|result| result.status),
        Some(CaseStatus::Failed)
    );
}

#[test]
fn what_the_learners_cases_miss_is_named_only_where_the_setting_discloses_it() {
    let mut coached = session(Setting::Coached);
    coached
        .learner_cases
        .push(learner_case(1, LearnerCaseVerdict::Right, &[]));
    let observation = observed(&coached);
    assert_eq!(observation.coverage.total, 1);
    assert_eq!(observation.coverage.caught, 0);
    assert_eq!(
        observation.coverage.uncaught.as_deref(),
        Some("Reads the letters backwards")
    );

    let mut mock = session(Setting::Mock);
    mock.learner_cases = coached.learner_cases.clone();
    let observation = observed(&mock);
    assert_eq!(observation.coverage.total, 1);
    assert_eq!(observation.coverage.caught, 0);
    assert_eq!(observation.coverage.uncaught, None);
}

#[test]
fn only_a_right_case_catches_a_misreading() {
    let mut each = session(Setting::Coached);
    each.learner_cases
        .push(learner_case(1, LearnerCaseVerdict::Wrong, &["miss-1"]));
    assert_eq!(observed(&each).coverage.caught, 0);
    each.learner_cases
        .push(learner_case(2, LearnerCaseVerdict::Right, &["miss-1"]));
    let observation = observed(&each);
    assert_eq!(observation.coverage.caught, 1);
    assert_eq!(observation.coverage.uncaught, None);
    assert_eq!(observation.learner_cases.len(), 2);
}

#[test]
fn changes_are_read_only_against_a_draft_the_counterpart_has_seen() {
    let each = session(Setting::Mock);
    let none = observe(&kind(), &each, &each.row.draft, None).unwrap();
    assert!(!none.has_baseline);
    assert!(none.changes.is_empty());

    let seen = piece(&["a", "b", "c"]);
    let some = observe(&kind(), &each, &each.row.draft, Some(&seen)).unwrap();
    assert!(some.has_baseline);
    assert_eq!(some.changes.len(), 2);
}

#[test]
fn the_draft_read_is_the_piece_given_and_not_the_stored_one() {
    let each = session(Setting::Mock);
    let typed = piece(&["z", "z", "z"]);
    assert_eq!(observe(&kind(), &each, &typed, None).unwrap().piece, typed);
}

#[test]
fn stages_reached_stop_at_the_stage_the_session_is_in() {
    let mut each = session(Setting::Mock);
    each.row.stage = Stage::Plan;
    assert_eq!(
        observed(&each).stages_reached,
        vec![Stage::Understand, Stage::Examples, Stage::Plan]
    );
    let plain = fake_kind(&["a", "b", "c"]).build();
    assert_eq!(
        observe(&plain, &each, &each.row.draft, None)
            .unwrap()
            .stages_reached,
        vec![Stage::Understand, Stage::Plan]
    );
}

#[test]
fn a_hint_carries_the_check_it_was_given_after() {
    let mut each = session(Setting::Mock);
    each.hints.push(SessionHint {
        ordinal: 1,
        rung: Rung::Idea,
        text: "Look at the second slot.".to_owned(),
        stage: Stage::Make,
        check_id: Some("c1".to_owned()),
        reason: String::new(),
        parts: Vec::new(),
        in_role: false,
        at: 0,
    });
    let hints = observed(&each).hints;
    assert_eq!(hints.len(), 1);
    assert_eq!(hints[0].rung, Rung::Idea);
    assert_eq!(hints[0].check_id.as_deref(), Some("c1"));
}

#[test]
fn the_brief_the_cases_shown_and_the_clock_come_from_the_session() {
    let each = session(Setting::Mock);
    let observation = observed(&each);
    assert_eq!(observation.session_id, "session-1");
    assert_eq!(observation.stage, Stage::Make);
    assert_eq!(observation.brief, each.exercise.brief);
    assert_eq!(ids(&observation.shown), ["shown-1"]);
    assert_eq!(observation.clock.active_ms, 61_000);
    assert_eq!(observation.clock.budget_seconds, Some(900));
}

#[test]
fn a_check_of_the_shown_cases_adds_the_learners_right_cases_as_their_own() {
    let mut each = session(Setting::Mock);
    each.learner_cases = vec![
        learner_case(1, LearnerCaseVerdict::Right, &[]),
        learner_case(2, LearnerCaseVerdict::Wrong, &[]),
        learner_case(3, LearnerCaseVerdict::Invalid, &[]),
    ];
    let (cases, own) = cases_for(&kind(), &each, CheckScope::Shown).unwrap();
    assert_eq!(ids(&cases), ["shown-1", "mine-1"]);
    assert_eq!(own, ["mine-1"]);
}

#[test]
fn a_check_of_every_case_runs_every_case_and_none_of_the_learners() {
    let mut each = session(Setting::Mock);
    each.learner_cases = vec![learner_case(1, LearnerCaseVerdict::Right, &[])];
    let (cases, own) = cases_for(&kind(), &each, CheckScope::All).unwrap();
    assert_eq!(ids(&cases), ["case-1", "shown-1"]);
    assert!(own.is_empty());
}

#[test]
fn a_check_of_the_learners_own_input_is_a_kinds_own_command() {
    let refused = cases_for(&kind(), &session(Setting::Mock), CheckScope::Own);
    assert!(
        matches!(refused, Err(AppError::InvalidInput(_))),
        "{refused:?}"
    );
}

#[test]
fn the_last_check_of_every_case_decides_whether_everything_passed() {
    let pass =
        |id: &str, at: i64| check(id, CheckScope::All, at, &[("case-1", CaseStatus::Passed)]);
    let fail =
        |id: &str, at: i64| check(id, CheckScope::All, at, &[("case-1", CaseStatus::Failed)]);

    let mut each = session(Setting::Mock);
    assert!(!outcome(&each).all_passed);

    each.checks = vec![pass("c1", 1), fail("c2", 2)];
    assert!(!outcome(&each).all_passed);

    each.checks = vec![fail("c1", 1), pass("c2", 2)];
    assert!(outcome(&each).all_passed);

    each.checks = vec![
        fail("c1", 1),
        check(
            "c2",
            CheckScope::Shown,
            2,
            &[("shown-1", CaseStatus::Passed)],
        ),
    ];
    assert!(!outcome(&each).all_passed);

    each.checks = vec![check("c1", CheckScope::All, 1, &[])];
    assert!(
        !outcome(&each).all_passed,
        "a check with no case passes nothing"
    );
}

#[test]
fn a_solution_seen_after_the_first_pass_is_not_seen_before_passing() {
    let mut each = session(Setting::Coached);
    each.checks = vec![check(
        "c1",
        CheckScope::All,
        100,
        &[("case-1", CaseStatus::Passed)],
    )];
    assert!(!outcome(&each).solution_seen_before_passing);

    each.row.solution_seen_at = Some(200);
    assert!(!outcome(&each).solution_seen_before_passing);

    each.row.solution_seen_at = Some(50);
    assert!(outcome(&each).solution_seen_before_passing);

    each.checks.clear();
    assert!(outcome(&each).solution_seen_before_passing);
}

#[test]
fn the_highest_rung_given_is_the_outcomes() {
    let mut each = session(Setting::Mock);
    assert_eq!(outcome(&each).highest_rung, None);
    for (ordinal, rung) in [(1, Rung::Nudge), (2, Rung::Plan), (3, Rung::Idea)] {
        each.hints.push(SessionHint {
            ordinal,
            rung,
            text: String::new(),
            stage: Stage::Make,
            check_id: None,
            reason: String::new(),
            parts: Vec::new(),
            in_role: false,
            at: 0,
        });
    }
    assert_eq!(outcome(&each).highest_rung, Some(Rung::Plan));
}

#[test]
fn the_counts_of_a_submit_are_those_of_the_last_check_of_every_case() {
    let mut each = session(Setting::Mock);
    assert_eq!(submit_counts(&each), (0, 0));
    each.checks = vec![
        check("c1", CheckScope::All, 1, &[("case-1", CaseStatus::Failed)]),
        check(
            "c2",
            CheckScope::All,
            2,
            &[
                ("case-1", CaseStatus::Passed),
                ("shown-1", CaseStatus::Failed),
                ("mine-1", CaseStatus::Passed),
            ],
        ),
        check(
            "c3",
            CheckScope::Shown,
            3,
            &[("shown-1", CaseStatus::Passed)],
        ),
    ];
    assert_eq!(submit_counts(&each), (2, 3));
}
