//! Grading a learner's fix: a Strategy for each key issue, the hidden suite once for the whole fix, the fall back to the
//! judge when a run proves nothing, and the real built-in runtime.

use super::support::{
    fix_answer, last, messages, operator_fixed, proved, FakeRunner, ScriptedModel, KILLING_TEST,
    LITERALS_AFTER, LITERALS_BEFORE, OPERATOR_AFTER, SUITE,
};
use crate::features::learning::drills::fix_grading::{choose, grade_fix, FixCase, StrategyKind};
use crate::features::learning::drills::proof::Tools;
use crate::features::learning::drills::runner::{CodeRunner, RunOutcome};
use crate::features::learning::practice::practice_tests::text_length_rules;
use async_trait::async_trait;
use lattice_core::domain::learning::drills::{
    DecidedBy, Drill, DrillFile, FixOutcome, IssueFix, Proof, SuiteVerdict,
};
use lattice_core::shared::error::{AppError, Result};
use tokio_util::sync::CancellationToken;

const OPERATOR_OUTPUT: &str = "AssertionError: a second call inside the window must be refused";

/// The drill the learner reviewed: the stocked one, with a killed issue and a judged one.
fn drill() -> Drill {
    let proved = proved();
    Drill {
        id: "drill".into(),
        language: "python".into(),
        title: "A sliding-window rate limiter".into(),
        files: proved.files.clone(),
        key: proved.planted_key(),
    }
}

/// The drill's files, which a learner who changed nothing hands back.
fn unchanged() -> Vec<DrillFile> {
    drill().files
}

/// The files with the literals put back as the key's fix has them, on top of the operator fix.
fn both_fixed() -> Vec<DrillFile> {
    operator_fixed()
        .into_iter()
        .map(|file| DrillFile {
            text: file.text.replace(LITERALS_AFTER, LITERALS_BEFORE),
            path: file.path,
        })
        .collect()
}

const SUITE_OUTPUT: &str = "ImportError: cannot import name 'default_limiter'";

/// A runner whose suite fails on any files, and whose killing test fails on code with the operator changed.
fn runner_with_a_broken_suite() -> FakeRunner {
    FakeRunner::new(|files, test| {
        if test == SUITE {
            return RunOutcome::Failed {
                output: SUITE_OUTPUT.into(),
            };
        }
        let mutated = files
            .iter()
            .any(|file| file.text.contains(OPERATOR_AFTER.trim()));
        if test == KILLING_TEST && mutated {
            return RunOutcome::Failed {
                output: OPERATOR_OUTPUT.into(),
            };
        }
        RunOutcome::Passed
    })
}

fn ran(runner: &FakeRunner, test: &str) -> usize {
    runner.runs().iter().filter(|(_, ran)| ran == test).count()
}

async fn grade(
    model: &ScriptedModel,
    runner: Option<&FakeRunner>,
    suite: Option<&str>,
    drill: &Drill,
    learner: &[DrillFile],
) -> Result<Vec<IssueFix>> {
    let cancel = CancellationToken::new();
    let tools = Tools {
        llm: model,
        runner: runner.map(|runner| runner as &dyn CodeRunner),
        cancel: &cancel,
    };
    grade_fix(
        tools,
        &FixCase {
            drill,
            learner,
            suite,
        },
    )
    .await
}

#[test]
fn an_issue_proven_by_a_killing_test_is_checked_by_it_and_any_other_is_judged() {
    let key = drill().key;
    assert_eq!(choose(&key[0]), StrategyKind::KillingTest);
    assert_eq!(choose(&key[1]), StrategyKind::Judged);
}

