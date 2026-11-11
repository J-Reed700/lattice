//! The engine's counterpart reply, with a program's roles, says what today's coding counterpart says, byte for
//! byte, and refuses what it refuses.

#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use crate::practice::coding::golden_support::{
    add_requests, assert_golden, rich_session, Golden, Recorder,
};
use crate::practice::coding::problem::{CodingSetting, CodingTask, SOLUTION_PATH};
use crate::practice::coding::proof::fakes::{Behaviour, CaseRun, FakeRunner, ScriptedModel};
use crate::practice::kinds::program::kind;
use crate::practice::kinds::program::observation_tests::{
    engine_observation, form_of, program_kind, setting_of,
};
use crate::practice::kinds::program::parts::part_of;
use crate::practice::kinds::program::shape::exercise_of;
use crate::practice::kinds::program::toolchain::FakeToolchains;
use crate::practice::session::counterpart::{respond, Reply};
use crate::practice::session::kind::Kind;
use crate::practice::session::policy::{policy_for, Cue};
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::practice::work::{CheckScope, Stage};
use lattice_core::domain::learning::practice::PROBLEM_PATTERNS;
use lattice_core::shared::error::{AppError, Result};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

fn cases() -> Vec<(&'static str, CodingSetting, Cue)> {
    vec![
        ("Opening", CodingSetting::Coached, Cue::Opening),
        ("Opening", CodingSetting::Mock, Cue::Opening),
        ("Reply", CodingSetting::Coached, Cue::Reply),
        (
            "StageOpened(Plan)",
            CodingSetting::Coached,
            Cue::StageOpened(Stage::Plan),
        ),
        (
            "AskForSkipped([Examples, Plan])",
            CodingSetting::Mock,
            Cue::AskForSkipped(vec![Stage::Examples, Stage::Plan]),
        ),
        (
            "RunReview(All)",
            CodingSetting::Coached,
            Cue::CheckReview(CheckScope::All),
        ),
        ("Submitted", CodingSetting::Coached, Cue::Submitted),
        ("Submitted", CodingSetting::Mock, Cue::Submitted),
        ("Checkpoint", CodingSetting::Watched, Cue::Checkpoint),
        ("Idle", CodingSetting::Coached, Cue::Idle),
    ]
}

fn reply(cue: &Cue, lines: &[u32]) -> String {
    let mut answer = serde_json::json!({
        "content": "Say more about what you plan to do.",
        "lines": lines,
        "path": "solution.py",
        "clarification": "none",
    });
    if *cue == Cue::Checkpoint {
        answer["speak"] = serde_json::json!(true);
    }
    answer.to_string()
}

async fn respond_to(
    llm: &dyn LLMPort,
    kind: &Kind,
    task: CodingTask,
    setting: CodingSetting,
    cue: &Cue,
) -> Result<Option<Reply>> {
    let (problem, session, seen) = rich_session(task, setting);
    let observation = engine_observation(&problem, &session, Some(&seen), false);
    let exercise = exercise_of(&problem).unwrap();
    let policy = policy_for(setting_of(setting), kind, form_of(task));
    respond(
        llm,
        kind,
        &exercise,
        policy.as_ref(),
        cue,
        &observation,
        CancellationToken::new(),
    )
    .await
}

async fn asked(
    task: CodingTask,
    setting: CodingSetting,
    cue: &Cue,
    answers: Vec<String>,
) -> Recorder {
    let model = Recorder::new(answers);
    let result = respond_to(&model, &program_kind(), task, setting, cue).await;
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
        let cue = Cue::Reply;
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
        include_str!("../../coding/golden/counterpart.txt"),
        golden.text(),
    );
}

/// A program kind whose fake runner passes the code that says PASSES, and the toolchains it runs on.
fn passing_kind() -> (Kind, Arc<FakeToolchains>) {
    let tools = Arc::new(FakeToolchains::new(FakeRunner::new(
        |code: &str, _case: &CaseRun| {
            if code.contains("PASSES") {
                Behaviour::Passes { ms: 1 }
            } else {
                Behaviour::Fails
            }
        },
    )));
    (kind(tools.clone()), tools)
}

fn raw(fields: serde_json::Value) -> String {
    let mut base = serde_json::json!({
        "content": "Look at it again.",
        "lines": [],
        "path": null,
        "clarification": "none",
    });
    if let (Some(base), Some(fields)) = (base.as_object_mut(), fields.as_object()) {
        for (key, value) in fields {
            base.insert(key.clone(), value.clone());
        }
    }
    base.to_string()
}

async fn answered(kind: &Kind, answers: &[String]) -> Result<Option<Reply>> {
    let model = ScriptedModel::new(answers.iter().cloned());
    respond_to(
        &model,
        kind,
        CodingTask::Problem,
        CodingSetting::Coached,
        &Cue::Reply,
    )
    .await
}

