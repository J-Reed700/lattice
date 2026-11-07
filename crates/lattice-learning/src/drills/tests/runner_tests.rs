//! The runner adapter: what a runtime's result means for a test, the entrypoint that runs a test file, which
//! languages get a runner, and the real built-in runtimes on small modules.

use crate::drills::runner::{
    outcome_of, runner_for, BuiltinRunner, CodeRunner, RunOutcome,
};
use crate::embedded_runtime::{BuiltinRuntimes, LearningBuiltinRuntime};
use crate::lab_runtime::{LearningLabExecutionResult, LearningLabRunStatus};

fn result(status: LearningLabRunStatus, stdout: &str, stderr: &str) -> LearningLabExecutionResult {
    LearningLabExecutionResult {
        status,
        exit_code: None,
        stdout: stdout.into(),
        stderr: stderr.into(),
        output_truncated: false,
        duration_ms: 0,
    }
}

#[test]
fn only_a_run_that_finished_says_anything_about_the_code() {
    assert_eq!(
        outcome_of(&result(LearningLabRunStatus::Passed, "ok\n", "")),
        RunOutcome::Passed
    );
    assert_eq!(
        outcome_of(&result(
            LearningLabRunStatus::Failed,
            "partial\n",
            "Traceback\nAssertionError\n"
        )),
        RunOutcome::Failed {
            output: "partial\nTraceback\nAssertionError".into()
        },
        "what the test wrote, then what the runtime wrote"
    );
    assert_eq!(
        outcome_of(&result(
            LearningLabRunStatus::Failed,
            "",
            "AssertionError\n"
        )),
        RunOutcome::Failed {
            output: "AssertionError".into()
        }
    );
    assert_eq!(
        outcome_of(&result(
            LearningLabRunStatus::TimedOut,
            "so far\n",
            "Run exceeded its time limit and was stopped.\n"
        )),
        RunOutcome::TimedOut {
            output: "so far\nRun exceeded its time limit and was stopped.".into()
        },
        "a timeout keeps what the run wrote, apart from a failure"
    );
    assert_eq!(
        outcome_of(&result(
            LearningLabRunStatus::Cancelled,
            "",
            "Run cancelled.\n"
        )),
        RunOutcome::Cancelled
    );
    assert_eq!(
        outcome_of(&result(
            LearningLabRunStatus::RuntimeUnavailable,
            "",
            "The bundled Python runtime is missing.\n"
        )),
        RunOutcome::Unavailable {
            why: "The bundled Python runtime is missing.".into()
        }
    );
    assert_eq!(
        outcome_of(&result(LearningLabRunStatus::RuntimeUnavailable, "", "")),
        RunOutcome::Unavailable {
            why: "The runtime is not available.".into()
        }
    );
}

#[test]
fn a_timeout_a_cancellation_and_a_missing_runtime_say_nothing_about_the_code() {
    assert_eq!(RunOutcome::Passed.says_nothing(), None);
    assert_eq!(
        RunOutcome::Failed {
            output: "boom".into()
        }
        .says_nothing(),
        None
    );
    assert!(RunOutcome::TimedOut {
        output: String::new()
    }
    .says_nothing()
    .is_some_and(|why| why.contains("time limit")));
    assert!(RunOutcome::Cancelled
        .says_nothing()
        .is_some_and(|why| why.contains("cancelled")));
    assert_eq!(
        RunOutcome::Unavailable {
            why: "The runtime is gone.".into()
        }
        .says_nothing(),
        Some("The runtime is gone.".to_owned())
    );
}

#[test]
fn a_test_file_is_run_as_a_program_by_a_one_line_entrypoint() {
    let runtimes = BuiltinRuntimes::for_tests();
    let python = BuiltinRunner::new(runtimes.clone(), LearningBuiltinRuntime::Python);
    assert_eq!(python.name(), "builtin-python");
    assert_eq!(python.entrypoint(), "checks.py");
    assert!(python
        .running("tests/test_limiter.py")
        .contains("runpy.run_path(os.path.join(os.path.dirname(__file__), \"tests/test_limiter.py\"), run_name=\"__main__\")"));
    assert!(
        python
            .running("tests/a\"b.py")
            .contains("\"tests/a\\\"b.py\""),
        "a path is written as a string literal"
    );

    let javascript = BuiltinRunner::new(runtimes, LearningBuiltinRuntime::Javascript);
    assert_eq!(javascript.name(), "builtin-javascript");
    assert_eq!(javascript.entrypoint(), "checks.mjs");
    assert_eq!(
        javascript.running("tests/limiter_test.mjs"),
        "import \"./tests/limiter_test.mjs\";\n"
    );
}

