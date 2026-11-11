//! The engine's policies, with a program's roles, say what today's coding policies say, byte for byte.

#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use crate::practice::coding::golden_support::{assert_golden, rich_session, Golden};
use crate::practice::kinds::program::observation_tests::{engine_observation, program_kind};
use crate::practice::session::kind::Wording;
use crate::practice::session::policy::{policy_for, Cue};
use lattice_core::domain::learning::practice::work::Setting;
use lattice_core::domain::learning::practice::work::{Brief, CheckScope, Difficulty, Form, Stage};

const SETTINGS: [Setting; 3] = [Setting::Coached, Setting::Mock, Setting::Watched];

const TASKS: [Form; 2] = [Form::Build, Form::Repair];

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

fn cues() -> Vec<Cue> {
    let mut cues = vec![Cue::Opening, Cue::Reply];
    cues.extend(STAGES.into_iter().map(Cue::StageOpened));
    cues.push(Cue::AskForSkipped(vec![Stage::Examples, Stage::Plan]));
    cues.push(Cue::AskForSkipped(vec![Stage::Reproduce]));
    for scope in [CheckScope::Shown, CheckScope::All, CheckScope::Own] {
        cues.push(Cue::CheckReview(scope));
    }
    cues.extend([Cue::Submitted, Cue::Checkpoint, Cue::Idle]);
    cues
}

#[test]
fn the_engine_policy_with_a_programs_roles_reproduces_the_coding_policy() {
    let kind = program_kind();
    let wording = &kind.wording;
    let brief = Brief {
        title: String::new(),
        difficulty: Difficulty::Easy,
        spec: serde_json::Value::Null,
    };
    let mut golden = Golden::default();
    for stage in STAGES {
        golden.add(
            &format!(
                "stage_instruction({})",
                wording.stage_label(&brief, Form::Build, stage)
            ),
            &wording.stage_instruction(&brief, Form::Build, stage),
        );
    }
    for task in TASKS {
        let form = task;
        for setting in SETTINGS {
            let (problem, session, seen) = rich_session(task, setting);
            let mut observation = engine_observation(&problem, &session, Some(&seen), false);
            observation.stage = form.stages(true).first().copied().unwrap();
            let policy = policy_for(setting, &kind, form);
            for cue in cues() {
                golden.add(
                    &format!(
                        "system {} {} {}",
                        wording.form_label(form),
                        setting.label(),
                        cue_name(wording.as_ref(), &observation.brief, form, &cue)
                    ),
                    &policy.system(&cue, &observation),
                );
            }
        }
    }
    assert_golden(
        "policy",
        include_str!("../../coding/golden/policy.txt"),
        golden.text(),
    );
}

fn cue_name(wording: &dyn Wording, brief: &Brief, form: Form, cue: &Cue) -> String {
    let stage = |stage: &Stage| wording.stage_label(brief, form, *stage);
    match cue {
        Cue::Opening => "Opening".to_owned(),
        Cue::Reply => "Reply".to_owned(),
        Cue::StageOpened(opened) => format!("StageOpened({})", stage(opened)),
        Cue::AskForSkipped(stages) => format!(
            "AskForSkipped([{}])",
            stages.iter().map(stage).collect::<Vec<_>>().join(", ")
        ),
        Cue::CheckReview(scope) => format!("RunReview({})", wording.scope_label(*scope)),
        Cue::Submitted => "Submitted".to_owned(),
        Cue::Checkpoint => "Checkpoint".to_owned(),
        Cue::Idle => "Idle".to_owned(),
    }
}
