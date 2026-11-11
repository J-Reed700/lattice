#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::practice::coding::golden_support::{add_requests, assert_golden, Golden, Recorder};

async fn drafted(asked: &ProblemRequest, answers: Vec<Value>) -> Recorder {
    let model = Recorder::new(answers.iter().map(Value::to_string));
    let judge = runner(killers());
    let source = GeneratedProblems {
        llm: &model,
        runner: &judge,
        harness: &FakeHarness,
    };
    let built = source
        .problem(asked, &RecordingSteps::new(), CancellationToken::new())
        .await;
    assert!(built.is_ok(), "{built:?}");
    model
}

#[tokio::test]
async fn golden_source() {
    let mut golden = Golden::default();
    for task in [CodingTask::Problem, CodingTask::BugFix] {
        for difficulty in [
            ProblemDifficulty::Easy,
            ProblemDifficulty::Medium,
            ProblemDifficulty::Hard,
        ] {
            let asked = ProblemRequest {
                difficulty,
                ..request(task)
            };
            let model = drafted(&asked, vec![answer()]).await;
            add_requests(&mut golden, &format!("{task:?} {difficulty:?}"), &model);
        }
    }
    let asked = ProblemRequest {
        lesson: None,
        patterns: Vec::new(),
        focus: Some("The focus text.".into()),
        details: Some("The details text.".into()),
        ..request(CodingTask::Problem)
    };
    let model = drafted(&asked, vec![answer()]).await;
    add_requests(
        &mut golden,
        "Problem Easy, no lesson or pattern, with a focus and details",
        &model,
    );
    for task in [CodingTask::Problem, CodingTask::BugFix] {
        let unproven = with(answer(), "reference", json!("REF BAD"));
        let model = drafted(&request(task), vec![unproven, answer()]).await;
        add_requests(
            &mut golden,
            &format!("{task:?} Easy, after an unproven draft"),
            &model,
        );
    }
    assert_golden(
        "source",
        include_str!("../../golden/source.txt"),
        golden.text(),
    );
}
