//! Stage 1: the clean code, and in a runner language the suite that passes on it.

use super::support::{
    build, clean_code_answer, code_check_answer, code_spec, course, files, idle_grader, last,
    lesson_titled, messages, system, FakeRunner, ScriptedModel, CODE_NOT_TEACHES_REASON,
    CODE_TEACHES_REASON, LITERALS, MODEL, OPERATOR, SPACING_LESSON, SUITE,
};
use crate::drills::build::{CleanCode, ModelCall, RunRecord};
use crate::drills::runner::RunOutcome;
use crate::drills::schedules;
use crate::drills::spec::DrillSpec;
use crate::persistence::db;
use crate::tests::pool;
use lattice_core::domain::learning::drills::{DrillFile, Material};
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};
use sqlx::SqlitePool;

/// A spec for code in Go on a new course, built from its spacing lesson.
async fn go_spec(pool: &sqlx::SqlitePool) -> Result<DrillSpec> {
    Ok(DrillSpec {
        material: Material::code("go"),
        lines: 30,
        ..code_spec(pool).await?
    })
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
    let model = ScriptedModel::new([
        code_check_answer(true),
        json!({
            "title": "A limiter",
            "files": [file("limiter.go", "package limiter\r\n\r\nfunc Allow() bool { return true }\r\n")]
        })
        .to_string(),
    ]);
    let grader = idle_grader();
    let code = build(&pool, &model, None, &grader)
        .clean_code(&go_spec(&pool).await?)
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
    assert_eq!(code.domain, SPACING_LESSON);
    assert_eq!(code.title, "A limiter");
    assert!(uuid::Uuid::parse_str(&code.drill_id).is_ok());
    assert_eq!(
        code.trail.calls,
        vec![
            ModelCall {
                purpose: "code fit check".into(),
                model: MODEL.into()
            },
            ModelCall {
                purpose: "clean code".into(),
                model: MODEL.into()
            }
        ]
    );
    assert!(code.trail.runs.is_empty());
    let schema = model.requests()[1].json_schema.clone().unwrap();
    assert!(schema["properties"].get("suite").is_none());
    Ok(())
}

#[tokio::test]
async fn a_runner_language_asks_for_a_suite_and_runs_it_on_the_code() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([code_check_answer(true), clean_code_answer()]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    let code = build(&pool, &model, Some(&runner), &grader)
        .clean_code(&code_spec(&pool).await?)
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
    let request = &model.requests()[1];
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
    let model = ScriptedModel::new([
        code_check_answer(true),
        clean_code_answer(),
        clean_code_answer(),
    ]);
    let runner = FakeRunner::scripted(vec![
        RunOutcome::Failed {
            output: "AssertionError: boom".into(),
        },
        RunOutcome::Passed,
    ]);
    let grader = idle_grader();
    let code = build(&pool, &model, Some(&runner), &grader)
        .clean_code(&code_spec(&pool).await?)
        .await?;
    let requests = model.requests();
    assert_eq!(requests.len(), 3);
    let (first, second) = (messages(&requests[1]), messages(&requests[2]));
    assert_eq!(second[..first.len()], first[..], "the same conversation");
    assert_eq!(
        second[first.len()],
        ("assistant".to_owned(), clean_code_answer())
    );
    let reask = last(&requests[2]);
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
    assert_eq!(
        purposes,
        ["code fit check", "clean code", "clean code (asked again)"]
    );
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
    let model = ScriptedModel::new([
        code_check_answer(true),
        clean_code_answer(),
        clean_code_answer(),
    ]);
    let runner = FakeRunner::new(|_, _| RunOutcome::Failed {
        output: "AssertionError: boom".into(),
    });
    let grader = idle_grader();
    let error = build(&pool, &model, Some(&runner), &grader)
        .clean_code(&code_spec(&pool).await?)
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AppError::ModelAnswerUnusable(reason) if reason.contains("AssertionError: boom")),
        "{error:?}"
    );
    assert_eq!(model.requests().len(), 3, "asked once more, never twice");
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
    let model = ScriptedModel::new([
        code_check_answer(true),
        clean_code_answer(),
        clean_code_answer(),
    ]);
    let runner = FakeRunner::scripted(vec![timed_out("waiting 1\nwaiting 2"), RunOutcome::Passed]);
    let grader = idle_grader();
    let code = build(&pool, &model, Some(&runner), &grader)
        .clean_code(&code_spec(&pool).await?)
        .await?;
    assert_eq!(code.files, files());
    let requests = model.requests();
    assert_eq!(requests.len(), 3);
    let (first, second) = (messages(&requests[1]), messages(&requests[2]));
    assert_eq!(second[..first.len()], first[..], "the same conversation");
    assert_eq!(
        second[first.len()],
        ("assistant".to_owned(), clean_code_answer())
    );
    let reask = last(&requests[2]);
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
    assert_eq!(
        purposes,
        ["code fit check", "clean code", "clean code (asked again)"]
    );
    Ok(())
}

