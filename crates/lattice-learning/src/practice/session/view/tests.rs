#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::practice::practical_dto::{LearningSimulationSpeaker, LearningSimulationTurnDto};
use crate::practice::session::fake_kind::{fake_kind, letters_exercise};
use crate::practice::session::policy::Cue;
use crate::practice::session::record::{
    CheckedCase, EventKind, SessionEvent, SessionRow, TechniqueRating,
};
use lattice_core::domain::learning::drills::PatternRating;
use lattice_core::domain::learning::practice::work::{
    Case, CaseResult, CaseVisibility, CheckReport, CheckScope, CheckStatus, Difficulty, Form,
    Piece, Rung, Setting, Stage,
};
use lattice_core::domain::learning::practice::Scorecard;
use serde_json::{json, Value};

fn plain_kind() -> Kind {
    fake_kind(&["a", "b", "c"]).build()
}

fn kind_with_learner_cases() -> Kind {
    fake_kind(&["a", "b", "c"]).with_learner_cases().build()
}

fn piece(slots: &[&str]) -> Piece {
    Piece::new(json!(slots))
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
        opened_at: Some(40),
        draft: piece(&["a", "x", "c"]),
        pending_cue: None,
        submitted_at: None,
        solution_seen_at: None,
        finished_at: None,
        revision: 3,
        created_at: 0,
    }
}

