#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::practice::session::fake_kind::fake_kind;
use lattice_core::domain::learning::practice::work::{CaseVisibility, Difficulty, PartRef};
use serde_json::json;

fn case(id: &str, title: &str, index: usize) -> Case {
    Case {
        id: id.to_owned(),
        title: title.to_owned(),
        visibility: CaseVisibility::Hidden,
        body: json!({"index": index, "expected": "a"}),
    }
}

fn result(id: &str, status: CaseStatus) -> CaseResult {
    CaseResult {
        case_id: id.to_owned(),
        status,
        detail: serde_json::Value::Null,
    }
}

fn turn(id: &str, speaker: LearningSimulationSpeaker, content: &str) -> LearningSimulationTurnDto {
    LearningSimulationTurnDto {
        id: id.to_owned(),
        ordinal: 0,
        speaker,
        content: content.to_owned(),
        citations: Vec::new(),
        model_name: None,
        created_at: 0,
    }
}

fn plain() -> Observation {
    Observation {
        session_id: "session-1".to_owned(),
        form: Form::Build,
        setting: Setting::Mock,
        stage: Stage::Make,
        stages_reached: vec![Stage::Understand, Stage::Plan, Stage::Make],
        brief: Brief {
            title: "Spell it".to_owned(),
            difficulty: Difficulty::Easy,
            spec: serde_json::Value::Null,
        },
        shown: vec![case("shown-1", "First slot", 0)],
        clock: ClockView {
            active_ms: 61_999,
            budget_seconds: None,
        },
        piece: Piece::new(json!(["a", "b"])),
        has_baseline: false,
        changes: Vec::new(),
        last_check: None,
        learner_cases: Vec::new(),
        coverage: Coverage {
            caught: 0,
            total: 0,
            uncaught: None,
        },
        hints: Vec::new(),
        turns: Vec::new(),
    }
}

fn checked() -> CheckView {
    CheckView {
        id: "check-1".to_owned(),
        scope: CheckScope::All,
        status: CheckStatus::Failed,
        cases: vec![
            CaseView {
                case: case("open-1", "Open slot", 0),
                result: Some(result("open-1", CaseStatus::Passed)),
                open: true,
            },
            CaseView {
                case: case("closed-1", "Closed slot", 1),
                result: Some(result("closed-1", CaseStatus::Failed)),
                open: false,
            },
            CaseView {
                case: case("waiting-1", "Waiting slot", 2),
                result: None,
                open: false,
            },
        ],
        detail: json!({"notes": ["First note", "Second note"]}),
    }
}

fn rich() -> Observation {
    Observation {
        has_baseline: true,
        changes: vec![
            Change {
                part: PartRef::new("1"),
                kind: ChangeKind::Removed,
                text: "b".to_owned(),
            },
            Change {
                part: PartRef::new("1"),
                kind: ChangeKind::Added,
                text: "c".to_owned(),
            },
        ],
        last_check: Some(checked()),
        learner_cases: vec![
            LearnerCaseView {
                ordinal: 0,
                body: json!({"letter": "z"}),
                verdict: LearnerCaseVerdict::Right,
            },
            LearnerCaseView {
                ordinal: 1,
                body: json!({"letter": "y"}),
                verdict: LearnerCaseVerdict::Invalid,
            },
        ],
        coverage: Coverage {
            caught: 1,
            total: 2,
            uncaught: Some("Reads the order backwards".to_owned()),
        },
        hints: vec![HintView {
            rung: Rung::Nudge,
            text: "Look at the second slot.".to_owned(),
            check_id: Some("check-1".to_owned()),
        }],
        turns: vec![
            turn("turn-1", LearningSimulationSpeaker::Counterpart, "Welcome."),
            turn("turn-2", LearningSimulationSpeaker::Learner, "Hello."),
            turn("turn-3", LearningSimulationSpeaker::Coach, "Go on."),
        ],
        ..plain()
    }
}

fn kind() -> Kind {
    fake_kind(&["a", "b"]).build()
}

fn kind_with_learner_cases() -> Kind {
    fake_kind(&["a", "b"]).with_learner_cases().build()
}

#[test]
fn a_plain_observation_renders_every_block_in_order() {
    let text = plain().render(&kind()).unwrap();
    assert_eq!(
        text,
        "Brief: Spell it\n\
         Shown: First slot\n\
         Task: Build. Setting: Mock. Stage: Make. Stages reached: Understand, Plan, Make.\n\
         Clock: 61 s active, no budget.\n\
         Draft:\n\
         a, b\n\
         Changes since you last looked: you have not looked at the draft before.\n\
         Last check: none yet.\n\
         Transcript:\n"
    );
}

#[test]
fn a_rich_observation_renders_every_block_in_order() {
    let text = rich().render(&kind_with_learner_cases()).unwrap();
    assert_eq!(
        text,
        "Brief: Spell it\n\
         Shown: First slot\n\
         Task: Build. Setting: Mock. Stage: Make. Stages reached: Understand, Plan, Make.\n\
         Clock: 61 s active, no budget.\n\
         Draft:\n\
         a, b\n\
         Changes since you last looked:\n\
         removed slot 1: b\n\
         added slot 1: c\n\
         Last check: All, Failed.\n\
         - Passed: Open slot (slot 0) -> Passed\n\
         - Failed: Closed slot -> Failed\n\
         - NotRun: Waiting slot\n\
         First note\n\
         Second note\n\
         Learner's example 0: letter z - Right\n\
         Learner's example 1: letter y - Invalid\n\
         Examples catch 1 of 2 common mistakes.\n\
         One mistake they do not catch yet: Reads the order backwards\n\
         Hint given (Nudge): Look at the second slot.\n\
         Transcript:\n\
         You: Welcome.\n\
         Learner: Hello.\n\
         You: Go on.\n"
    );
}

