#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::practice::coding::golden_support::{add_requests, assert_golden, Golden, Recorder};
use crate::practice::kinds::program::wording::ProgramWording;
use crate::practice::session::kind::Wording;

async fn drafted(asked: &ProblemRequest, answers: Vec<Value>) -> Recorder {
    let model = Recorder::new(answers.iter().map(Value::to_string));
    let judge = runner(killers());
    let built = build_problem(
        &program(&judge),
        asked,
        &model,
        &RecordingSteps::new(),
        CancellationToken::new(),
    )
    .await;
    assert!(built.is_ok(), "{built:?}");
    model
}

#[tokio::test]
async fn golden_source() {
    let mut golden = Golden::default();
    for task in [Form::Build, Form::Repair] {
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
            add_requests(
                &mut golden,
                &format!("{} {difficulty:?}", ProgramWording.form_label(task)),
                &model,
            );
        }
    }
    let asked = ProblemRequest {
        lesson: None,
        patterns: Vec::new(),
        focus: Some("The focus text.".into()),
        details: Some("The details text.".into()),
        ..request(Form::Build)
    };
    let model = drafted(&asked, vec![answer()]).await;
    add_requests(
        &mut golden,
        "Problem Easy, no lesson or pattern, with a focus and details",
        &model,
    );
    for task in [Form::Build, Form::Repair] {
        let unproven = with(answer(), "reference", json!("REF BAD"));
        let model = drafted(&request(task), vec![unproven, answer()]).await;
        add_requests(
            &mut golden,
            &format!(
                "{} Easy, after an unproven draft",
                ProgramWording.form_label(task)
            ),
            &model,
        );
    }
    assert_golden(
        "source",
        include_str!("../../golden/source.txt"),
        golden.text(),
    );
}