#[test]
fn only_a_language_whose_runtime_is_available_gets_a_runner() {
    let runtimes = BuiltinRuntimes::for_tests();
    for language in ["rust", "go", "typescript", "csharp", "c", "shell", "sql"] {
        assert!(runner_for(language, &runtimes).is_none(), "{language}");
    }
    for (language, runtime) in [
        ("python", LearningBuiltinRuntime::Python),
        ("javascript", LearningBuiltinRuntime::Javascript),
    ] {
        let available = runtimes
            .capabilities()
            .iter()
            .any(|capability| capability.id == runtime && capability.available);
        assert_eq!(
            runner_for(language, &runtimes).is_some(),
            available,
            "{language}"
        );
    }
    // Without the bundled interpreter Python has no runner.
    assert!(runner_for("python", &BuiltinRuntimes::new(None)).is_none());
}

#[cfg(feature = "learning-labs")]
mod real_runtimes {
    use super::super::support::{files, operator_edit, KILLING_TEST, SUITE};
    use crate::drills::runner::{runner_for, CodeRunner, RunOutcome};
    use crate::embedded_runtime::BuiltinRuntimes;
    use lattice_core::domain::learning::drills::{apply, DrillFile};
    use lattice_core::shared::error::{AppError, Result};
    use tokio_util::sync::CancellationToken;

    const JS_LIMITER: &str = "export class RateLimiter {\n  constructor(limit, window, clock) {\n    this.limit = limit;\n    this.window = window;\n    this.clock = clock;\n    this.hits = new Map();\n  }\n\n  allow(key) {\n    const now = this.clock();\n    const recent = (this.hits.get(key) ?? []).filter((t) => now - t < this.window);\n    if (recent.length >= this.limit) {\n      this.hits.set(key, recent);\n      return false;\n    }\n    recent.push(now);\n    this.hits.set(key, recent);\n    return true;\n  }\n}\n";

    const JS_SUITE: &str = "import { RateLimiter } from './src/limiter.mjs';\n\nlet now = 0;\nconst limiter = new RateLimiter(2, 10, () => now);\nif (!limiter.allow('a')) throw new Error('the first call must be allowed');\nif (!limiter.allow('a')) throw new Error('the second call must be allowed');\nif (limiter.allow('a')) throw new Error('the third call must be refused');\nnow = 11;\nif (!limiter.allow('a')) throw new Error('a call after the window must be allowed');\nconsole.log('ok');\n";

    const JS_KILLING: &str = "import { RateLimiter } from './src/limiter.mjs';\n\nconst limiter = new RateLimiter(1, 10, () => 0);\nif (!limiter.allow('a')) throw new Error('the first call must be allowed');\nif (limiter.allow('a')) throw new Error('a second call inside the window must be refused');\n";

    const JS_TEST_FILE: &str = "import { RateLimiter } from '../src/limiter.mjs';\n\nconst limiter = new RateLimiter(1, 10, () => 0);\nif (!limiter.allow('a')) throw new Error('the first call must be allowed');\nif (limiter.allow('a')) throw new Error('the second call must be refused');\n";

    const PY_TEST_FILE: &str = "from src.limiter import RateLimiter\n\nlimiter = RateLimiter(1, 10, lambda: 0)\nassert limiter.allow(\"a\")\nassert not limiter.allow(\"a\"), \"the second call must be refused\"\n";

    fn javascript(limiter: &str) -> Vec<DrillFile> {
        vec![DrillFile {
            path: "src/limiter.mjs".into(),
            text: limiter.into(),
        }]
    }

    fn with(files: &[DrillFile], path: &str, text: &str) -> Vec<DrillFile> {
        let mut more = files.to_vec();
        more.push(DrillFile {
            path: path.into(),
            text: text.into(),
        });
        more
    }

    fn failed_with(outcome: &RunOutcome, fragment: &str) -> bool {
        matches!(outcome, RunOutcome::Failed { output } if output.contains(fragment))
    }