/// The reason a reply is refused for: the model gives it twice, and the second refusal fails the call.
async fn refused(fields: serde_json::Value) -> String {
    let (kind, _) = passing_kind();
    let bad = raw(fields);
    match answered(&kind, &[bad.clone(), bad]).await {
        Err(AppError::ModelAnswerUnusable(why)) => why,
        other => format!("not refused: {other:?}"),
    }
}

#[tokio::test]
async fn line_zero_and_a_line_past_the_end_are_refused() {
    for line in [0, 4] {
        assert_eq!(
            refused(serde_json::json!({"lines": [line], "path": "solution.py"})).await,
            format!("Line {line} is not in solution.py, which has 3 lines. Refer only to lines that exist.")
        );
    }
}

#[tokio::test]
async fn a_path_that_is_not_a_learners_file_is_refused() {
    assert_eq!(
        refused(serde_json::json!({"lines": [1], "path": "other.py"})).await,
        "other.py is not one of the learner's files."
    );
}

#[tokio::test]
async fn your_call_without_its_line_is_refused() {
    assert_eq!(
        refused(serde_json::json!({"content": "It is up to you.", "clarification": "your_call"}))
            .await,
        "With clarification your_call the reply must contain \"Your call: state your assumption\"."
    );
}

#[tokio::test]
async fn a_clarification_out_of_range_is_refused() {
    assert_eq!(
        refused(serde_json::json!({"clarification": "7"})).await,
        "7 is not a clarification of this problem. Use a clarification's number, your_call or none."
    );
}

#[tokio::test]
async fn a_missing_clarification_answer_is_refused() {
    assert_eq!(
        refused(serde_json::json!({"content": "Yes, it can be empty.", "clarification": "0"}))
            .await,
        "The reply must contain the answer to clarification 0 word for word: Yes, and its total is 0."
    );
}

#[tokio::test]
async fn a_named_technique_is_refused() {
    let name = PROBLEM_PATTERNS.first().unwrap().name;
    assert_eq!(
        refused(serde_json::json!({"content": format!("Try {name} here.")})).await,
        format!(
            "The reply names the technique \"{name}\". Do not name it; ask about the idea instead."
        )
    );
}

#[tokio::test]
async fn the_clarification_answered_verbatim_is_accepted() {
    let (kind, _) = passing_kind();
    let good = raw(serde_json::json!({
        "content": "Yes, and its total is 0. Anything else?",
        "clarification": "0",
    }));
    let reply = answered(&kind, &[good]).await.unwrap().unwrap();
    assert_eq!(reply.content, "Yes, and its total is 0. Anything else?");
    assert!(reply.parts.is_empty());
}

#[tokio::test]
async fn a_block_that_passes_every_case_is_refused_and_a_failing_one_is_not() {
    let (kind, tools) = passing_kind();
    let solved = raw(serde_json::json!({
        "content": "Try this:\n```python\ndef total(values):\n    return sum(values)  # PASSES\n```\n",
    }));
    let result = answered(&kind, &[solved.clone(), solved]).await;
    assert!(
        matches!(&result, Err(AppError::ModelAnswerUnusable(why)) if why == "A code block in the reply passes every case, so it solves the problem. Do not write a working solution; describe the step in words, or show a fragment that leaves the problem unsolved."),
        "{result:?}"
    );
    let ran = tools.runner.codes();
    assert!(!ran.is_empty() && ran.iter().all(|code| code.contains("PASSES")));

    let (kind, tools) = passing_kind();
    let fragment = raw(serde_json::json!({
        "content": "Think about this:\n```python\nfor value in values:\n    pass\n```\n",
    }));
    let result = answered(&kind, &[fragment]).await;
    assert!(result.unwrap().is_some());
    let ran = tools.runner.codes();
    assert!(!ran.is_empty() && ran.iter().all(|code| !code.contains("PASSES")));
}

#[tokio::test]
async fn a_reply_without_a_block_runs_nothing() {
    let (kind, tools) = passing_kind();
    let plain = raw(serde_json::json!({}));
    assert!(answered(&kind, &[plain]).await.unwrap().is_some());
    assert!(tools.runner.codes().is_empty());
}

#[tokio::test]
async fn lines_without_a_path_point_into_the_solution() {
    let (kind, _) = passing_kind();
    let pointing = raw(serde_json::json!({"content": "Check these two.", "lines": [2, 3]}));
    let reply = answered(&kind, &[pointing]).await.unwrap().unwrap();
    assert_eq!(
        reply.parts,
        vec![part_of(SOLUTION_PATH, 2), part_of(SOLUTION_PATH, 3)]
    );
}
