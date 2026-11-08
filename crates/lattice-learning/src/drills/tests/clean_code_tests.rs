//! Stage 1: the clean code, and in a runner language the suite that passes on it.

use super::support::{
    build, clean_code_answer, files, idle_grader, last, messages, spec, system, FakeRunner,
    ScriptedModel, LITERALS, MODEL, OPERATOR, SUITE,
};
use crate::drills::bank;
use crate::drills::build::{ModelCall, RunRecord};
use crate::drills::runner::RunOutcome;
use crate::drills::spec::DrillSpec;
use crate::tests::pool;
use lattice_core::domain::learning::drills::{DrillFile, Material};
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};

fn go_spec() -> DrillSpec {
    DrillSpec {
        material: Material::code("go"),
        domain: Some("rate limiting".into()),
        lines: 30,
        patterns: vec![OPERATOR.into(), LITERALS.into()],
        program_id: None,
    }
}

/// An answer for the Python project with its files and suite chosen by the test.
fn answer(files: Vec<Value>, suite: Option<&str>) -> String {
    let mut answer = json!({"title": "A sliding-window rate limiter", "files": files});
    if let Some(suite) = suite {
        answer["suite"] = json!(suite);
    }
    answer.to_string()
}

fn file(path: &str, text: &str) -> Value {
    json!({"path": path, "kind": "source", "text": text})
}

#[tokio::test]
async fn the_code_of_a_language_without_a_runner_is_accepted_with_its_lines_made_plain(
) -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([json!({
        "title": "A limiter",
        "files": [file("limiter.go", "package limiter\r\n\r\nfunc Allow() bool { return true }\r\n")]
    })
    .to_string()]);
    let grader = idle_grader();
    let code = build(&pool, &model, None, &grader)
        .clean_code(&go_spec())
        .await?;
    assert_eq!(
        code.files,
        vec![DrillFile {
            path: "limiter.go".into(),
            text: "package limiter\n\nfunc Allow() bool { return true }\n".into()
        }]
    );
    assert!(code.tests.is_empty());
    assert_eq!(code.suite, None);
    assert_eq!(code.domain, "rate limiting");
    assert_eq!(code.title, "A limiter");
    assert!(uuid::Uuid::parse_str(&code.drill_id).is_ok());
    assert_eq!(
        code.trail.calls,
        vec![ModelCall {
            purpose: "clean code".into(),
            model: MODEL.into()
        }]
    );
    assert!(code.trail.runs.is_empty());
    let schema = model.requests()[0].json_schema.clone().unwrap();
    assert!(schema["properties"].get("suite").is_none());
    Ok(())
}

#[tokio::test]
async fn a_runner_language_asks_for_a_suite_and_runs_it_on_the_code() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([clean_code_answer()]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    let code = build(&pool, &model, Some(&runner), &grader)
        .clean_code(&spec())
        .await?;
    assert_eq!(code.files, files());
    assert_eq!(code.suite.as_deref(), Some(SUITE));
    assert_eq!(runner.runs(), vec![(files(), SUITE.to_owned())]);
    assert_eq!(
        code.trail.runs,
        vec![RunRecord {
            what: "The suite on the code".into(),
            outcome: RunOutcome::Passed
        }]
    );
    let request = &model.requests()[0];
    let schema = request.json_schema.clone().unwrap();
    assert!(schema["required"]
        .as_array()
        .unwrap()
        .contains(&json!("suite")));
    let system = system(request);
    assert!(system.contains("Fake contract"));
    assert!(system.contains("checks.py"));
    assert!(system.contains("Every file is a .py file."));
    Ok(())
}