#[tokio::test]
async fn a_timeout_that_wrote_nothing_says_only_that_the_suite_did_not_finish() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([
        code_check_answer(true),
        clean_code_answer(),
        clean_code_answer(),
    ]);
    let runner = FakeRunner::scripted(vec![timed_out(""), RunOutcome::Passed]);
    let grader = idle_grader();
    build(&pool, &model, Some(&runner), &grader)
        .clean_code(&code_spec(&pool).await?)
        .await?;
    let reask = last(&model.requests()[2]);
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
    let model = ScriptedModel::new([
        code_check_answer(true),
        clean_code_answer(),
        clean_code_answer(),
    ]);
    let runner = FakeRunner::new(|_, _| timed_out("still going"));
    let grader = idle_grader();
    let error = build(&pool, &model, Some(&runner), &grader)
        .clean_code(&code_spec(&pool).await?)
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AppError::ModelAnswerUnusable(reason)
            if reason.contains("did not finish within the runtime's time limit")
                && reason.contains("still going")),
        "{error:?}"
    );
    assert_eq!(model.requests().len(), 3, "asked once more, never twice");
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
        let model = ScriptedModel::new([
            code_check_answer(true),
            clean_code_answer(),
            clean_code_answer(),
        ]);
        let runner = FakeRunner::scripted(outcomes);
        let grader = idle_grader();
        let error = build(&pool, &model, Some(&runner), &grader)
            .clean_code(&code_spec(&pool).await?)
            .await
            .unwrap_err();
        assert!(
            matches!(&error, AppError::ModelAnswerUnusable(reason) if reason.contains(fragment)),
            "{fragment}: {error:?}"
        );
        assert_eq!(model.requests().len(), 3);
    }
    Ok(())
}

#[tokio::test]
async fn a_runtime_that_is_not_there_is_an_unavailable_service_and_the_answer_is_not_blamed(
) -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([
        code_check_answer(true),
        clean_code_answer(),
        clean_code_answer(),
    ]);
    let runner = FakeRunner::new(|_, _| RunOutcome::Unavailable {
        why: "The bundled Python runtime is missing.".into(),
    });
    let grader = idle_grader();
    let error = build(&pool, &model, Some(&runner), &grader)
        .clean_code(&code_spec(&pool).await?)
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AppError::ServiceNotAvailable(why) if why.contains("bundled Python runtime is missing")),
        "{error:?}"
    );
    assert_eq!(model.requests().len(), 2, "nothing is asked again");
    Ok(())
}

#[tokio::test]
async fn a_cancelled_run_is_a_cancelled_build_and_the_answer_is_not_blamed() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([
        code_check_answer(true),
        clean_code_answer(),
        clean_code_answer(),
    ]);
    let runner = FakeRunner::new(|_, _| RunOutcome::Cancelled);
    let grader = idle_grader();
    let error = build(&pool, &model, Some(&runner), &grader)
        .clean_code(&code_spec(&pool).await?)
        .await
        .unwrap_err();
    assert!(matches!(&error, AppError::Cancelled(_)), "{error:?}");
    assert_eq!(model.requests().len(), 2, "nothing is asked again");
    Ok(())
}

