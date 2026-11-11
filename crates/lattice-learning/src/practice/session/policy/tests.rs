#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::practice::session::fake_kind::fake_kind;
use crate::practice::session::observation::{ClockView, Coverage};
use lattice_core::domain::learning::practice::work::{Brief, Difficulty, Piece};
use lattice_core::shared::error::AppError;
use serde_json::json;

const SETTINGS: [Setting; 3] = [Setting::Coached, Setting::Mock, Setting::Watched];

const STAGES: [Stage; 10] = [
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
];

const SCOPES: [CheckScope; 3] = [CheckScope::Shown, CheckScope::All, CheckScope::Own];

fn kind() -> Kind {
    fake_kind(&["a", "b"]).build()
}

fn observation(setting: Setting, form: Form, stage: Stage) -> Observation {
    Observation {
        session_id: "session-1".to_owned(),
        form,
        setting,
        stage,
        stages_reached: vec![stage],
        brief: Brief {
            title: "Spell it".to_owned(),
            difficulty: Difficulty::Easy,
            spec: serde_json::Value::Null,
        },
        shown: Vec::new(),
        clock: ClockView {
            active_ms: 0,
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

fn moved(from: Stage, to: Stage, skipped: &[Stage]) -> Event {
    Event::StageMoved {
        from,
        to,
        skipped: skipped.to_vec(),
    }
}

fn every_cue() -> Vec<Cue> {
    let mut cues = vec![Cue::Opening, Cue::Reply];
    cues.extend(STAGES.into_iter().map(Cue::StageOpened));
    cues.push(Cue::AskForSkipped(vec![Stage::Examples, Stage::Plan]));
    cues.push(Cue::AskForSkipped(vec![Stage::Reproduce]));
    cues.extend(SCOPES.into_iter().map(Cue::CheckReview));
    cues.extend([Cue::Submitted, Cue::Checkpoint, Cue::Idle]);
    cues
}

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// One row of the cue table: the event and the cue of Coached, Mock and Watched.
fn rows() -> Vec<(&'static str, Event, [Option<Cue>; 3])> {
    vec![
        (
            "started",
            Event::Started,
            [Some(Cue::Opening), Some(Cue::Opening), Some(Cue::Opening)],
        ),
        (
            "typed message",
            Event::LearnerMessage { spoken: false },
            [Some(Cue::Reply), Some(Cue::Reply), Some(Cue::Reply)],
        ),
        (
            "spoken message",
            Event::LearnerMessage { spoken: true },
            [Some(Cue::Reply), Some(Cue::Reply), Some(Cue::Checkpoint)],
        ),
        (
            "stage moved, nothing skipped",
            moved(Stage::Understand, Stage::Examples, &[]),
            [Some(Cue::StageOpened(Stage::Examples)), None, None],
        ),
        (
            "stage moved, one asked-for stage skipped",
            moved(Stage::Understand, Stage::Plan, &[Stage::Examples]),
            [
                Some(Cue::StageOpened(Stage::Plan)),
                Some(Cue::AskForSkipped(vec![Stage::Examples])),
                Some(Cue::AskForSkipped(vec![Stage::Examples])),
            ],
        ),
        (
            "stage moved, both asked-for stages skipped",
            moved(
                Stage::Understand,
                Stage::Make,
                &[Stage::Examples, Stage::Plan],
            ),
            [
                Some(Cue::StageOpened(Stage::Make)),
                Some(Cue::AskForSkipped(vec![Stage::Examples, Stage::Plan])),
                Some(Cue::AskForSkipped(vec![Stage::Examples, Stage::Plan])),
            ],
        ),
        (
            "stage moved, asked-for and other stages skipped",
            moved(Stage::Examples, Stage::Review, &[Stage::Plan, Stage::Make]),
            [
                Some(Cue::StageOpened(Stage::Review)),
                Some(Cue::AskForSkipped(vec![Stage::Plan])),
                Some(Cue::AskForSkipped(vec![Stage::Plan])),
            ],
        ),
        (
            "stage moved, other stages skipped",
            moved(Stage::Plan, Stage::Review, &[Stage::Make, Stage::Check]),
            [Some(Cue::StageOpened(Stage::Review)), None, None],
        ),
        (
            "shown cases checked",
            Event::Checked {
                scope: CheckScope::Shown,
            },
            [
                Some(Cue::CheckReview(CheckScope::Shown)),
                None,
                Some(Cue::Checkpoint),
            ],
        ),
        (
            "own case checked",
            Event::Checked {
                scope: CheckScope::Own,
            },
            [
                Some(Cue::CheckReview(CheckScope::Own)),
                None,
                Some(Cue::Checkpoint),
            ],
        ),
        (
            "all cases checked",
            Event::Checked {
                scope: CheckScope::All,
            },
            [
                Some(Cue::CheckReview(CheckScope::All)),
                Some(Cue::Submitted),
                Some(Cue::Submitted),
            ],
        ),
        (
            "draft paused, changed",
            Event::DraftPaused { changed: true },
            [None, None, Some(Cue::Checkpoint)],
        ),
        (
            "draft paused, unchanged",
            Event::DraftPaused { changed: false },
            [None, None, None],
        ),
        ("idle", Event::Idle, [Some(Cue::Idle), None, None]),
    ]
}

#[test]
fn the_cue_table_holds_cell_by_cell() {
    let kind = kind();
    for (name, event, expected) in rows() {
        for (setting, want) in SETTINGS.into_iter().zip(expected) {
            let observation = observation(setting, Form::Build, Stage::Understand);
            let policy = policy_for(setting, &kind, Form::Build);
            assert_eq!(
                policy.cue(&event, &observation),
                want,
                "{name} in {setting:?}"
            );
        }
    }
}

#[test]
fn what_is_asked_for_when_skipped_follows_the_form() {
    let kind = kind();
    let cases = [
        (
            Form::Build,
            moved(
                Stage::Understand,
                Stage::Review,
                &[Stage::Examples, Stage::Plan, Stage::Make],
            ),
            Some(vec![Stage::Examples, Stage::Plan]),
        ),
        (
            Form::Repair,
            moved(
                Stage::Locate,
                Stage::Verify,
                &[Stage::Reproduce, Stage::Fix],
            ),
            Some(vec![Stage::Reproduce]),
        ),
        (
            Form::Repair,
            moved(Stage::Reproduce, Stage::Fix, &[Stage::Locate]),
            None,
        ),
        (
            Form::Complete,
            moved(
                Stage::Understand,
                Stage::Review,
                &[Stage::Make, Stage::Check],
            ),
            None,
        ),
        (
            Form::Arrange,
            moved(
                Stage::Understand,
                Stage::Review,
                &[Stage::Make, Stage::Check],
            ),
            None,
        ),
    ];
    for (form, event, asked) in cases {
        for setting in [Setting::Mock, Setting::Watched] {
            let observation = observation(setting, form, Stage::Understand);
            let policy = policy_for(setting, &kind, form);
            assert_eq!(
                policy.cue(&event, &observation),
                asked.clone().map(Cue::AskForSkipped),
                "{form:?} in {setting:?}"
            );
        }
    }
}

#[test]
fn a_cue_outranks_those_below_it_and_never_itself() {
    let ranked = [
        Cue::Opening,
        Cue::Reply,
        Cue::Submitted,
        Cue::AskForSkipped(vec![Stage::Examples]),
        Cue::StageOpened(Stage::Plan),
        Cue::CheckReview(CheckScope::Shown),
        Cue::Checkpoint,
        Cue::Idle,
    ];
    for (index, higher) in ranked.iter().enumerate() {
        assert!(!higher.outranks(higher), "{higher:?} outranks itself");
        for lower in ranked.iter().skip(index + 1) {
            assert!(higher.outranks(lower), "{higher:?} must outrank {lower:?}");
            assert!(
                !lower.outranks(higher),
                "{lower:?} must not outrank {higher:?}"
            );
        }
    }
}

#[test]
fn cues_of_one_kind_do_not_outrank_each_other() {
    assert!(!Cue::StageOpened(Stage::Plan).outranks(&Cue::StageOpened(Stage::Make)));
    assert!(!Cue::CheckReview(CheckScope::All).outranks(&Cue::CheckReview(CheckScope::Own)));
    assert!(!Cue::AskForSkipped(vec![Stage::Plan]).outranks(&Cue::AskForSkipped(vec![])));
}

#[test]
fn a_cue_survives_storage() {
    for cue in every_cue() {
        let json = serde_json::to_string(&cue).unwrap();
        assert_eq!(serde_json::from_str::<Cue>(&json).unwrap(), cue);
    }
    assert_eq!(
        serde_json::to_string(&Cue::CheckReview(CheckScope::All)).unwrap(),
        r#"{"check_review":"all"}"#
    );
}

#[test]
fn interviewers_ask_one_question_and_the_coach_may_ask_two() {
    let kind = kind();
    let two = "What does it hold? And for an empty one?";
    for setting in SETTINGS {
        let observation = observation(setting, Form::Build, Stage::Plan);
        let policy = policy_for(setting, &kind, Form::Build);
        let verdict = policy.check(two, &Cue::Reply, &observation);
        match setting {
            Setting::Coached => assert!(verdict.is_ok(), "{setting:?}"),
            _ => assert!(verdict.is_err(), "{setting:?}"),
        }
        assert!(
            policy
                .check("What does it hold?", &Cue::Reply, &observation)
                .is_ok(),
            "{setting:?}"
        );
    }
}

#[test]
fn a_question_mark_inside_a_fenced_block_is_not_a_question() {
    let kind = kind();
    let observation = observation(Setting::Mock, Form::Build, Stage::Make);
    let policy = policy_for(Setting::Mock, &kind, Form::Build);
    let reply = "Look at this:\n```\nx = a if b else c  # why?\nshow('?')\n```\nWhat does it show?";
    assert!(policy.check(reply, &Cue::Reply, &observation).is_ok());
}

#[test]
fn the_one_question_rule_names_the_quoted_work_only_when_the_kind_has_one() {
    let kind = kind();
    let observation = observation(Setting::Mock, Form::Build, Stage::Make);
    let policy = policy_for(Setting::Mock, &kind, Form::Build);
    let Err(AppError::InvalidInput(message)) = policy.check("One? Two?", &Cue::Reply, &observation)
    else {
        panic!("two questions must be refused");
    };
    assert!(message.ends_with("with one question mark."), "{message}");
    let system = policy.system(&Cue::Reply, &observation);
    assert!(system.contains("with one question mark."), "{system}");
}

#[test]
fn a_stage_cue_carries_the_kinds_stage_instruction() {
    let kind = kind();
    let observation = observation(Setting::Coached, Form::Build, Stage::Plan);
    let policy = policy_for(Setting::Coached, &kind, Form::Build);
    let instruction = kind
        .wording
        .stage_instruction(&observation.brief, Form::Build, Stage::Plan);
    assert!(!instruction.is_empty());
    let system = policy.system(&Cue::StageOpened(Stage::Plan), &observation);
    assert!(system.contains(&instruction), "{system}");
    let opening = policy.system(&Cue::Opening, &observation);
    assert!(opening.contains(&instruction), "{opening}");
}

#[test]
fn only_the_watching_interviewer_is_told_it_may_stay_silent() {
    let kind = kind();
    for setting in SETTINGS {
        let observation = observation(setting, Form::Build, Stage::Make);
        let policy = policy_for(setting, &kind, Form::Build);
        let system = policy.system(&Cue::Checkpoint, &observation);
        assert!(system.contains("Answer as JSON"), "{setting:?}");
        assert_eq!(
            system.contains("\"speak\""),
            setting == Setting::Watched,
            "{setting:?}"
        );
        let other = policy.system(&Cue::Reply, &observation);
        assert!(!other.contains("\"speak\""), "{setting:?}");
    }
}

#[test]
fn a_system_prompt_names_the_role_of_its_setting() {
    let kind = kind();
    let roles = [
        (
            Setting::Coached,
            "You are the coach in a practice session. You teach:",
        ),
        (
            Setting::Mock,
            "You are the interviewer in a mock interview. Stay in role: you hold a task for the candidate",
        ),
        (
            Setting::Watched,
            "You are the interviewer in a mock interview. Stay in role: you hold a task for the candidate",
        ),
    ];
    for (setting, role) in roles {
        let observation = observation(setting, Form::Build, Stage::Make);
        let policy = policy_for(setting, &kind, Form::Build);
        let system = policy.system(&Cue::Reply, &observation);
        assert!(system.starts_with(role), "{setting:?}: {system}");
        assert_eq!(
            system.contains("You are watching the candidate's work change as they work."),
            setting == Setting::Watched,
            "{setting:?}"
        );
    }
}

#[test]
fn the_engine_names_nothing_of_code() {
    let kind = kind();
    let banned = ["code", "run", "line", "problem"];
    for form in [Form::Build, Form::Repair] {
        for setting in SETTINGS {
            let observation = observation(setting, form, Stage::Understand);
            let policy = policy_for(setting, &kind, form);
            for cue in every_cue() {
                let system = policy.system(&cue, &observation);
                assert!(
                    system.contains("Answer as JSON"),
                    "{form:?} {setting:?} {cue:?}: {system}"
                );
                for word in words(&system) {
                    assert!(
                        !banned.contains(&word.as_str()),
                        "{form:?} {setting:?} {cue:?} says {word}: {system}"
                    );
                }
            }
        }
    }
}

#[test]
fn a_technique_kind_never_names_the_technique_and_an_item_kind_never_states_a_part() {
    let technique = fake_kind(&["a"]).with_spacing(Spacing::Technique).build();
    let item = fake_kind(&["a"]).with_spacing(Spacing::Item).build();
    let observation = observation(Setting::Coached, Form::Build, Stage::Make);
    let named =
        "Never name the technique or pattern that solves the task; ask about the idea instead.";
    let stated =
        "Never state what belongs in a part; ask the learner why a part belongs where they put it.";
    let technique_system =
        policy_for(Setting::Coached, &technique, Form::Build).system(&Cue::Reply, &observation);
    assert!(technique_system.contains(named), "{technique_system}");
    assert!(!technique_system.contains(stated), "{technique_system}");
    let item_system =
        policy_for(Setting::Coached, &item, Form::Build).system(&Cue::Reply, &observation);
    assert!(item_system.contains(stated), "{item_system}");
    assert!(!item_system.contains(named), "{item_system}");
}

#[test]
fn a_kind_with_no_reply_fields_gets_a_format_line_that_ends_after_the_content() {
    let kind = kind();
    for setting in SETTINGS {
        let observation = observation(setting, Form::Build, Stage::Make);
        let policy = policy_for(setting, &kind, Form::Build);
        let system = policy.system(&Cue::Reply, &observation);
        assert!(
            system.ends_with("Answer as JSON: content is what you say."),
            "{setting:?}: {system}"
        );
    }
}