#[tokio::test]
async fn a_suite_that_fails_is_sent_back_once_with_the_output_and_then_accepted() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([clean_code_answer(), clean_code_answer()]);
    let runner = FakeRunner::scripted(vec![
        RunOutcome::Failed {
            output: "AssertionError: boom".into(),
        },
        RunOutcome::Passed,
    ]);
    let grader = idle_grader();
    let code = build(&pool, &model, Some(&runner), &grader)
        .clean_code(&spec())
        .await?;
    let requests = model.requests();
    assert_eq!(requests.len(), 2);
    let (first, second) = (messages(&requests[0]), messages(&requests[1]));
    assert_eq!(second[..first.len()], first[..], "the same conversation");
    assert_eq!(
        second[first.len()],
        ("assistant".to_owned(), clean_code_answer())
    );
    let reask = last(&requests[1]);
    assert!(
        reask.starts_with("That answer could not be used:"),
        "{reask}"
    );
    assert!(reask.contains("AssertionError: boom"), "{reask}");
    let purposes: Vec<&str> = code
        .trail
        .calls
        .iter()
        .map(|call| call.purpose.as_str())
        .collect();
    assert_eq!(purposes, ["clean code", "clean code (asked again)"]);
    let outcomes: Vec<RunOutcome> = code
        .trail
        .runs
        .iter()
        .map(|run| run.outcome.clone())
        .collect();
    assert_eq!(
        outcomes,
        [
            RunOutcome::Failed {
                output: "AssertionError: boom".into()
            },
            RunOutcome::Passed
        ]
    );
    Ok(())
}

#[tokio::test]
async fn a_suite_that_fails_twice_fails_the_stage_with_the_output() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([clean_code_answer(), clean_code_answer()]);
    let runner = FakeRunner::new(|_, _| RunOutcome::Failed {
        output: "AssertionError: boom".into(),
    });
    let grader = idle_grader();
    let error = build(&pool, &model, Some(&runner), &grader)
        .clean_code(&spec())
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AppError::ModelAnswerUnusable(reason) if reason.contains("AssertionError: boom")),
        "{error:?}"
    );
    assert_eq!(model.requests().len(), 2, "asked once more, never twice");
    assert_eq!(runner.runs().len(), 2);
    Ok(())
}

fn timed_out(output: &str) -> RunOutcome {
    RunOutcome::TimedOut {
        output: output.into(),
    }
}

#[tokio::test]
async fn a_suite_that_does_not_finish_is_sent_back_once_like_a_failure_with_what_it_wrote(
) -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([clean_code_answer(), clean_code_answer()]);
    let runner = FakeRunner::scripted(vec![timed_out("waiting 1\nwaiting 2"), RunOutcome::Passed]);
    let grader = idle_grader();
    let code = build(&pool, &model, Some(&runner), &grader)
        .clean_code(&spec())
        .await?;
    assert_eq!(code.files, files());
    let requests = model.requests();
    assert_eq!(requests.len(), 2);
    let (first, second) = (messages(&requests[0]), messages(&requests[1]));
    assert_eq!(second[..first.len()], first[..], "the same conversation");
    assert_eq!(
        second[first.len()],
        ("assistant".to_owned(), clean_code_answer())
    );
    let reask = last(&requests[1]);
    assert!(
        reask.starts_with("That answer could not be used:"),
        "{reask}"
    );
    assert!(
        reask.contains("The suite on the code did not finish within the runtime's time limit."),
        "{reask}"
    );
    assert!(reask.contains("waiting 2"), "{reask}");
    let outcomes: Vec<RunOutcome> = code
        .trail
        .runs
        .iter()
        .map(|run| run.outcome.clone())
        .collect();
    assert_eq!(
        outcomes,
        [timed_out("waiting 1\nwaiting 2"), RunOutcome::Passed]
    );
    let purposes: Vec<&str> = code
        .trail
        .calls
        .iter()
        .map(|call| call.purpose.as_str())
        .collect();
    assert_eq!(purposes, ["clean code", "clean code (asked again)"]);
    Ok(())
}

#[tokio::test]
async fn a_timeout_that_wrote_nothing_says_only_that_the_suite_did_not_finish() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([clean_code_answer(), clean_code_answer()]);
    let runner = FakeRunner::scripted(vec![timed_out(""), RunOutcome::Passed]);
    let grader = idle_grader();
    build(&pool, &model, Some(&runner), &grader)
        .clean_code(&spec())
        .await?;
    let reask = last(&model.requests()[1]);
    assert!(
        reask.contains("did not finish within the runtime's time limit"),
        "{reask}"
    );
    assert!(!reask.contains("What it wrote"), "{reask}");
    Ok(())
}