/// The model's first answer is `bad`, which has to be refused with a message holding `fragment`; the second is good.
async fn refused_then_accepted(bad: String, fragment: &str) -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([code_check_answer(true), bad, clean_code_answer()]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    let code = build(&pool, &model, Some(&runner), &grader)
        .clean_code(&code_spec(&pool).await?)
        .await?;
    assert_eq!(code.files, files(), "{fragment}");
    let requests = model.requests();
    assert_eq!(requests.len(), 3, "{fragment}");
    let reask = last(&requests[2]);
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
        code_check_answer(true),
        answer(vec![file("checks.py", &limiter)], Some(SUITE)),
        answer(vec![file("src/limiter.js", &limiter)], Some(SUITE)),
    ]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    let error = build(&pool, &model, Some(&runner), &grader)
        .clean_code(&code_spec(&pool).await?)
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AppError::ModelAnswerUnusable(reason) if reason.contains("is not a .py file")),
        "{error:?}"
    );
    assert_eq!(model.requests().len(), 3);
    assert!(runner.runs().is_empty(), "nothing refused is run");
    Ok(())
}

#[tokio::test]
async fn a_testing_pattern_asks_for_a_test_file_that_is_part_of_the_code() -> Result<()> {
    let pool = pool().await?;
    let mut spec = code_spec(&pool).await?;
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
    let model = ScriptedModel::new([code_check_answer(true), without, with]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    let code = build(&pool, &model, Some(&runner), &grader)
        .clean_code(&spec)
        .await?;
    assert_eq!(code.tests, ["tests/test_limiter.py"]);
    let requests = model.requests();
    assert!(system(&requests[1]).contains("Include a test file (kind `test`)"));
    assert!(last(&requests[2]).contains("There is no test file"));
    Ok(())
}

#[tokio::test]
async fn the_writer_is_told_nothing_of_the_patterns_to_plant() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([code_check_answer(true), clean_code_answer()]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    build(&pool, &model, Some(&runner), &grader)
        .clean_code(&code_spec(&pool).await?)
        .await?;
    let sent: String = messages(&model.requests()[1])
        .into_iter()
        .map(|(_, content)| content.to_lowercase())
        .collect();
    for word in ["plant", "mutat", OPERATOR, LITERALS, "wrong operator"] {
        assert!(!sent.contains(word), "{word}");
    }
    Ok(())
}

#[tokio::test]
async fn the_writer_is_given_the_lessons_brief_and_no_domain() -> Result<()> {
    let pool = pool().await?;
    let spec = code_spec(&pool).await?;
    let model = ScriptedModel::new([code_check_answer(true), clean_code_answer()]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    build(&pool, &model, Some(&runner), &grader)
        .clean_code(&spec)
        .await?;
    let prompt = last(&model.requests()[1]);
    assert!(
        prompt.starts_with("Language: python\nSize: about 40 lines of code in all\n\n<brief>\nCourse: A generic learning program\n"),
        "{prompt}"
    );
    assert!(
        prompt.contains("\nLesson: Spacing study over sessions\nLesson objective: Explain why spreading study over several sessions improves recall\n"),
        "{prompt}"
    );
    assert!(prompt.ends_with("\n</brief>"), "{prompt}");
    assert!(!prompt.contains("Domain:"), "{prompt}");
    assert!(!prompt.contains("rate limiting"), "{prompt}");
    Ok(())
}

#[tokio::test]
async fn the_clean_stage_keeps_the_brief_the_writer_was_shown_and_no_fit_basis() -> Result<()> {
    let pool = pool().await?;
    let spec = code_spec(&pool).await?;
    let model = ScriptedModel::new([code_check_answer(true), clean_code_answer()]);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    let code = build(&pool, &model, Some(&runner), &grader)
        .clean_code(&spec)
        .await?;
    let brief = code.brief.clone().expect("the brief the writer was shown");
    assert!(last(&model.requests()[1]).contains(&brief.render()));
    assert_eq!(Some(brief.lesson_id.as_str()), spec.lesson_id.as_deref());
    assert_eq!(brief.lesson, SPACING_LESSON);
    assert_eq!(code.basis, None);
    assert_eq!(code.domain, SPACING_LESSON);
    Ok(())
}

#[tokio::test]
async fn a_code_spec_without_a_lesson_or_with_one_of_another_course_fails_before_any_model_call(
) -> Result<()> {
    let pool = pool().await?;
    let spec = code_spec(&pool).await?;
    let other = course(&pool).await?;
    let foreign = lesson_titled(&pool, &other, SPACING_LESSON).await?;
    let cases = [
        DrillSpec {
            lesson_id: None,
            ..spec.clone()
        },
        DrillSpec {
            program_id: None,
            ..spec.clone()
        },
        DrillSpec {
            lesson_id: Some(foreign),
            ..spec.clone()
        },
    ];
    for case in cases {
        let model = ScriptedModel::new([clean_code_answer()]);
        let runner = FakeRunner::passing();
        let grader = idle_grader();
        let error = build(&pool, &model, Some(&runner), &grader)
            .clean_code(&case)
            .await
            .unwrap_err();
        assert!(matches!(&error, AppError::InvalidInput(_)), "{error:?}");
        assert!(model.requests().is_empty(), "{case:?}");
    }
    Ok(())
}

// The code fit check.

/// What a lesson's code fit check is told of it: its module's title, its title and its objective.
async fn lesson_facts(pool: &SqlitePool, lesson_id: &str) -> Result<(String, String, String)> {
    sqlx::query_as(
        "SELECT m.title,l.title,l.objective FROM learning_lessons l JOIN learning_modules m ON m.id=l.module_id WHERE l.id=?",
    )
    .bind(lesson_id)
    .fetch_one(pool)
    .await
    .map_err(db)
}

/// The message the code fit check is sent for a lesson with `facts`.
fn check_message(facts: &(String, String, String)) -> String {
    format!(
        "Module: {}\nLesson: {}\nObjective: {}",
        facts.0, facts.1, facts.2
    )
}

/// The schema of the code fit check's answer.
fn check_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["reason", "lessonTeachesWhatCodeCanShow"],
        "properties": {
            "reason": {"type": "string", "minLength": 1},
            "lessonTeachesWhatCodeCanShow": {"type": "boolean"}
        }
    })
}