    #[tokio::test]
    async fn a_javascript_suite_and_a_killing_test_run_on_the_built_in_runtime() -> Result<()> {
        let runtimes = BuiltinRuntimes::for_tests();
        let runner = runner_for("javascript", &runtimes)
            .ok_or_else(|| AppError::InternalError("JavaScript has no runner here".into()))?;
        let clean = javascript(JS_LIMITER);
        let mutated = javascript(
            &JS_LIMITER.replace("recent.length >= this.limit", "recent.length > this.limit"),
        );
        let token = CancellationToken::new;
        assert_eq!(
            runner.run(&clean, JS_SUITE, token()).await?,
            RunOutcome::Passed
        );
        assert_eq!(
            runner.run(&clean, JS_KILLING, token()).await?,
            RunOutcome::Passed
        );
        let killed = runner.run(&mutated, JS_KILLING, token()).await?;
        assert!(
            failed_with(&killed, "a second call inside the window must be refused"),
            "{killed:?}"
        );
        // The suite itself also sees the mutation: one call too many gets through.
        assert!(matches!(
            runner.run(&mutated, JS_SUITE, token()).await?,
            RunOutcome::Failed { .. }
        ));
        Ok(())
    }

    #[tokio::test]
    async fn a_javascript_test_file_runs_as_a_program_beside_the_code() -> Result<()> {
        let runtimes = BuiltinRuntimes::for_tests();
        let runner = runner_for("javascript", &runtimes)
            .ok_or_else(|| AppError::InternalError("JavaScript has no runner here".into()))?;
        let clean = with(
            &javascript(JS_LIMITER),
            "tests/limiter_test.mjs",
            JS_TEST_FILE,
        );
        let entry = runner.running("tests/limiter_test.mjs");
        assert_eq!(
            runner.run(&clean, &entry, CancellationToken::new()).await?,
            RunOutcome::Passed
        );
        let weak = with(
            &javascript(JS_LIMITER),
            "tests/limiter_test.mjs",
            &JS_TEST_FILE.replace("RateLimiter(1, 10", "RateLimiter(5, 10"),
        );
        let outcome = runner.run(&weak, &entry, CancellationToken::new()).await?;
        assert!(
            failed_with(&outcome, "the second call must be refused"),
            "{outcome:?}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_run_cancelled_before_it_starts_proves_nothing() -> Result<()> {
        let runtimes = BuiltinRuntimes::for_tests();
        let runner = runner_for("javascript", &runtimes)
            .ok_or_else(|| AppError::InternalError("JavaScript has no runner here".into()))?;
        let token = CancellationToken::new();
        token.cancel();
        let outcome = runner.run(&javascript(JS_LIMITER), JS_SUITE, token).await?;
        assert_eq!(outcome, RunOutcome::Cancelled);
        Ok(())
    }

    #[tokio::test]
    async fn a_python_suite_and_a_killing_test_run_on_the_built_in_runtime() -> Result<()> {
        let runtimes = BuiltinRuntimes::for_tests();
        let runner = runner_for("python", &runtimes)
            .ok_or_else(|| AppError::InternalError("Python has no runner here".into()))?;
        let clean = files();
        let (mutated, _) = apply(&clean, &[operator_edit()]).unwrap();
        let token = CancellationToken::new;
        let suite = runner.run(&clean, SUITE, token()).await?;
        assert_eq!(suite, RunOutcome::Passed, "{suite:?}");
        assert_eq!(
            runner.run(&clean, KILLING_TEST, token()).await?,
            RunOutcome::Passed
        );
        let killed = runner.run(&mutated, KILLING_TEST, token()).await?;
        assert!(
            failed_with(&killed, "a second call inside the window must be refused"),
            "{killed:?}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_python_test_file_runs_as_a_program_beside_the_code() -> Result<()> {
        let runtimes = BuiltinRuntimes::for_tests();
        let runner = runner_for("python", &runtimes)
            .ok_or_else(|| AppError::InternalError("Python has no runner here".into()))?;
        let entry = runner.running("tests/test_limiter.py");
        let clean = with(&files(), "tests/test_limiter.py", PY_TEST_FILE);
        let outcome = runner.run(&clean, &entry, CancellationToken::new()).await?;
        assert_eq!(outcome, RunOutcome::Passed, "{outcome:?}");
        let weak = with(
            &files(),
            "tests/test_limiter.py",
            &PY_TEST_FILE.replace("RateLimiter(1, 10", "RateLimiter(5, 10"),
        );
        let outcome = runner.run(&weak, &entry, CancellationToken::new()).await?;
        assert!(
            failed_with(&outcome, "the second call must be refused"),
            "{outcome:?}"
        );
        Ok(())
    }
}
