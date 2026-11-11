#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::practice::coding::dto::stages_of;
use crate::practice::coding::golden_support::{assert_golden, rich_observation, Golden};

const SETTINGS: [CodingSetting; 3] = [
    CodingSetting::Coached,
    CodingSetting::Mock,
    CodingSetting::Watched,
];

const TASKS: [CodingTask; 2] = [CodingTask::Problem, CodingTask::BugFix];

const STAGES: [CodingStage; 10] = [
    CodingStage::Understand,
    CodingStage::Examples,
    CodingStage::Plan,
    CodingStage::Code,
    CodingStage::Test,
    CodingStage::Review,
    CodingStage::Reproduce,
    CodingStage::Locate,
    CodingStage::Fix,
    CodingStage::Verify,
];

fn cues() -> Vec<(String, CodingCue)> {
    let mut cues = vec![
        ("Opening".to_owned(), CodingCue::Opening),
        ("Reply".to_owned(), CodingCue::Reply),
    ];
    for stage in STAGES {
        cues.push((
            format!("StageOpened({stage:?})"),
            CodingCue::StageOpened(stage),
        ));
    }
    cues.push((
        "AskForSkipped([Examples, Plan])".to_owned(),
        CodingCue::AskForSkipped(vec![CodingStage::Examples, CodingStage::Plan]),
    ));
    cues.push((
        "AskForSkipped([Reproduce])".to_owned(),
        CodingCue::AskForSkipped(vec![CodingStage::Reproduce]),
    ));
    for scope in [
        CodingRunScope::Examples,
        CodingRunScope::All,
        CodingRunScope::Input,
    ] {
        cues.push((format!("RunReview({scope:?})"), CodingCue::RunReview(scope)));
    }
    cues.push(("Submitted".to_owned(), CodingCue::Submitted));
    cues.push(("Checkpoint".to_owned(), CodingCue::Checkpoint));
    cues.push(("Idle".to_owned(), CodingCue::Idle));
    cues
}

#[test]
fn golden_policy() {
    let mut golden = Golden::default();
    for stage in STAGES {
        golden.add(
            &format!("stage_instruction({stage:?})"),
            stage_instruction(stage),
        );
    }
    for task in TASKS {
        for setting in SETTINGS {
            let (_, mut observation) = rich_observation(task, setting);
            observation.stage = stages_of(task)
                .first()
                .copied()
                .unwrap_or(CodingStage::Understand);
            let policy = policy_for(setting, task);
            for (name, cue) in cues() {
                golden.add(
                    &format!("system {task:?} {setting:?} {name}"),
                    &policy.system(&cue, &observation),
                );
            }
        }
    }
    assert_golden(
        "policy",
        include_str!("../golden/policy.txt"),
        golden.text(),
    );
}