fn blank_reason_answer() -> String {
    json!({"reason": " ", "lessonTeachesWhatCodeCanShow": true}).to_string()
}

fn nothing_to_show(lesson: &str) -> String {
    format!("The lesson \"{lesson}\" teaches nothing a code drill can show.")
}

/// The verdicts stored for the lesson `lesson_id`: `teaches`, the reason and the model of each.
async fn verdicts(pool: &SqlitePool, lesson_id: &str) -> Result<Vec<(i64, String, String)>> {
    sqlx::query_as("SELECT teaches,reason,model FROM learning_drill_code_checks WHERE lesson_id=?")
        .bind(lesson_id)
        .fetch_all(pool)
        .await
        .map_err(db)
}

/// Runs the clean stage for `spec` on a model that answers `answers` in order, and returns the model and what the
/// stage came to.
async fn clean_with(
    pool: &SqlitePool,
    spec: &DrillSpec,
    answers: Vec<String>,
) -> (ScriptedModel, Result<CleanCode>) {
    let model = ScriptedModel::new(answers);
    let runner = FakeRunner::passing();
    let grader = idle_grader();
    let built = build(pool, &model, Some(&runner), &grader)
        .clean_code(spec)
        .await;
    (model, built)
}

async fn fits_code(pool: &SqlitePool, spec: &DrillSpec) -> Result<bool> {
    let mut connection = pool.acquire().await.map_err(db)?;
    schedules::lesson_fits_in(
        &mut connection,
        spec.course().expect("a course"),
        spec.lesson_id.as_deref().expect("a lesson"),
        &spec.material,
    )
    .await
}