#[tokio::test]
async fn a_killing_test_that_passes_on_the_learners_files_fixes_its_issue() -> Result<()> {
    let model = ScriptedModel::new([fix_answer("not", "The literals are still written out.")]);
    let runner = FakeRunner::killing_the_operator();
    let learner = operator_fixed();
    let results = grade(&model, Some(&runner), Some(SUITE), &drill(), &learner).await?;

    let first = &results[0];
    assert_eq!(first.issue, 0);
    assert_eq!(first.outcome, FixOutcome::Fixed);
    assert_eq!(first.decided_by, DecidedBy::Test);
    assert_eq!(first.output, None);
    assert_eq!(first.fallback, None);
    assert_eq!(first.suite, SuiteVerdict::Passed);
    let runs = runner.runs();
    assert_eq!(runs.len(), 2, "the suite once and the one killing test");
    assert!(
        runs.iter().all(|(files, _)| *files == learner),
        "every run is on the learner's files"
    );
    assert_eq!(
        runs.iter()
            .map(|(_, test)| test.as_str())
            .collect::<Vec<_>>(),
        [SUITE, KILLING_TEST]
    );
    assert_eq!(
        model.purposes(),
        ["fix judge"],
        "only the judged issue goes to the judge"
    );
    Ok(())
}

#[tokio::test]
async fn a_killing_test_that_fails_on_the_learners_files_leaves_its_issue_unfixed_with_its_output(
) -> Result<()> {
    let model = ScriptedModel::new([fix_answer("not", "The literals are still written out.")]);
    let runner = FakeRunner::killing_the_operator();
    let results = grade(&model, Some(&runner), Some(SUITE), &drill(), &unchanged()).await?;

    let first = &results[0];
    assert_eq!(first.outcome, FixOutcome::NotFixed);
    assert_eq!(first.decided_by, DecidedBy::Test);
    assert_eq!(first.output.as_deref(), Some(OPERATOR_OUTPUT));
    assert!(first.reason.contains("still fails"), "{}", first.reason);
    assert_eq!(first.fallback, None);
    Ok(())
}

#[tokio::test]
async fn an_issue_proven_by_judgement_is_judged_from_the_code_the_issue_and_the_keys_fix(
) -> Result<()> {
    let model = ScriptedModel::new([fix_answer(
        "fixed",
        "The learner uses the two constants again.",
    )]);
    let runner = FakeRunner::passing();
    let mut learner = both_fixed();
    learner[1].text.push_str("# the learner's own note\n");
    let results = grade(&model, Some(&runner), Some(SUITE), &drill(), &learner).await?;

    let second = &results[1];
    assert_eq!(second.issue, 1);
    assert_eq!(second.outcome, FixOutcome::Fixed);
    assert_eq!(second.decided_by, DecidedBy::Judge);
    assert_eq!(second.reason, "The learner uses the two constants again.");
    assert_eq!(second.output, None);
    assert_eq!(second.fallback, None);

    let requests = model.requests();
    assert_eq!(requests.len(), 1);
    let sent = messages(&requests[0]);
    assert!(sent[0].1.contains("You judge whether a learner's change"));
    let task = last(&requests[0]);
    for (shown, what) in [
        ("readability.magic_number", "the issue's pattern"),
        ("src/config.py line 8", "where the issue is"),
        (
            "The default limit and window are written twice, so they can drift.",
            "what goes wrong",
        ),
        (LITERALS_AFTER, "the code as it was reviewed"),
        (LITERALS_BEFORE, "the key's fix"),
        ("# the learner's own note", "the learner's code"),
    ] {
        assert!(task.contains(shown), "the judge reads {what}: {task}");
    }
    Ok(())
}

