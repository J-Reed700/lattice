//! Stage 2: an independent reviewer repairs the clean code, and the repaired code is the baseline.

use super::support::{
    build, code, config, files, flawed_limiter, idle_grader, last, messages, review_answer, spec,
    system, FakeRunner, ScriptedModel, SUITE,
};
use crate::features::learning::drills::build::{CleanCode, RunRecord};
use crate::features::learning::drills::runner::RunOutcome;
use crate::features::learning::tests::pool;
use lattice_core::domain::learning::drills::{DrillFile, Severity};
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};

/// The code the stage reviews: the project with a flaw in `forget`.
fn flawed() -> CleanCode {
    let mut flawed = code();
    flawed.files = vec![
        DrillFile {
            path: "src/limiter.py".into(),
            text: flawed_limiter(),
        },
        DrillFile {
            path: "src/config.py".into(),
            text: config(),
        },
    ];
    flawed
}

const FLAW: &str = "        del self.hits[key]\n";
const REPAIR: &str = "        self.hits.pop(key, None)\n";

fn issue(before: &str, after: &str) -> Value {
    json!({
        "description": "forget raises KeyError for a key never seen",
        "severity": "minor",
        "path": "src/limiter.py",
        "before": before,
        "after": after
    })
}

#[tokio::test]
async fn a_review_that_finds_nothing_leaves_the_code_and_runs_nothing() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([review_answer(vec![])]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    let reviewed = build(&pool, &model, Some(&runner), &grader)
        .clean_review(&spec(), &code())
        .await?;
    assert_eq!(reviewed.files, files());
    assert!(reviewed.repairs.is_empty());
    assert!(runner.runs().is_empty());
    assert_eq!(reviewed.trail.calls.len(), 1);
    assert_eq!(reviewed.trail.calls[0].purpose, "clean review");
    Ok(())
}

#[tokio::test]
async fn repairs_are_made_and_the_suite_runs_on_the_repaired_code() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([review_answer(vec![issue(FLAW, REPAIR)])]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    let reviewed = build(&pool, &model, Some(&runner), &grader)
        .clean_review(&spec(), &flawed())
        .await?;
    assert_eq!(reviewed.files, files(), "the repaired code is the baseline");
    assert_eq!(reviewed.repairs.len(), 1);
    assert_eq!(reviewed.repairs[0].edit.before, FLAW);
    assert_eq!(reviewed.repairs[0].edit.after, REPAIR);
    assert_eq!(reviewed.repairs[0].severity, Severity::Minor);
    assert_eq!(runner.runs(), vec![(files(), SUITE.to_owned())]);
    assert_eq!(
        reviewed.trail.runs,
        vec![RunRecord {
            what: "The suite on the repaired code".into(),
            outcome: RunOutcome::Passed
        }]
    );
    Ok(())
}

/// The reviewer's first answer is `bad`, which has to be refused with a message holding `fragment`; the second repairs
/// the flaw.
async fn refused_then_accepted(bad: String, fragment: &str) -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([bad, review_answer(vec![issue(FLAW, REPAIR)])]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    let reviewed = build(&pool, &model, Some(&runner), &grader)
        .clean_review(&spec(), &flawed())
        .await?;
    assert_eq!(reviewed.files, files(), "{fragment}");
    let requests = model.requests();
    assert_eq!(requests.len(), 2, "{fragment}");
    let reask = last(&requests[1]);
    assert!(
        reask.starts_with("That answer could not be used:"),
        "{reask}"
    );
    assert!(reask.contains(fragment), "{fragment} in {reask}");
    Ok(())
}