#[tokio::test]
async fn before_the_code_is_written_the_lesson_is_asked_whether_it_teaches_what_code_can_show(
) -> Result<()> {
    let pool = pool().await?;
    let spec = code_spec(&pool).await?;
    let lesson = spec.lesson_id.clone().expect("a lesson");
    let facts = lesson_facts(&pool, &lesson).await?;
    let (model, built) = clean_with(
        &pool,
        &spec,
        vec![code_check_answer(true), clean_code_answer()],
    )
    .await;
    let code = built?;

    assert_eq!(model.purposes(), ["code fit check", "clean code"]);
    let request = &model.requests()[0];
    assert_eq!(last(request), check_message(&facts));
    assert_eq!(request.json_schema, Some(check_schema()));
    assert!(
        system(request).starts_with("You decide whether a lesson of a course"),
        "{}",
        system(request)
    );
    let calls: Vec<(&str, &str)> = code
        .trail
        .calls
        .iter()
        .map(|call| (call.purpose.as_str(), call.model.as_str()))
        .collect();
    assert_eq!(calls, [("code fit check", MODEL), ("clean code", MODEL)]);
    assert_eq!(
        verdicts(&pool, &lesson).await?,
        [(1, CODE_TEACHES_REASON.to_owned(), MODEL.to_owned())]
    );
    let basis: String =
        sqlx::query_scalar("SELECT basis FROM learning_drill_code_checks WHERE lesson_id=?")
            .bind(&lesson)
            .fetch_one(&pool)
            .await
            .map_err(db)?;
    assert!(!basis.trim().is_empty());
    assert!(fits_code(&pool, &spec).await?);
    Ok(())
}

#[tokio::test]
async fn a_lesson_that_teaches_nothing_code_can_show_gives_no_drill_and_writes_no_code(
) -> Result<()> {
    let pool = pool().await?;
    let spec = code_spec(&pool).await?;
    let lesson = spec.lesson_id.clone().expect("a lesson");
    let program = spec.course().expect("a course").to_owned();
    let (model, built) = clean_with(&pool, &spec, vec![code_check_answer(false)]).await;
    let error = built.unwrap_err();

    assert!(
        matches!(&error, AppError::InvalidInput(message) if *message == nothing_to_show(SPACING_LESSON)),
        "{error:?}"
    );
    assert_eq!(model.purposes(), ["code fit check"]);
    assert_eq!(model.unused(), 0);
    assert_eq!(
        verdicts(&pool, &lesson).await?,
        [(0, CODE_NOT_TEACHES_REASON.to_owned(), MODEL.to_owned())]
    );
    assert!(!fits_code(&pool, &spec).await?);
    let mut connection = pool.acquire().await.map_err(db)?;
    let states =
        schedules::lesson_states_in(&mut connection, &program, &spec.material, None).await?;
    let state = states
        .iter()
        .find(|state| state.lesson_id == lesson)
        .expect("the lesson");
    assert!(!state.fits);
    let passages =
        schedules::lesson_states_in(&mut connection, &program, &Material::Passage, None).await?;
    assert!(
        passages
            .iter()
            .find(|state| state.lesson_id == lesson)
            .expect("the lesson")
            .fits,
        "a lesson that teaches nothing code can show can still be explained"
    );
    Ok(())
}

