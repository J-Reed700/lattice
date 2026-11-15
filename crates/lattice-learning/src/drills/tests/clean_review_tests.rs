//! Stage 2: an independent reviewer repairs the clean code, and the repaired code is the baseline.

use super::support::{
    build, code, config, files, flawed_limiter, handle_of, idle_grader, last, messages,
    review_answer, spec, system, FakeRunner, ScriptedModel, SUITE,
};
use crate::drills::build::{CleanCode, RunRecord};
use crate::drills::runner::RunOutcome;
use crate::tests::pool;
use lattice_core::domain::learning::drills::{DrillFile, Severity};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;
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
const FLAW_LINE: &str = "        del self.hits[key]";
const REPAIR_LINE: &str = "        self.hits.pop(key, None)";

/// The handle the reviewer is shown for a line of the flawed project.
fn handle(file: &str, line: &str) -> String {
    handle_of(&flawed().files, file, line)
}

fn problem(change: Value) -> Value {
    json!({
        "description": "forget raises KeyError for a key never seen",
        "severity": "minor",
        "change": change
    })
}

fn replace(at: &str, with: &str) -> Value {
    json!({"replace": [{"at": at, "with": with}], "insert": [], "remove": [], "move": []})
}

/// The repair of the flaw.
fn repair() -> Value {
    problem(replace(&handle("src/limiter.py", FLAW_LINE), REPAIR_LINE))
}