#[tokio::test]
async fn a_suite_that_does_not_finish_twice_fails_the_stage_as_an_unusable_answer() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([clean_code_answer(), clean_code_answer()]);
    let runner = FakeRunner::new(|_, _| timed_out("still going"));
    let grader = idle_grader();
    let error = build(&pool, &model, Some(&runner), &grader)
        .clean_code(&spec())
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AppError::ModelAnswerUnusable(reason)
            if reason.contains("did not finish within the runtime's time limit")
                && reason.contains("still going")),
        "{error:?}"
    );
    assert_eq!(model.requests().len(), 2, "asked once more, never twice");
    assert_eq!(runner.runs().len(), 2);
    Ok(())
}

#[tokio::test]
async fn a_failure_and_a_timeout_in_either_order_fail_the_stage() -> Result<()> {
    let failed = || RunOutcome::Failed {
        output: "AssertionError: boom".into(),
    };
    for (outcomes, fragment) in [
        (vec![failed(), timed_out("")], "time limit"),
        (vec![timed_out(""), failed()], "AssertionError: boom"),
    ] {
        let pool = pool().await?;
        let model = ScriptedModel::new([clean_code_answer(), clean_code_answer()]);
        let runner = FakeRunner::scripted(outcomes);
        let grader = idle_grader();
        let error = build(&pool, &model, Some(&runner), &grader)
            .clean_code(&spec())
            .await
            .unwrap_err();
        assert!(
            matches!(&error, AppError::ModelAnswerUnusable(reason) if reason.contains(fragment)),
            "{fragment}: {error:?}"
        );
        assert_eq!(model.requests().len(), 2);
    }
    Ok(())
}

#[tokio::test]
async fn a_runtime_that_is_not_there_is_an_unavailable_service_and_the_answer_is_not_blamed(
) -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([clean_code_answer(), clean_code_answer()]);
    let runner = FakeRunner::new(|_, _| RunOutcome::Unavailable {
        why: "The bundled Python runtime is missing.".into(),
    });
    let grader = idle_grader();
    let error = build(&pool, &model, Some(&runner), &grader)
        .clean_code(&spec())
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AppError::ServiceNotAvailable(why) if why.contains("bundled Python runtime is missing")),
        "{error:?}"
    );
    assert_eq!(model.requests().len(), 1, "nothing is asked again");
    Ok(())
}

#[tokio::test]
async fn a_cancelled_run_is_a_cancelled_build_and_the_answer_is_not_blamed() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([clean_code_answer(), clean_code_answer()]);
    let runner = FakeRunner::new(|_, _| RunOutcome::Cancelled);
    let grader = idle_grader();
    let error = build(&pool, &model, Some(&runner), &grader)
        .clean_code(&spec())
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AppError::InvalidState(message) if message.contains("cancelled")),
        "{error:?}"
    );
    assert_eq!(model.requests().len(), 1, "nothing is asked again");
    Ok(())
}