#[test]
fn the_three_states_of_the_changes_block() {
    let kind = kind();
    let mut observation = plain();
    let unseen = observation.render(&kind).unwrap();
    assert!(unseen
        .contains("Changes since you last looked: you have not looked at the draft before.\n"));

    observation.has_baseline = true;
    let none = observation.render(&kind).unwrap();
    assert!(none.contains("Changes since you last looked: none.\n"));
    assert!(!none.contains("not looked"));

    observation.changes = rich().changes;
    let some = observation.render(&kind).unwrap();
    assert!(some.contains(
        "Changes since you last looked:\nremoved slot 1: b\nadded slot 1: c\nLast check:"
    ));
}

#[test]
fn a_check_shows_an_open_case_in_full_and_a_closed_case_by_its_title() {
    let mut observation = plain();
    observation.last_check = Some(checked());
    let text = observation.render(&kind()).unwrap();
    assert!(text.contains("Last check: All, Failed.\n"));
    assert!(text.contains("- Passed: Open slot (slot 0) -> Passed\n"));
    assert!(text.contains("- Failed: Closed slot -> Failed\n"));
    assert!(!text.contains("(slot 1)"));
}

#[test]
fn a_case_without_a_result_stands_as_not_run() {
    let mut observation = plain();
    observation.last_check = Some(checked());
    let text = observation.render(&kind()).unwrap();
    assert!(text.contains("- NotRun: Waiting slot\n"));
}

#[test]
fn the_notes_of_a_check_follow_its_cases_one_line_each() {
    let mut observation = plain();
    observation.last_check = Some(checked());
    let text = observation.render(&kind()).unwrap();
    assert!(text.contains("- NotRun: Waiting slot\nFirst note\nSecond note\nTranscript:\n"));
}

#[test]
fn a_check_with_nothing_to_add_has_no_notes() {
    let mut observation = plain();
    let mut check = checked();
    check.detail = serde_json::Value::Null;
    observation.last_check = Some(check);
    let text = observation.render(&kind()).unwrap();
    assert!(text.contains("- NotRun: Waiting slot\nTranscript:\n"));
}

#[test]
fn the_learner_cases_block_is_there_only_when_the_kind_has_the_role() {
    let observation = rich();
    let without = observation.render(&kind()).unwrap();
    assert!(!without.contains("Learner's example"));
    assert!(!without.contains("Examples catch"));
    assert!(!without.contains("One mistake they do not catch yet"));
    let with = observation.render(&kind_with_learner_cases()).unwrap();
    assert!(with.contains("Learner's example 0: letter z - Right\n"));
    assert!(with.contains("Examples catch 1 of 2 common mistakes.\n"));
    assert!(with.contains("One mistake they do not catch yet: Reads the order backwards\n"));
}

#[test]
fn the_uncaught_line_is_there_only_when_there_is_a_mistake_left() {
    let mut observation = rich();
    observation.coverage.uncaught = None;
    let text = observation.render(&kind_with_learner_cases()).unwrap();
    assert!(text.contains("Examples catch 1 of 2 common mistakes.\nHint given"));
    assert!(!text.contains("One mistake"));
}

#[test]
fn the_clock_reads_whole_seconds_with_or_without_a_budget() {
    let mut observation = plain();
    let unbudgeted = observation.render(&kind()).unwrap();
    assert!(unbudgeted.contains("Clock: 61 s active, no budget.\n"));
    observation.clock.budget_seconds = Some(1200);
    let budgeted = observation.render(&kind()).unwrap();
    assert!(budgeted.contains("Clock: 61 s active of a 1200 s budget.\n"));
}

#[test]
fn the_words_come_from_the_kinds_wording() {
    let mut observation = plain();
    observation.form = Form::Repair;
    observation.stage = Stage::Fix;
    observation.stages_reached = vec![Stage::Reproduce, Stage::Locate, Stage::Fix];
    let text = observation.render(&kind()).unwrap();
    assert!(text.contains(
        "Task: Repair. Setting: Mock. Stage: Fix. Stages reached: Reproduce, Locate, Fix.\n"
    ));
}

#[test]
fn a_part_the_kind_cannot_name_is_an_error_not_a_blank() {
    let mut observation = plain();
    observation.has_baseline = true;
    observation.changes = vec![Change {
        part: PartRef::new("not a slot"),
        kind: ChangeKind::Added,
        text: "x".to_owned(),
    }];
    assert!(observation.render(&kind()).is_err());
}

#[test]
fn the_internal_ids_are_the_session_turns_hints_check_and_cases_in_that_order() {
    assert_eq!(
        rich().internal_ids(),
        [
            "session-1",
            "turn-1",
            "turn-2",
            "turn-3",
            "check-1",
            "check-1",
            "open-1",
            "closed-1",
            "waiting-1",
            "shown-1"
        ]
    );
}

#[test]
fn a_plain_observation_has_only_the_session_and_the_shown_cases_as_ids() {
    assert_eq!(plain().internal_ids(), ["session-1", "shown-1"]);
}

#[test]
fn the_rendering_holds_no_internal_id() {
    let observation = rich();
    let text = observation.render(&kind_with_learner_cases()).unwrap();
    for id in observation.internal_ids() {
        assert!(!text.contains(&id), "the rendering holds the id {id}");
    }
}