#[tokio::test]
async fn a_review_that_finds_nothing_leaves_the_code_and_runs_nothing() -> Result<()> {
    let db = Database::new(pool().await?);
    let model = ScriptedModel::new([review_answer(vec![])]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    let reviewed = build(&db, &model, Some(&runner), &grader)
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
    let db = Database::new(pool().await?);
    let model = ScriptedModel::new([review_answer(vec![repair()])]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    let reviewed = build(&db, &model, Some(&runner), &grader)
        .clean_review(&spec(), &flawed())
        .await?;
    assert_eq!(reviewed.files, files(), "the repaired code is the baseline");
    assert_eq!(reviewed.repairs.len(), 1);
    assert_eq!(reviewed.repairs[0].edit.path, "src/limiter.py");
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

#[tokio::test]
async fn the_reviewer_is_shown_every_line_under_a_name_and_no_other_number() -> Result<()> {
    let db = Database::new(pool().await?);
    let model = ScriptedModel::new([review_answer(vec![])]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    build(&db, &model, Some(&runner), &grader)
        .clean_review(&spec(), &flawed())
        .await?;
    let task = last(&model.requests()[0]);
    let shown = format!(
        "[{}] {}",
        handle("src/limiter.py", FLAW_LINE),
        FLAW_LINE
    );
    assert!(task.contains(&shown), "{task}");
    assert!(task.contains("[u1] class RateLimiter:"), "{task}");
    Ok(())
}

/// The reviewer reports `bad` beside the repair of the flaw: `bad` is left out, and the repair stands.
async fn left_out(bad: Value, why: &str) -> Result<()> {
    let db = Database::new(pool().await?);
    let model = ScriptedModel::new([review_answer(vec![bad, repair()])]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    let reviewed = build(&db, &model, Some(&runner), &grader)
        .clean_review(&spec(), &flawed())
        .await?;
    assert_eq!(reviewed.files, files(), "{why}");
    assert_eq!(reviewed.repairs.len(), 1, "{why}");
    assert_eq!(model.requests().len(), 1, "{why}");
    Ok(())
}

#[tokio::test]
async fn a_repair_the_app_cannot_use_is_left_out_and_the_others_stand() -> Result<()> {
    let at = handle("src/limiter.py", FLAW_LINE);
    let other_file = handle("src/config.py", "DEFAULT_LIMIT = 100");
    let twice = json!({
        "replace": [{"at": at, "with": REPAIR_LINE}, {"at": at, "with": "        pass"}],
        "insert": [], "remove": [], "move": []
    });
    let across = json!({
        "replace": [{"at": at, "with": REPAIR_LINE}, {"at": other_file, "with": "DEFAULT_LIMIT = 10"}],
        "insert": [], "remove": [], "move": []
    });
    let at_start = json!({
        "replace": [],
        "insert": [{"after": "start", "with": "import os"}],
        "remove": [], "move": []
    });
    let mut nameless = repair();
    nameless["description"] = json!("  ");
    let cases = [
        (problem(replace("u999", REPAIR_LINE)), "a line that is not shown"),
        (problem(replace(&at, FLAW_LINE)), "a repair that changes nothing"),
        (problem(twice), "a line changed twice"),
        (problem(across), "two files in one repair"),
        (problem(at_start), "an insert at the start of several files"),
        (nameless, "a repair with no description"),
    ];
    for (bad, why) in cases {
        left_out(bad, why).await?;
    }
    Ok(())
}

#[tokio::test]
async fn a_repair_on_a_line_an_earlier_repair_changed_is_left_out_and_the_others_stand() -> Result<()> {
    let db = Database::new(pool().await?);
    let at = handle("src/limiter.py", FLAW_LINE);
    let model = ScriptedModel::new([review_answer(vec![
        repair(),
        problem(replace(&at, "        self.hits.clear()")),
    ])]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    let reviewed = build(&db, &model, Some(&runner), &grader)
        .clean_review(&spec(), &flawed())
        .await?;
    assert_eq!(reviewed.files, files());
    assert_eq!(reviewed.repairs.len(), 1);
    assert_eq!(model.requests().len(), 1, "nothing is asked again");
    Ok(())
}

#[tokio::test]
async fn a_review_that_cannot_be_read_twice_fails_the_stage() -> Result<()> {
    let db = Database::new(pool().await?);
    let model = ScriptedModel::new(["No problems.".to_owned(), "None at all.".to_owned()]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    let error = build(&db, &model, Some(&runner), &grader)
        .clean_review(&spec(), &flawed())
        .await
        .unwrap_err();
    assert!(matches!(&error, AppError::ModelAnswerUnusable(_)), "{error:?}");
    assert_eq!(model.requests().len(), 2);
    assert!(runner.runs().is_empty());
    Ok(())
}

fn breaking() -> String {
    review_answer(vec![problem(replace(
        &handle("src/limiter.py", FLAW_LINE),
        "        self.hits.clear()",
    ))])
}

#[tokio::test]
async fn a_repair_that_breaks_the_suite_is_sent_back_once_with_the_output() -> Result<()> {
    let db = Database::new(pool().await?);
    let model = ScriptedModel::new([breaking(), review_answer(vec![repair()])]);
    let runner = FakeRunner::scripted(vec![
        RunOutcome::Failed {
            output: "AssertionError: the limiter forgot every key".into(),
        },
        RunOutcome::Passed,
    ]);
    let grader = idle_grader();
    let reviewed = build(&db, &model, Some(&runner), &grader)
        .clean_review(&spec(), &flawed())
        .await?;
    assert_eq!(reviewed.files, files());
    let reask = last(&model.requests()[1]);
    assert!(reask.contains("Your earlier answer made these repairs"), "{reask}");
    assert!(reask.contains("self.hits.clear()"), "{reask}");
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
    let db = Database::new(pool().await?);
    let model = ScriptedModel::new([breaking(), breaking()]);
    let runner = FakeRunner::new(|_, _| RunOutcome::Failed {
        output: "AssertionError: the limiter forgot every key".into(),
    });
    let grader = idle_grader();
    let error = build(&db, &model, Some(&runner), &grader)
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

fn looping() -> String {
    review_answer(vec![problem(replace(
        &handle("src/limiter.py", FLAW_LINE),
        "        while True:\n            pass",
    ))])
}

#[tokio::test]
async fn a_repair_that_leaves_the_suite_running_is_sent_back_once_with_what_it_wrote() -> Result<()>
{
    let db = Database::new(pool().await?);
    let model = ScriptedModel::new([looping(), review_answer(vec![repair()])]);
    let runner = FakeRunner::scripted(vec![
        RunOutcome::TimedOut {
            output: "spinning".into(),
        },
        RunOutcome::Passed,
    ]);
    let grader = idle_grader();
    let reviewed = build(&db, &model, Some(&runner), &grader)
        .clean_review(&spec(), &flawed())
        .await?;
    assert_eq!(reviewed.files, files());
    let reask = last(&model.requests()[1]);
    assert!(
        reask.contains(
            "The suite on the repaired code did not finish within the runtime's time limit."
        ),
        "{reask}"
    );
    assert!(reask.contains("spinning"), "{reask}");
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
async fn a_repair_that_leaves_the_suite_running_twice_fails_the_stage_as_an_unusable_answer(
) -> Result<()> {
    let db = Database::new(pool().await?);
    let model = ScriptedModel::new([looping(), looping()]);
    let runner = FakeRunner::new(|_, _| RunOutcome::TimedOut {
        output: String::new(),
    });
    let grader = idle_grader();
    let error = build(&db, &model, Some(&runner), &grader)
        .clean_review(&spec(), &flawed())
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AppError::ModelAnswerUnusable(reason) if reason.contains("did not finish within the runtime's time limit")),
        "{error:?}"
    );
    assert_eq!(model.requests().len(), 2, "asked once more, never twice");
    Ok(())
}

#[tokio::test]
async fn a_runtime_that_is_not_there_or_a_cancelled_run_is_never_blamed_on_the_answer() -> Result<()>
{
    for (outcome, unavailable) in [
        (
            RunOutcome::Unavailable {
                why: "The bundled Python runtime is missing.".into(),
            },
            true,
        ),
        (RunOutcome::Cancelled, false),
    ] {
        let db = Database::new(pool().await?);
        let model = ScriptedModel::new([looping(), looping()]);
        let runner = FakeRunner::new(move |_, _| outcome.clone());
        let grader = idle_grader();
        let error = build(&db, &model, Some(&runner), &grader)
            .clean_review(&spec(), &flawed())
            .await
            .unwrap_err();
        if unavailable {
            assert!(
                matches!(&error, AppError::ServiceNotAvailable(_)),
                "{error:?}"
            );
        } else {
            assert!(matches!(&error, AppError::Cancelled(_)), "{error:?}");
        }
        assert_eq!(model.requests().len(), 1, "nothing is asked again");
    }
    Ok(())
}

#[tokio::test]
async fn the_reviewer_is_told_nothing_of_planting() -> Result<()> {
    let db = Database::new(pool().await?);
    let model = ScriptedModel::new([review_answer(vec![])]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    build(&db, &model, Some(&runner), &grader)
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

#[tokio::test]
async fn prompt_snapshot_drill_clean_review() -> Result<()> {
    use lattice_core::application::services::model_contract::test_support::snapshot;
    let db = Database::new(pool().await?);
    let model = ScriptedModel::new([review_answer(vec![])]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    build(&db, &model, Some(&runner), &grader)
        .clean_review(&spec(), &flawed())
        .await?;
    snapshot::assert_requests(
        env!("CARGO_MANIFEST_DIR"),
        "drill-clean-review",
        &model.requests(),
    );
    Ok(())
}