fn session(setting: Setting) -> Session {
    Session {
        row: row(setting),
        activity_revision: 1,
        exercise: letters_exercise(),
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

fn check(id: &str, scope: CheckScope, at: i64, statuses: &[CaseStatus]) -> SessionCheck {
    SessionCheck {
        id: id.to_owned(),
        scope,
        piece: piece(&["a", "x", "c"]),
        report: CheckReport {
            results: statuses
                .iter()
                .enumerate()
                .map(|(index, status)| CaseResult {
                    case_id: format!("case-{index}"),
                    status: *status,
                    detail: Value::Null,
                })
                .collect(),
            marks: Vec::new(),
            detail: json!({"notes": ["kept by the kind"]}),
        },
        at,
        status: CheckStatus::Failed,
        cases: statuses
            .iter()
            .enumerate()
            .map(|(index, _)| CheckedCase {
                case: Case {
                    id: format!("case-{index}"),
                    title: "A case".to_owned(),
                    visibility: CaseVisibility::Hidden,
                    body: json!({"index": index, "expected": "a"}),
                },
                own: false,
            })
            .collect(),
    }
}

fn hint(ordinal: u32, rung: Rung, check_id: Option<&str>) -> SessionHint {
    SessionHint {
        ordinal,
        rung,
        text: format!("Hint {ordinal}"),
        stage: Stage::Make,
        check_id: check_id.map(str::to_owned),
        reason: format!("Reason {ordinal}"),
        parts: Vec::new(),
        in_role: ordinal == 2,
        at: 700 + i64::from(ordinal),
    }
}

fn learner_case(ordinal: u32, verdict: LearnerCaseVerdict, catches: &[&str]) -> SessionLearnerCase {
    SessionLearnerCase {
        ordinal,
        body: json!({"index": 0, "expected": "a"}),
        verdict,
        catches: catches.iter().map(|each| (*each).to_owned()).collect(),
        found: Value::Null,
    }
}

fn technique(id: &str, rating: PatternRating) -> TechniqueRating {
    TechniqueRating {
        id: id.to_owned(),
        name: format!("Name of {id}"),
        family: "Family".to_owned(),
        cue: format!("Cue of {id}"),
        rating,
        due_at: 9_000,
    }
}

fn debrief() -> SessionDebrief {
    SessionDebrief {
        scorecard: Scorecard { rows: Vec::new() },
        stated: vec![("model".to_owned(), "The model answer.".to_owned())],
        model: "fake-model".to_owned(),
        techniques: vec![
            technique("first", PatternRating::Good),
            technique("second", PatternRating::Again),
        ],
    }
}

fn finished(mut session: Session) -> Session {
    session.row.finished_at = Some(1_000);
    session.debrief = Some(debrief());
    session
}

#[test]
fn the_view_names_the_kind_the_title_and_the_stages_in_the_forms_order() {
    let with_cases = session_view(&kind_with_learner_cases(), &session(Setting::Mock)).unwrap();
    assert_eq!(with_cases.kind, "letters");
    assert_eq!(with_cases.title, "Spell it");
    assert_eq!(with_cases.difficulty, Difficulty::Easy);
    assert_eq!(with_cases.session_id, "session-1");
    assert_eq!(with_cases.program_id, "program-1");
    assert_eq!(with_cases.activity_id, "activity-1");
    assert_eq!(with_cases.lesson_id.as_deref(), Some("lesson-1"));
    let expected: Vec<(Stage, String)> = Form::Build
        .stages(true)
        .into_iter()
        .map(|stage| (stage, stage.label().to_owned()))
        .collect();
    let labelled: Vec<(Stage, String)> = with_cases
        .stages
        .iter()
        .map(|each| (each.stage, each.label.clone()))
        .collect();
    assert_eq!(labelled, expected);

    let without = session_view(&plain_kind(), &session(Setting::Mock)).unwrap();
    let stages: Vec<Stage> = without.stages.iter().map(|each| each.stage).collect();
    assert_eq!(stages, Form::Build.stages(false));
    assert_ne!(stages, Form::Build.stages(true));
}

#[test]
fn the_view_carries_the_rows_own_values() {
    let mut held = session(Setting::Watched);
    held.settings = vec![Setting::Coached, Setting::Watched];
    held.turns = vec![LearningSimulationTurnDto {
        id: "turn-1".to_owned(),
        ordinal: 0,
        speaker: LearningSimulationSpeaker::Learner,
        content: "Where do I start?".to_owned(),
        citations: Vec::new(),
        model_name: None,
        created_at: 0,
    }];
    held.row.pending_cue = Some(Cue::Idle);
    let view = session_view(&plain_kind(), &held).unwrap();
    assert_eq!(view.form, Form::Build);
    assert_eq!(view.setting, Setting::Watched);
    assert_eq!(view.settings_used, [Setting::Coached, Setting::Watched]);
    assert_eq!(view.stage, Stage::Make);
    assert_eq!(view.revision, 3);
    assert_eq!(
        view.clock,
        ClockDto {
            active_ms: 61_000,
            open_since: Some(40),
            budget_seconds: Some(900),
        }
    );
    assert_eq!(view.turns.len(), 1);
    assert_eq!(view.turns[0].id, "turn-1");
    assert!(view.cue_pending);
    assert!(!view.finished);
    assert!(!view.submitted);
}

#[test]
fn a_check_is_summarized_with_its_counts_and_what_it_found_is_the_kinds_to_draw() {
    let mut held = session(Setting::Coached);
    held.checks = vec![
        check(
            "check-a",
            CheckScope::Shown,
            100,
            &[CaseStatus::Passed, CaseStatus::Failed, CaseStatus::Passed],
        ),
        check("check-b", CheckScope::All, 200, &[CaseStatus::Passed]),
    ];
    let view = session_view(&plain_kind(), &held).unwrap();
    assert_eq!(
        view.checks,
        [
            PracticeCheckDto {
                check_id: "check-a".to_owned(),
                scope: CheckScope::Shown,
                status: CheckStatus::Failed,
                passed: 2,
                total: 3,
                at: 100,
            },
            PracticeCheckDto {
                check_id: "check-b".to_owned(),
                scope: CheckScope::All,
                status: CheckStatus::Failed,
                passed: 1,
                total: 1,
                at: 200,
            },
        ]
    );
    let found: Vec<&str> = view.work["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|each| each["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        found,
        ["check-a", "check-b"],
        "the kind draws one per check"
    );
    assert_eq!(
        view.work["checks"][0]["found"]["notes"][0],
        "kept by the kind"
    );
    assert_eq!(view.work["draft"], json!(["a", "x", "c"]));
}

#[test]
fn a_hint_carries_its_reason_the_check_it_followed_and_when_it_was_given() {
    let mut held = session(Setting::Coached);
    held.hints = vec![
        hint(1, Rung::Nudge, Some("check-a")),
        hint(2, Rung::Idea, None),
    ];
    let view = session_view(&plain_kind(), &held).unwrap();
    assert_eq!(
        view.hints,
        [
            PracticeHintDto {
                ordinal: 1,
                rung: Rung::Nudge,
                text: "Hint 1".to_owned(),
                reason: "Reason 1".to_owned(),
                stage: Stage::Make,
                check_id: Some("check-a".to_owned()),
                in_role: false,
                at: 701,
            },
            PracticeHintDto {
                ordinal: 2,
                rung: Rung::Idea,
                text: "Hint 2".to_owned(),
                reason: "Reason 2".to_owned(),
                stage: Stage::Make,
                check_id: None,
                in_role: true,
                at: 702,
            },
        ]
    );
    assert_eq!(view.next_rung, Rung::Plan);
}

#[test]
fn coverage_is_absent_for_a_kind_without_cases_the_learner_writes() {
    let mut held = session(Setting::Coached);
    held.learner_cases = vec![learner_case(1, LearnerCaseVerdict::Right, &["miss-1"])];
    let view = session_view(&plain_kind(), &held).unwrap();
    assert_eq!(view.coverage, None);
}

#[test]
fn coverage_counts_the_misreadings_the_right_cases_catch_and_withholds_the_uncaught_one_by_setting()
{
    for (setting, shown) in [
        (Setting::Coached, true),
        (Setting::Mock, false),
        (Setting::Watched, false),
    ] {
        let none = session_view(&kind_with_learner_cases(), &session(setting)).unwrap();
        assert_eq!(
            none.coverage,
            Some(MisreadingCoverageDto {
                caught: 0,
                total: 1,
                uncaught: shown.then(|| "Reads the letters backwards".to_owned()),
            }),
            "{setting:?}"
        );

        let mut held = session(setting);
        held.learner_cases = vec![
            learner_case(1, LearnerCaseVerdict::Wrong, &["miss-1"]),
            learner_case(2, LearnerCaseVerdict::Invalid, &["miss-1"]),
        ];
        let wrong = session_view(&kind_with_learner_cases(), &held).unwrap();
        assert_eq!(
            wrong.coverage.unwrap().caught,
            0,
            "only a right case counts"
        );

        held.learner_cases
            .push(learner_case(3, LearnerCaseVerdict::Right, &["miss-1"]));
        let caught = session_view(&kind_with_learner_cases(), &held).unwrap();
        assert_eq!(
            caught.coverage,
            Some(MisreadingCoverageDto {
                caught: 1,
                total: 1,
                uncaught: None,
            }),
            "{setting:?}"
        );
    }
}

#[test]
fn a_finished_session_shows_everything_the_open_one_withholds() {
    let mut held = session(Setting::Mock);
    held.row.submitted_at = Some(500);
    let open = session_view(&kind_with_learner_cases(), &held).unwrap();
    assert!(!open.solution_available, "Mock never offers the reference");
    assert!(!open.solution_shown);
    assert!(open.work["reference"].is_null());
    assert_eq!(open.coverage.unwrap().uncaught, None);

    let done = session_view(&kind_with_learner_cases(), &finished(held)).unwrap();
    assert!(done.finished);
    assert!(done.solution_shown);
    assert_eq!(done.work["reference"], json!(["a", "b", "c"]));
    assert_eq!(
        done.coverage.unwrap().uncaught.as_deref(),
        Some("Reads the letters backwards")
    );
}

#[test]
fn the_reference_is_shown_in_coached_only_once_it_was_revealed_after_a_submit() {
    let mut held = session(Setting::Coached);
    let before = session_view(&plain_kind(), &held).unwrap();
    assert!(!before.solution_available);
    assert!(!before.solution_shown);

    held.row.submitted_at = Some(500);
    let submitted = session_view(&plain_kind(), &held).unwrap();
    assert!(submitted.solution_available);
    assert!(!submitted.solution_shown, "available is not shown");
    assert!(submitted.submitted);
    assert!(submitted.work["reference"].is_null());

    held.row.solution_seen_at = Some(600);
    let revealed = session_view(&plain_kind(), &held).unwrap();
    assert!(revealed.solution_shown);
    assert_eq!(revealed.work["reference"], json!(["a", "b", "c"]));
}

#[test]
fn the_debrief_is_there_only_when_the_session_is_finished_and_has_one_stored() {
    let open = session(Setting::Mock);
    assert!(session_view(&plain_kind(), &open)
        .unwrap()
        .debrief
        .is_none());

    let mut stored_early = session(Setting::Mock);
    stored_early.debrief = Some(debrief());
    assert!(
        session_view(&plain_kind(), &stored_early)
            .unwrap()
            .debrief
            .is_none(),
        "a debrief of a session that is not finished stays unseen"
    );

    let mut unwritten = session(Setting::Mock);
    unwritten.row.finished_at = Some(1_000);
    let view = session_view(&plain_kind(), &unwritten).unwrap();
    assert!(view.finished);
    assert!(view.debrief.is_none(), "nothing stored, nothing shown");

    let view = session_view(&plain_kind(), &finished(session(Setting::Mock))).unwrap();
    assert!(view.debrief.is_some());
}

#[test]
fn the_debrief_carries_the_scorecard_the_model_the_techniques_and_the_kinds_part() {
    let view = session_view(&plain_kind(), &finished(session(Setting::Mock))).unwrap();
    let debrief = view.debrief.unwrap();
    assert_eq!(debrief.scorecard, Scorecard { rows: Vec::new() });
    assert_eq!(debrief.model, "fake-model");
    assert_eq!(
        debrief.techniques,
        [
            TechniqueRatingDto {
                technique_id: "first".to_owned(),
                name: "Name of first".to_owned(),
                family: "Family".to_owned(),
                cue: "Cue of first".to_owned(),
                rating: PatternRating::Good,
                due_at: 9_000,
            },
            TechniqueRatingDto {
                technique_id: "second".to_owned(),
                name: "Name of second".to_owned(),
                family: "Family".to_owned(),
                cue: "Cue of second".to_owned(),
                rating: PatternRating::Again,
                due_at: 9_000,
            },
        ]
    );
    assert_eq!(
        debrief.work,
        json!({"stated": [["model", "The model answer."]]})
    );
    assert!(
        debrief.misreadings.is_empty(),
        "no cases of the learner's own"
    );
}

#[test]
fn the_debrief_lists_each_misreading_and_whether_the_learner_caught_it() {
    let mut held = session(Setting::Mock);
    let missed = session_view(&kind_with_learner_cases(), &finished(held.clone())).unwrap();
    assert_eq!(
        missed.debrief.unwrap().misreadings,
        [MisreadingViewDto {
            description: "Reads the letters backwards".to_owned(),
            caught: false,
        }]
    );
    held.learner_cases = vec![learner_case(1, LearnerCaseVerdict::Right, &["miss-1"])];
    let caught = session_view(&kind_with_learner_cases(), &finished(held)).unwrap();
    assert_eq!(
        caught.debrief.unwrap().misreadings,
        [MisreadingViewDto {
            description: "Reads the letters backwards".to_owned(),
            caught: true,
        }]
    );
}

#[test]
fn the_debriefs_facts_are_counted_from_the_record() {
    let mut held = session(Setting::Mock);
    held.settings = vec![Setting::Coached, Setting::Mock];
    held.checks = vec![
        check("check-a", CheckScope::Shown, 100, &[CaseStatus::Passed]),
        check("check-b", CheckScope::All, 200, &[CaseStatus::Failed]),
        check("check-c", CheckScope::All, 300, &[CaseStatus::Passed]),
    ];
    held.hints = vec![
        hint(1, Rung::Nudge, None),
        hint(2, Rung::Nudge, None),
        hint(3, Rung::Plan, None),
    ];
    held.events = vec![
        SessionEvent {
            ordinal: 1,
            kind: EventKind::Opened,
            at: 1_000,
        },
        SessionEvent {
            ordinal: 2,
            kind: EventKind::Moved(Some(Stage::Plan)),
            at: 4_000,
        },
        SessionEvent {
            ordinal: 3,
            kind: EventKind::Closed,
            at: 9_000,
        },
    ];
    let view = session_view(&kind_with_learner_cases(), &finished(held)).unwrap();
    let facts = view.debrief.unwrap().facts;
    assert_eq!(facts.checks, 3);
    assert_eq!(facts.submits, 2);
    assert_eq!(
        facts.hints,
        [
            RungCountDto {
                rung: Rung::Nudge,
                count: 2
            },
            RungCountDto {
                rung: Rung::Plan,
                count: 1
            },
        ]
    );
    assert_eq!(facts.settings, [Setting::Coached, Setting::Mock]);
    assert_eq!(
        facts.stage_ms,
        [
            StageTimeDto {
                stage: Stage::Understand,
                active_ms: 3_000
            },
            StageTimeDto {
                stage: Stage::Plan,
                active_ms: 5_000
            },
        ]
    );
}

#[test]
fn the_wire_spells_the_new_fields_in_camel_case() {
    let view = session_view(&plain_kind(), &session(Setting::Mock)).unwrap();
    let wire = serde_json::to_value(&view).unwrap();
    assert_eq!(wire["kind"], "letters");
    assert_eq!(wire["lessonId"], "lesson-1");
    assert_eq!(wire["solutionShown"], false);
    assert!(wire.get("coverage").is_some_and(Value::is_null));
}