#[tokio::test]
async fn the_judge_gives_its_reasons_before_its_outcome_and_its_schema_caps_nothing() -> Result<()>
{
    let model = ScriptedModel::new([fix_answer("partly", "Only the window is a constant.")]);
    let runner = FakeRunner::passing();
    let results = grade(&model, Some(&runner), Some(SUITE), &drill(), &both_fixed()).await?;
    assert_eq!(results[1].outcome, FixOutcome::Partly);

    let requests = model.requests();
    let schema = requests[0].json_schema.as_ref().expect("a structured call");
    assert!(text_length_rules(schema).is_empty(), "{schema}");
    assert_eq!(schema["additionalProperties"], false);
    let properties: Vec<&str> = schema["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(properties, ["reason", "outcome"]);
    assert_eq!(schema["required"], serde_json::json!(["reason", "outcome"]));
    assert_eq!(
        schema["properties"]["outcome"]["enum"],
        serde_json::json!(["fixed", "partly", "not"])
    );
    Ok(())
}

#[tokio::test]
async fn code_of_any_length_reaches_the_judge_whole() -> Result<()> {
    let model = ScriptedModel::new([fix_answer("not", "Nothing changed there.")]);
    let runner = FakeRunner::passing();
    let long = "x = 1\n".repeat(20_000);
    let mut learner = unchanged();
    learner[1].text.push_str(&long);
    grade(&model, Some(&runner), Some(SUITE), &drill(), &learner).await?;
    let task = last(&model.requests()[0]);
    assert!(task.contains(&long));
    Ok(())
}

#[tokio::test]
async fn the_judge_is_asked_again_once_when_its_answer_is_refused() -> Result<()> {
    let model = ScriptedModel::new([
        fix_answer("fixed", "   "),
        fix_answer("fixed", "The constants are used."),
    ]);
    let runner = FakeRunner::passing();
    let results = grade(&model, Some(&runner), Some(SUITE), &drill(), &both_fixed()).await?;
    assert_eq!(results[1].reason, "The constants are used.");
    let requests = model.requests();
    assert_eq!(requests.len(), 2);
    assert!(
        last(&requests[1]).contains("The reasons are empty"),
        "the second request says what was wrong"
    );
    Ok(())
}

#[tokio::test]
async fn a_judge_that_gives_no_usable_answer_twice_is_an_error_and_never_a_guess() -> Result<()> {
    let model = ScriptedModel::new([fix_answer("fixed", ""), fix_answer("fixed", " ")]);
    let runner = FakeRunner::passing();
    let result = grade(&model, Some(&runner), Some(SUITE), &drill(), &both_fixed()).await;
    assert!(
        matches!(result, Err(AppError::ModelAnswerUnusable(_))),
        "{result:?}"
    );
    assert_eq!(model.requests().len(), 2, "asked again once, no more");
    Ok(())
}

#[tokio::test]
async fn with_no_runtime_every_issue_is_judged_and_a_killing_test_is_said_to_have_been_missed(
) -> Result<()> {
    let model = ScriptedModel::new([
        fix_answer("fixed", "The comparison is back to the limit."),
        fix_answer("not", "The literals are as they were."),
    ]);
    let results = grade(&model, None, Some(SUITE), &drill(), &operator_fixed()).await?;

    assert_eq!(model.purposes(), ["fix judge", "fix judge"]);
    assert_eq!(results[0].decided_by, DecidedBy::Judge);
    assert_eq!(results[0].outcome, FixOutcome::Fixed);
    assert!(
        results[0]
            .fallback
            .as_deref()
            .is_some_and(|why| why.contains("no runtime")),
        "the result says why the test did not decide: {:?}",
        results[0].fallback
    );
    assert_eq!(
        results[1].fallback, None,
        "a judged issue was never a test's"
    );
    for result in &results {
        assert!(
            matches!(&result.suite, SuiteVerdict::NotRun { why } if why.contains("no runtime")),
            "{:?}",
            result.suite
        );
    }
    Ok(())
}

#[tokio::test]
async fn in_a_language_with_no_runner_the_whole_fix_is_judged_and_nothing_is_run() -> Result<()> {
    let mut judged = drill();
    judged.key[0].proof = Proof::Judged {
        reason: "The edit changes the comparison.".into(),
    };
    let model = ScriptedModel::new([
        fix_answer("fixed", "The comparison is back."),
        fix_answer("partly", "One literal is a constant."),
    ]);
    let results = grade(&model, None, None, &judged, &operator_fixed()).await?;

    assert_eq!(
        results
            .iter()
            .map(|result| (result.decided_by, result.outcome, result.fallback.clone()))
            .collect::<Vec<_>>(),
        [
            (DecidedBy::Judge, FixOutcome::Fixed, None),
            (DecidedBy::Judge, FixOutcome::Partly, None),
        ]
    );
    for result in &results {
        assert!(
            matches!(&result.suite, SuiteVerdict::NotRun { why } if why.contains("no hidden suite")),
            "{:?}",
            result.suite
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_suite_the_fix_breaks_is_carried_by_every_issue_with_its_output() -> Result<()> {
    let model = ScriptedModel::new([fix_answer("fixed", "The constants are used.")]);
    let runner = runner_with_a_broken_suite();
    let results = grade(&model, Some(&runner), Some(SUITE), &drill(), &both_fixed()).await?;
    assert_eq!(results.len(), 2);
    for result in &results {
        assert_eq!(
            result.suite,
            SuiteVerdict::Failed {
                output: SUITE_OUTPUT.into()
            }
        );
        assert_eq!(result.outcome, FixOutcome::Partly, "{result:?}");
        assert_eq!(result.decided_by, DecidedBy::Suite, "{result:?}");
        assert_eq!(result.output.as_deref(), Some(SUITE_OUTPUT));
        assert!(
            result.reason.contains("hidden suite fails"),
            "{}",
            result.reason
        );
    }
    assert_eq!(
        ran(&runner, SUITE),
        1,
        "the suite runs once for the whole fix"
    );
    assert_eq!(ran(&runner, KILLING_TEST), 1);
    Ok(())
}

#[tokio::test]
async fn an_issue_that_is_not_fixed_stays_so_when_the_suite_is_broken_too() -> Result<()> {
    let model = ScriptedModel::new([fix_answer("not", "The literals are as they were.")]);
    let runner = runner_with_a_broken_suite();
    let results = grade(&model, Some(&runner), Some(SUITE), &drill(), &unchanged()).await?;
    assert_eq!(results[0].outcome, FixOutcome::NotFixed);
    assert_eq!(results[0].decided_by, DecidedBy::Test);
    assert_eq!(results[0].output.as_deref(), Some(OPERATOR_OUTPUT));
    assert!(matches!(results[0].suite, SuiteVerdict::Failed { .. }));
    assert!(matches!(results[1].suite, SuiteVerdict::Failed { .. }));
    Ok(())
}

#[tokio::test]
async fn a_fix_that_passes_the_suite_is_fixed_by_test() -> Result<()> {
    let model = ScriptedModel::new([fix_answer("fixed", "The constants are used.")]);
    let runner = FakeRunner::killing_the_operator();
    let results = grade(&model, Some(&runner), Some(SUITE), &drill(), &both_fixed()).await?;
    assert_eq!(
        results
            .iter()
            .map(|result| (result.outcome, result.decided_by))
            .collect::<Vec<_>>(),
        [
            (FixOutcome::Fixed, DecidedBy::Test),
            (FixOutcome::Fixed, DecidedBy::Judge)
        ]
    );
    Ok(())
}

#[tokio::test]
async fn a_test_that_runs_out_of_time_proves_nothing_so_the_judge_decides_and_the_result_says_so(
) -> Result<()> {
    let model = ScriptedModel::new([
        fix_answer("fixed", "The comparison is back to the limit."),
        fix_answer("not", "The literals are as they were."),
    ]);
    let runner = FakeRunner::scripted(vec![
        RunOutcome::Passed,
        RunOutcome::TimedOut {
            output: "so far".into(),
        },
    ]);
    let results = grade(
        &model,
        Some(&runner),
        Some(SUITE),
        &drill(),
        &operator_fixed(),
    )
    .await?;

    assert_eq!(results[0].decided_by, DecidedBy::Judge);
    assert_eq!(results[0].outcome, FixOutcome::Fixed);
    assert_eq!(results[0].output, None);
    assert!(
        results[0]
            .fallback
            .as_deref()
            .is_some_and(|why| why.contains("time limit")),
        "{:?}",
        results[0].fallback
    );
    assert_eq!(model.purposes(), ["fix judge", "fix judge"]);
    Ok(())
}

#[tokio::test]
async fn a_runtime_that_is_not_there_proves_nothing_either() -> Result<()> {
    let model = ScriptedModel::new([
        fix_answer("not", "The comparison is still wrong."),
        fix_answer("not", "The literals are as they were."),
    ]);
    let gone = RunOutcome::Unavailable {
        why: "The runtime is gone.".into(),
    };
    let runner = FakeRunner::scripted(vec![gone.clone(), gone]);
    let results = grade(&model, Some(&runner), Some(SUITE), &drill(), &unchanged()).await?;

    assert_eq!(results[0].decided_by, DecidedBy::Judge);
    assert_eq!(results[0].fallback.as_deref(), Some("The runtime is gone."));
    for result in &results {
        assert_eq!(
            result.suite,
            SuiteVerdict::NotRun {
                why: "The runtime is gone.".into()
            }
        );
    }
    Ok(())
}

/// A runner whose runs all fail with an error of the runtime's own.
struct Refusing;

#[async_trait]
impl CodeRunner for Refusing {
    fn name(&self) -> &str {
        "refusing"
    }
    fn entrypoint(&self) -> &str {
        "checks.py"
    }
    fn contract(&self) -> &str {
        ""
    }
    fn running(&self, path: &str) -> String {
        format!("run {path}\n")
    }
    async fn run(
        &self,
        _files: &[DrillFile],
        _test: &str,
        _cancel: CancellationToken,
    ) -> Result<RunOutcome> {
        Err(AppError::InvalidInput(
            "Lab files exceed the 2 MiB run limit.".into(),
        ))
    }
}

#[tokio::test]
async fn files_the_runtime_refuses_prove_nothing_so_the_judge_decides() -> Result<()> {
    let model = ScriptedModel::new([
        fix_answer("fixed", "The comparison is back to the limit."),
        fix_answer("not", "The literals are as they were."),
    ]);
    let cancel = CancellationToken::new();
    let tools = Tools {
        llm: &model,
        runner: Some(&Refusing as &dyn CodeRunner),
        cancel: &cancel,
    };
    let learner = operator_fixed();
    let drill = drill();
    let results = grade_fix(
        tools,
        &FixCase {
            drill: &drill,
            learner: &learner,
            suite: Some(SUITE),
        },
    )
    .await?;
    assert_eq!(results[0].decided_by, DecidedBy::Judge);
    assert!(
        results[0]
            .fallback
            .as_deref()
            .is_some_and(|why| why.contains("2 MiB")),
        "{:?}",
        results[0].fallback
    );
    assert!(
        matches!(&results[0].suite, SuiteVerdict::NotRun { why } if why.contains("2 MiB")),
        "{:?}",
        results[0].suite
    );
    Ok(())
}

#[tokio::test]
async fn a_cancelled_run_stops_the_grading_and_grades_nothing() -> Result<()> {
    let model = ScriptedModel::new(Vec::<String>::new());
    let runner = FakeRunner::scripted(vec![RunOutcome::Cancelled]);
    let result = grade(
        &model,
        Some(&runner),
        Some(SUITE),
        &drill(),
        &operator_fixed(),
    )
    .await;
    assert!(
        matches!(result, Err(AppError::InvalidState(_))),
        "{result:?}"
    );
    assert!(model.requests().is_empty());
    Ok(())
}

#[tokio::test]
async fn the_results_follow_the_key_in_order_one_for_each_issue() -> Result<()> {
    let model = ScriptedModel::new([fix_answer("not", "As they were.")]);
    let runner = FakeRunner::killing_the_operator();
    let results = grade(&model, Some(&runner), Some(SUITE), &drill(), &unchanged()).await?;
    assert_eq!(
        results
            .iter()
            .map(|result| result.issue)
            .collect::<Vec<_>>(),
        [0, 1]
    );
    Ok(())
}

#[tokio::test]
async fn a_key_with_no_issue_has_no_result_and_runs_only_the_suite() -> Result<()> {
    let mut bare = drill();
    bare.key.clear();
    let model = ScriptedModel::new(Vec::<String>::new());
    let runner = FakeRunner::passing();
    let results = grade(&model, Some(&runner), Some(SUITE), &bare, &unchanged()).await?;
    assert!(results.is_empty());
    assert_eq!(runner.runs().len(), 1);
    Ok(())
}

#[cfg(feature = "learning-labs")]
mod real_runtime {
    use super::*;
    use crate::features::learning::drills::runner::runner_for;
    use crate::features::learning::embedded_runtime::BuiltinRuntimes;

    fn python() -> Result<Box<dyn CodeRunner>> {
        runner_for("python", &BuiltinRuntimes::for_tests())
            .ok_or_else(|| AppError::InternalError("Python has no runner here".into()))
    }

    async fn grade_on(
        runner: &dyn CodeRunner,
        model: &ScriptedModel,
        learner: &[DrillFile],
    ) -> Result<Vec<IssueFix>> {
        let cancel = CancellationToken::new();
        let tools = Tools {
            llm: model,
            runner: Some(runner),
            cancel: &cancel,
        };
        let drill = drill();
        grade_fix(
            tools,
            &FixCase {
                drill: &drill,
                learner,
                suite: Some(SUITE),
            },
        )
        .await
    }

    #[tokio::test]
    async fn a_fix_that_repairs_the_operator_passes_the_killing_test_and_the_suite_on_python(
    ) -> Result<()> {
        let runner = python()?;
        let model = ScriptedModel::new([fix_answer("not", "The literals are as they were.")]);
        let results = grade_on(runner.as_ref(), &model, &operator_fixed()).await?;
        assert_eq!(results[0].outcome, FixOutcome::Fixed, "{:?}", results[0]);
        assert_eq!(results[0].decided_by, DecidedBy::Test);
        assert_eq!(results[0].suite, SuiteVerdict::Passed);
        Ok(())
    }

    #[tokio::test]
    async fn a_fix_that_changes_nothing_fails_the_killing_test_and_the_suite_on_python(
    ) -> Result<()> {
        let runner = python()?;
        let model = ScriptedModel::new([fix_answer("not", "The literals are as they were.")]);
        let results = grade_on(runner.as_ref(), &model, &unchanged()).await?;
        assert_eq!(results[0].outcome, FixOutcome::NotFixed);
        assert_eq!(results[0].decided_by, DecidedBy::Test);
        assert!(
            results[0].output.as_deref().is_some_and(
                |output| output.contains("a second call inside the window must be refused")
            ),
            "{:?}",
            results[0].output
        );
        assert!(
            matches!(&results[0].suite, SuiteVerdict::Failed { output } if output.contains("AssertionError")),
            "{:?}",
            results[0].suite
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_fix_that_breaks_what_the_suite_uses_is_only_partly_fixed_on_python() -> Result<()> {
        let runner = python()?;
        let model = ScriptedModel::new([fix_answer("fixed", "The constants are used.")]);
        let learner: Vec<DrillFile> = both_fixed()
            .into_iter()
            .map(|file| DrillFile {
                text: file
                    .text
                    .replace("def default_limiter(clock):", "def make_limiter(clock):"),
                path: file.path,
            })
            .collect();
        let results = grade_on(runner.as_ref(), &model, &learner).await?;
        assert_eq!(results[0].outcome, FixOutcome::Partly, "{:?}", results[0]);
        assert_eq!(results[0].decided_by, DecidedBy::Suite);
        assert!(
            matches!(&results[0].suite, SuiteVerdict::Failed { output } if output.contains("default_limiter")),
            "{:?}",
            results[0].suite
        );
        assert_eq!(results[1].outcome, FixOutcome::Partly);
        Ok(())
    }
}