/// The model's first answer is `bad`, which has to be refused with a message holding `fragment`; the second is good.
async fn refused_then_accepted(bad: String, fragment: &str) -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([bad, clean_code_answer()]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    let code = build(&pool, &model, Some(&runner), &grader)
        .clean_code(&spec())
        .await?;
    assert_eq!(code.files, files(), "{fragment}");
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
async fn an_answer_a_drill_cannot_be_made_from_is_refused_with_what_to_change() -> Result<()> {
    let limiter = super::support::limiter();
    let config = super::support::config();
    let cases = [
        (
            answer(
                vec![
                    file("src/limiter.py", &limiter),
                    file("src/limiter.py", &config),
                ],
                Some(SUITE),
            ),
            "Two files have the path src/limiter.py",
        ),
        (
            answer(
                vec![file("src/limiter.py", &limiter), file("checks.py", &config)],
                Some(SUITE),
            ),
            "checks.py is the file the test suite runs as",
        ),
        (
            answer(
                vec![
                    file("src/limiter.js", &limiter),
                    file("src/config.py", &config),
                ],
                Some(SUITE),
            ),
            "src/limiter.js is not a .py file",
        ),
        (
            answer(vec![file("../limiter.py", &limiter)], Some(SUITE)),
            "The path ../limiter.py cannot be used",
        ),
        (
            answer(
                vec![
                    file("src/limiter.py", "  \n"),
                    file("src/config.py", &config),
                ],
                Some(SUITE),
            ),
            "src/limiter.py is empty",
        ),
        (
            answer(
                vec![
                    file("src/limiter.py", &limiter),
                    file("src/config.py", &config),
                ],
                None,
            ),
            "`suite` is empty",
        ),
        (
            answer(
                vec![
                    file("src/limiter.py", &limiter),
                    file("src/config.py", &config),
                ],
                Some(" \n"),
            ),
            "`suite` is empty",
        ),
        (answer(Vec::new(), Some(SUITE)), "There are no files"),
        ("{\"title\": \"cut off".to_owned(), "malformed"),
    ];
    for (bad, fragment) in cases {
        refused_then_accepted(bad, fragment).await?;
    }
    Ok(())
}

#[tokio::test]
async fn an_answer_refused_twice_fails_the_stage_with_the_second_reason() -> Result<()> {
    let pool = pool().await?;
    let limiter = super::support::limiter();
    let model = ScriptedModel::new([
        answer(vec![file("checks.py", &limiter)], Some(SUITE)),
        answer(vec![file("src/limiter.js", &limiter)], Some(SUITE)),
    ]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    let error = build(&pool, &model, Some(&runner), &grader)
        .clean_code(&spec())
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AppError::ModelAnswerUnusable(reason) if reason.contains("is not a .py file")),
        "{error:?}"
    );
    assert_eq!(model.requests().len(), 2);
    assert!(runner.runs().is_empty(), "nothing refused is run");
    Ok(())
}

#[tokio::test]
async fn a_testing_pattern_asks_for_a_test_file_that_is_part_of_the_code() -> Result<()> {
    let pool = pool().await?;
    let mut spec = spec();
    spec.patterns = vec!["testing.assertion_free_test".into()];
    let limiter = super::support::limiter();
    let without = answer(vec![file("src/limiter.py", &limiter)], Some(SUITE));
    let with = answer(
        vec![
            file("src/limiter.py", &limiter),
            json!({"path": "tests/test_limiter.py", "kind": "test", "text": "from src.limiter import RateLimiter\n"}),
        ],
        Some(SUITE),
    );
    let model = ScriptedModel::new([without, with]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    let code = build(&pool, &model, Some(&runner), &grader)
        .clean_code(&spec)
        .await?;
    assert_eq!(code.tests, ["tests/test_limiter.py"]);
    let requests = model.requests();
    assert!(system(&requests[0]).contains("Include a test file (kind `test`)"));
    assert!(last(&requests[1]).contains("There is no test file"));
    Ok(())
}

#[tokio::test]
async fn the_writer_is_told_nothing_of_the_patterns_to_plant() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([clean_code_answer()]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    build(&pool, &model, Some(&runner), &grader)
        .clean_code(&spec())
        .await?;
    let sent: String = messages(&model.requests()[0])
        .into_iter()
        .map(|(_, content)| content.to_lowercase())
        .collect();
    for word in ["plant", "mutat", OPERATOR, LITERALS, "wrong operator"] {
        assert!(!sent.contains(word), "{word}");
    }
    Ok(())
}

#[tokio::test]
async fn without_a_domain_the_bank_picks_the_one_used_least_recently() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([clean_code_answer()]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    let mut spec = spec();
    spec.domain = None;
    let code = build(&pool, &model, Some(&runner), &grader)
        .clean_code(&spec)
        .await?;
    assert_eq!(code.domain, bank::DOMAINS[0]);
    assert!(messages(&model.requests()[0])
        .iter()
        .any(|(_, content)| content.contains(&format!("Domain: {}", bank::DOMAINS[0]))));
    Ok(())
}
