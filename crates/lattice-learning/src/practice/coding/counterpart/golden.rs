#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::practice::coding::dto::{CodingRunScope, CodingStage};
use crate::practice::coding::golden_support::{
    add_requests, assert_golden, rich_observation, Golden, Recorder,
};
use crate::practice::coding::policy::policy_for;
use crate::practice::coding::problem::{CodingSetting, CodingTask};
use crate::practice::coding::proof::fakes::{Behaviour, FakeHarness, FakeRunner};

fn cases() -> Vec<(&'static str, CodingSetting, CodingCue)> {
    vec![
        ("Opening", CodingSetting::Coached, CodingCue::Opening),
        ("Opening", CodingSetting::Mock, CodingCue::Opening),
        ("Reply", CodingSetting::Coached, CodingCue::Reply),
        (
            "StageOpened(Plan)",
            CodingSetting::Coached,
            CodingCue::StageOpened(CodingStage::Plan),
        ),
        (
            "AskForSkipped([Examples, Plan])",
            CodingSetting::Mock,
            CodingCue::AskForSkipped(vec![CodingStage::Examples, CodingStage::Plan]),
        ),
        (
            "RunReview(All)",
            CodingSetting::Coached,
            CodingCue::RunReview(CodingRunScope::All),
        ),
        ("Submitted", CodingSetting::Coached, CodingCue::Submitted),
        ("Submitted", CodingSetting::Mock, CodingCue::Submitted),
        ("Checkpoint", CodingSetting::Watched, CodingCue::Checkpoint),
        ("Idle", CodingSetting::Coached, CodingCue::Idle),
    ]
}

fn reply(cue: &CodingCue, lines: &[u32]) -> String {
    let mut answer = serde_json::json!({
        "content": "Say more about what you plan to do.",
        "lines": lines,
        "path": "solution.py",
        "clarification": "none",
    });
    if *cue == CodingCue::Checkpoint {
        answer["speak"] = serde_json::json!(true);
    }
    answer.to_string()
}

async fn asked(
    task: CodingTask,
    setting: CodingSetting,
    cue: &CodingCue,
    answers: Vec<String>,
) -> Recorder {
    let (problem, observation) = rich_observation(task, setting);
    let policy = policy_for(setting, task);
    let runner = FakeRunner::new(|_, _| Behaviour::Fails);
    let model = Recorder::new(answers);
    let result = respond(
        &model,
        &runner,
        &FakeHarness,
        &problem,
        policy.as_ref(),
        cue,
        &observation,
        CancellationToken::new(),
    )
    .await;
    assert!(result.is_ok(), "{result:?}");
    model
}

#[tokio::test]
async fn golden_counterpart() {
    let mut golden = Golden::default();
    for task in [CodingTask::Problem, CodingTask::BugFix] {
        for (name, setting, cue) in cases() {
            let model = asked(task, setting, &cue, vec![reply(&cue, &[])]).await;
            add_requests(&mut golden, &format!("{task:?} {setting:?} {name}"), &model);
        }
        let cue = CodingCue::Reply;
        let model = asked(
            task,
            CodingSetting::Coached,
            &cue,
            vec![reply(&cue, &[99]), reply(&cue, &[])],
        )
        .await;
        add_requests(
            &mut golden,
            &format!("{task:?} Coached Reply, after a refused reply"),
            &model,
        );
    }
    assert_golden(
        "counterpart",
        include_str!("../golden/counterpart.txt"),
        golden.text(),
    );
}