#[tokio::test]
async fn a_repair_that_does_not_apply_is_refused_naming_it_and_what_to_change() -> Result<()> {
    let mut elsewhere = issue(FLAW, REPAIR);
    elsewhere["path"] = json!("src/other.py");
    let cases = [
        (
            review_answer(vec![issue("        del self.hits[other]\n", REPAIR)]),
            "Repair 1: `before` is not in src/limiter.py",
        ),
        (
            review_answer(vec![issue("        self.hits[key] = recent\n", REPAIR)]),
            "`before` is in src/limiter.py 2 times",
        ),
        (
            review_answer(vec![issue("", REPAIR)]),
            "Repair 1: `before` is empty",
        ),
        (
            review_answer(vec![elsewhere]),
            "Repair 1: src/other.py is not one of the files",
        ),
        (
            review_answer(vec![issue(FLAW, REPAIR), issue(FLAW, REPAIR)]),
            "Repair 1 and Repair 2 overlap",
        ),
        (
            review_answer(vec![issue(FLAW, FLAW)]),
            "Repair 1 changes nothing",
        ),
    ];
    for (bad, fragment) in cases {
        refused_then_accepted(bad, fragment).await?;
    }
    Ok(())
}

#[tokio::test]
async fn a_review_refused_twice_fails_the_stage() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([
        review_answer(vec![issue("        del self.hits[other]\n", REPAIR)]),
        review_answer(vec![issue("        del self.hits[another]\n", REPAIR)]),
    ]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    let error = build(&pool, &model, Some(&runner), &grader)
        .clean_review(&spec(), &flawed())
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AppError::ModelAnswerUnusable(reason) if reason.contains("not in src/limiter.py")),
        "{error:?}"
    );
    assert_eq!(model.requests().len(), 2);
    assert!(runner.runs().is_empty());
    Ok(())
}

#[tokio::test]
async fn a_repair_that_breaks_the_suite_is_sent_back_once_with_the_output() -> Result<()> {
    let pool = pool().await?;
    let breaking = issue(FLAW, "        self.hits.clear()\n");
    let model = ScriptedModel::new([
        review_answer(vec![breaking]),
        review_answer(vec![issue(FLAW, REPAIR)]),
    ]);
    let runner = FakeRunner::scripted(vec![
        RunOutcome::Failed {
            output: "AssertionError: the limiter forgot every key".into(),
        },
        RunOutcome::Passed,
    ]);
    let grader = idle_grader();
    let reviewed = build(&pool, &model, Some(&runner), &grader)
        .clean_review(&spec(), &flawed())
        .await?;
    assert_eq!(reviewed.files, files());
    let reask = last(&model.requests()[1]);
    assert!(
        reask.starts_with("That answer could not be used:"),
        "{reask}"
    );
    assert!(reask.contains("the limiter forgot every key"), "{reask}");
    let purposes: Vec<&str> = reviewed
        .trail
        .calls
        .iter()
        .map(|call| call.purpose.as_str())
        .collect();
    assert_eq!(purposes, ["clean review", "clean review (asked again)"]);
    Ok(())
}

#[tokio::test]
async fn a_repair_that_breaks_the_suite_twice_fails_the_stage() -> Result<()> {
    let pool = pool().await?;
    let breaking = || review_answer(vec![issue(FLAW, "        self.hits.clear()\n")]);
    let model = ScriptedModel::new([breaking(), breaking()]);
    let runner = FakeRunner::new(|_, _| RunOutcome::Failed {
        output: "AssertionError: the limiter forgot every key".into(),
    });
    let grader = idle_grader();
    let error = build(&pool, &model, Some(&runner), &grader)
        .clean_review(&spec(), &flawed())
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AppError::ModelAnswerUnusable(reason) if reason.contains("forgot every key")),
        "{error:?}"
    );
    assert_eq!(model.requests().len(), 2);
    Ok(())
}

#[tokio::test]
async fn the_reviewer_is_told_nothing_of_planting() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([review_answer(vec![])]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    build(&pool, &model, Some(&runner), &grader)
        .clean_review(&spec(), &code())
        .await?;
    let request = &model.requests()[0];
    let system = system(request).to_lowercase();
    for word in ["plant", "mutat", "exercise", "key"] {
        assert!(!system.contains(word), "{word}");
    }
    let sent = messages(request);
    assert_eq!(sent.len(), 2);
    assert!(sent[1]
        .1
        .starts_with("Language: python\n\n<file path=\"src/limiter.py\">"));
    Ok(())
}