#[tokio::test]
async fn a_lesson_judged_before_on_the_same_basis_is_not_asked_again() -> Result<()> {
    let pool = pool().await?;
    let spec = code_spec(&pool).await?;
    let lesson = spec.lesson_id.clone().expect("a lesson");
    let (_, first) = clean_with(
        &pool,
        &spec,
        vec![code_check_answer(true), clean_code_answer()],
    )
    .await;
    first?;

    let (model, second) = clean_with(&pool, &spec, vec![clean_code_answer()]).await;
    second?;
    assert_eq!(model.purposes(), ["clean code"]);
    assert_eq!(verdicts(&pool, &lesson).await?.len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_lesson_turned_down_before_on_the_same_basis_is_refused_without_a_call() -> Result<()> {
    let pool = pool().await?;
    let spec = code_spec(&pool).await?;
    let (_, first) = clean_with(&pool, &spec, vec![code_check_answer(false)]).await;
    first.unwrap_err();

    let (model, second) = clean_with(&pool, &spec, Vec::new()).await;
    let error = second.unwrap_err();
    assert!(
        matches!(&error, AppError::InvalidInput(message) if *message == nothing_to_show(SPACING_LESSON)),
        "{error:?}"
    );
    assert!(model.requests().is_empty());
    Ok(())
}

#[tokio::test]
async fn a_verdict_made_on_another_basis_is_asked_again() -> Result<()> {
    let pool = pool().await?;
    let spec = code_spec(&pool).await?;
    let lesson = spec.lesson_id.clone().expect("a lesson");
    let facts = lesson_facts(&pool, &lesson).await?;
    let (_, first) = clean_with(&pool, &spec, vec![code_check_answer(false)]).await;
    first.unwrap_err();
    sqlx::query("UPDATE learning_drill_code_checks SET basis='stale'")
        .execute(&pool)
        .await
        .map_err(db)?;

    let (model, second) = clean_with(
        &pool,
        &spec,
        vec![code_check_answer(true), clean_code_answer()],
    )
    .await;
    second?;
    assert_eq!(model.purposes(), ["code fit check", "clean code"]);
    assert_eq!(last(&model.requests()[0]), check_message(&facts));
    let row: (i64, String) =
        sqlx::query_as("SELECT teaches,basis FROM learning_drill_code_checks WHERE lesson_id=?")
            .bind(&lesson)
            .fetch_one(&pool)
            .await
            .map_err(db)?;
    assert_eq!(row.0, 1, "the old row was replaced");
    assert_ne!(row.1, "stale");
    Ok(())
}

#[tokio::test]
async fn changing_the_lessons_objective_asks_again() -> Result<()> {
    let pool = pool().await?;
    let spec = code_spec(&pool).await?;
    let lesson = spec.lesson_id.clone().expect("a lesson");
    let (_, first) = clean_with(
        &pool,
        &spec,
        vec![code_check_answer(true), clean_code_answer()],
    )
    .await;
    first?;
    let (module, title, objective) = lesson_facts(&pool, &lesson).await?;
    let changed = format!("{objective}. Gaps matter most.");
    sqlx::query("UPDATE learning_lessons SET objective=? WHERE id=?")
        .bind(&changed)
        .bind(&lesson)
        .execute(&pool)
        .await
        .map_err(db)?;

    let (model, second) = clean_with(
        &pool,
        &spec,
        vec![code_check_answer(true), clean_code_answer()],
    )
    .await;
    second?;
    assert_eq!(model.purposes(), ["code fit check", "clean code"]);
    assert_eq!(
        last(&model.requests()[0]),
        check_message(&(module, title, changed))
    );
    Ok(())
}

#[tokio::test]
async fn a_blank_reason_is_asked_for_again_once() -> Result<()> {
    let pool = pool().await?;
    let spec = code_spec(&pool).await?;
    let lesson = spec.lesson_id.clone().expect("a lesson");
    let (model, built) = clean_with(
        &pool,
        &spec,
        vec![
            blank_reason_answer(),
            code_check_answer(true),
            clean_code_answer(),
        ],
    )
    .await;
    built?;

    assert_eq!(
        model.purposes(),
        ["code fit check", "code fit check", "clean code"]
    );
    let again = last(&model.requests()[1]);
    assert!(
        again.contains("The reason is empty. Give your reason first."),
        "{again}"
    );
    assert_eq!(verdicts(&pool, &lesson).await?.len(), 1);
    Ok(())
}

#[tokio::test]
async fn an_answer_refused_twice_fails_the_clean_stage_and_stores_nothing() -> Result<()> {
    let pool = pool().await?;
    let spec = code_spec(&pool).await?;
    let lesson = spec.lesson_id.clone().expect("a lesson");
    let (model, built) = clean_with(
        &pool,
        &spec,
        vec![blank_reason_answer(), blank_reason_answer()],
    )
    .await;
    let error = built.unwrap_err();

    assert!(
        matches!(error, AppError::ModelAnswerUnusable(_)),
        "{error:?}"
    );
    assert_eq!(model.purposes(), ["code fit check", "code fit check"]);
    assert!(verdicts(&pool, &lesson).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_failing_code_fit_check_call_fails_the_clean_stage_and_stores_nothing() -> Result<()> {
    let pool = pool().await?;
    let spec = code_spec(&pool).await?;
    let lesson = spec.lesson_id.clone().expect("a lesson");
    let (model, built) = clean_with(&pool, &spec, Vec::new()).await;
    let error = built.unwrap_err();

    assert!(
        matches!(&error, AppError::InternalError(message) if message.contains("Unexpected extra model call")),
        "{error:?}"
    );
    assert_eq!(model.purposes(), ["code fit check"]);
    assert!(verdicts(&pool, &lesson).await?.is_empty());
    Ok(())
}
