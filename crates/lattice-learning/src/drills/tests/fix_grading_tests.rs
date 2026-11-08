//! Grading a learner's fix: a Strategy for each key issue, the hidden suite once for the whole fix, the fall back to the
//! judge when a run proves nothing, and the real built-in runtime.

use super::support::{
    fix_answer, last, messages, operator_fixed, proved, raised_all, FakeRunner, ScriptedModel,
    KILLING_TEST, LITERALS_AFTER, LITERALS_BEFORE, OPERATOR_AFTER, SUITE,
};
use crate::drills::fix_grading::{choose, grade_fix, FixCase, StrategyKind};
use crate::drills::material::CodeMaterial;
use crate::drills::proof::Tools;
use crate::drills::runner::{CodeRunner, RunOutcome};
use crate::practice::practice_tests::text_length_rules;
use async_trait::async_trait;
use lattice_core::domain::learning::drills::{
    Assessment, DecidedBy, Drill, DrillFile, FixOutcome, IssueFix, IssueOutcome, Material, Origin,
    Proof, SuiteVerdict,
};
use lattice_core::shared::error::{AppError, Result};
use tokio_util::sync::CancellationToken;

const OPERATOR_OUTPUT: &str = "AssertionError: a second call inside the window must be refused";

/// The drill the learner reviewed: the stocked one, with a killed issue and a judged one.
fn drill() -> Drill {
    let proved = proved();
    Drill {
        id: "drill".into(),
        material: Material::code("python"),
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
    grade_assessed(model, runner, suite, drill, learner, &raised_all(drill)).await
}

async fn grade_assessed(
    model: &ScriptedModel,
    runner: Option<&FakeRunner>,
    suite: Option<&str>,
    drill: &Drill,
    learner: &[DrillFile],
    assessment: &Assessment,
) -> Result<Vec<IssueFix>> {
    let cancel = CancellationToken::new();
    let tools = Tools {
        llm: model,
        runner: runner.map(|runner| runner as &dyn CodeRunner),
        cancel: &cancel,
    };
    grade_fix(
        &CodeMaterial::new(tools.runner),
        tools,
        &FixCase {
            drill,
            learner,
            suite,
            source: None,
            assessment,
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
    assert_eq!(
        first.suite,
        SuiteVerdict::Passed {
            ran_beside: vec![1]
        }
    );
    let runs = runner.runs();
    assert_eq!(runs.len(), 2, "the suite once and the one killing test");
    assert_eq!(
        runs[0].0,
        both_fixed(),
        "the suite runs beside the fix of the planted issue the learner left"
    );
    assert_eq!(
        runs[1].0,
        both_fixed(),
        "the killing test runs beside the fix of the other issue, which the learner left"
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
    let shown = serde_json::to_value(&results).expect("results are JSON");
    assert_eq!(
        shown[1]["gradedBeside"],
        serde_json::json!([]),
        "a judge decided it, beside nothing"
    );

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
                output: SUITE_OUTPUT.into(),
                ran_beside: Vec::new()
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
    assert_eq!(
        serde_json::to_value(&results).expect("results are JSON")[0]["gradedBeside"],
        serde_json::json!([]),
        "the judge read the learner's code as it is, beside nothing"
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
        &CodeMaterial::new(tools.runner),
        tools,
        &FixCase {
            drill: &drill,
            learner: &learner,
            suite: Some(SUITE),
            source: None,
            assessment: &raised_all(&drill),
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

/// The stocked drill with its second issue, the literals, found in review, and an assessment in which the learner
/// found the operator and `literals` says what became of the other, when it says anything.
fn with_a_review_found_issue(literals: Option<IssueOutcome>) -> (Drill, Assessment) {
    let mut drill = drill();
    drill.key[1].origin = Origin::FoundInReview;
    let mut issues = vec![(IssueOutcome::Found { finding: 0 }, "Found.".to_owned())];
    issues.extend(literals.map(|outcome| (outcome, "Said.".to_owned())));
    let assessment = Assessment {
        issues,
        findings: Vec::new(),
    };
    (drill, assessment)
}

fn positions(results: &[IssueFix]) -> Vec<usize> {
    results.iter().map(|result| result.issue).collect()
}

#[tokio::test]
async fn a_review_found_issue_the_learner_did_not_raise_gets_no_result_and_no_model_call(
) -> Result<()> {
    for literals in [Some(IssueOutcome::Missed), None] {
        let (drill, assessment) = with_a_review_found_issue(literals);
        let model = ScriptedModel::new(Vec::<String>::new());
        let runner = FakeRunner::passing();
        let results = grade_assessed(
            &model,
            Some(&runner),
            Some(SUITE),
            &drill,
            &unchanged(),
            &assessment,
        )
        .await?;
        assert_eq!(positions(&results), [0], "{literals:?}");
        assert!(model.requests().is_empty(), "{literals:?}");
    }
    Ok(())
}

#[tokio::test]
async fn a_review_found_issue_the_learner_raised_gets_a_result() -> Result<()> {
    for raised in [
        IssueOutcome::Found { finding: 0 },
        IssueOutcome::HalfFound { finding: 0 },
    ] {
        let (drill, assessment) = with_a_review_found_issue(Some(raised));
        let model = ScriptedModel::new([fix_answer("not", "The literals are as they were.")]);
        let runner = FakeRunner::passing();
        let results = grade_assessed(
            &model,
            Some(&runner),
            Some(SUITE),
            &drill,
            &unchanged(),
            &assessment,
        )
        .await?;
        assert_eq!(positions(&results), [0, 1], "{raised:?}");
        assert_eq!(model.requests().len(), 1, "{raised:?}");
    }
    Ok(())
}

#[tokio::test]
async fn a_planted_issue_is_graded_whatever_the_assessment_says_of_it() -> Result<()> {
    let drill = drill();
    let missed_both = Assessment {
        issues: vec![
            (IssueOutcome::Missed, "Not named.".to_owned()),
            (IssueOutcome::Missed, "Not named.".to_owned()),
        ],
        findings: Vec::new(),
    };
    let no_outcomes = Assessment {
        issues: Vec::new(),
        findings: Vec::new(),
    };
    for assessment in [&missed_both, &no_outcomes] {
        let model = ScriptedModel::new([fix_answer("not", "The literals are as they were.")]);
        let runner = FakeRunner::passing();
        let results = grade_assessed(
            &model,
            Some(&runner),
            Some(SUITE),
            &drill,
            &unchanged(),
            assessment,
        )
        .await?;
        assert_eq!(positions(&results), [0, 1]);
        assert_eq!(model.requests().len(), 1, "the judged planted issue");
    }
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
    use crate::drills::runner::runner_for;
    use crate::embedded_runtime::BuiltinRuntimes;

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
            &CodeMaterial::new(tools.runner),
            tools,
            &FixCase {
                drill: &drill,
                learner,
                suite: Some(SUITE),
                source: None,
                assessment: &raised_all(&drill),
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
        assert_eq!(
            results[0].suite,
            SuiteVerdict::Passed {
                ran_beside: vec![1]
            }
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_fix_that_changes_nothing_fails_the_killing_test_and_breaks_nothing_on_python(
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
        // Both defects are repaired for the suite, which passes: nothing the learner did broke what worked.
        assert_eq!(
            results[0].suite,
            SuiteVerdict::Passed {
                ran_beside: vec![0, 1]
            }
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
            matches!(&results[0].suite, SuiteVerdict::Failed { output, .. } if output.contains("default_limiter")),
            "{:?}",
            results[0].suite
        );
        assert_eq!(results[1].outcome, FixOutcome::Partly);
        Ok(())
    }
}

/// Grading each issue on its own: the key's fixes of the planted issues are applied to the learner's files for an
/// issue's killing test, and for the hidden suite, where they apply, so that a defect the learner left in one issue
/// cannot fail the test of another or the suite. A fix found in review is never applied.
mod beside {
    use super::super::support::{
        files, literals_edit, operator_edit, LITERALS, OPERATOR, OPERATOR_BEFORE,
    };
    use super::*;
    use lattice_core::domain::learning::drills::{
        apply, check_key, Edit, KeyIssue, LineSpan, Origin, Severity,
    };
    use serde_json::json;

    /// The test that shows the literals' defect.
    const OTHER_TEST: &str = "assert default_limiter(lambda: 0).limit == 100\n";
    const OTHER_OUTPUT: &str = "AssertionError: the defaults are written out twice";
    /// `forget` as the clean code has it, and as the defect planted in it leaves it: it raises for a key never seen.
    const FORGET_CLEAN: &str = "        self.hits.pop(key, None)\n";
    const FORGET_FLAWED: &str = "        del self.hits[key]\n";
    /// A comment the blind review found misleading, and the line that replaces it.
    const DOC_FLAWED: &str =
        "    \"\"\"Allows at most `limit` calls per key in a sliding window.\"\"\"\n";
    const DOC_FIXED: &str =
        "    \"\"\"Allows at most `limit` calls per key within `window` seconds.\"\"\"\n";
    /// The literals' line as a learner rewrites it in a way of their own.
    const REWRITTEN: &str = "    return RateLimiter(120, 30, clock)\n";

    fn rewritten(files: &[DrillFile], change: impl Fn(&str) -> String) -> Vec<DrillFile> {
        files
            .iter()
            .map(|file| DrillFile {
                path: file.path.clone(),
                text: change(&file.text),
            })
            .collect()
    }

    fn reversed(edit: &Edit) -> Edit {
        Edit {
            path: edit.path.clone(),
            before: edit.after.clone(),
            after: edit.before.clone(),
        }
    }

    fn key_issue(
        id: &str,
        pattern: &str,
        span: LineSpan,
        fix: Edit,
        proof: Proof,
        origin: Origin,
    ) -> KeyIssue {
        KeyIssue {
            id: id.into(),
            pattern: pattern.into(),
            severity: Severity::Major,
            span,
            consequence: format!("{id} goes wrong."),
            fix,
            explanation: format!("Say what is wrong with {id}."),
            proof,
            origin,
        }
    }

    /// The stocked drill with its second issue proven by a killing test too.
    fn two_killed() -> Drill {
        let mut drill = drill();
        drill.key[1].proof = Proof::Killed {
            test: OTHER_TEST.into(),
        };
        drill
    }

    /// A drill of four issues: the operator, the literals and `forget`, which were planted, and a misleading comment,
    /// which the blind review found. The operator and the literals have killing tests. Returns the clean code beside
    /// the drill.
    fn four_issues() -> (Vec<DrillFile>, Drill) {
        let clean = files();
        let planted = [
            operator_edit(),
            literals_edit(),
            Edit {
                path: "src/limiter.py".into(),
                before: FORGET_CLEAN.into(),
                after: FORGET_FLAWED.into(),
            },
        ];
        let (drilled, spans) = apply(&clean, &planted).expect("the edits apply");
        let comment = Edit {
            path: "src/limiter.py".into(),
            before: DOC_FLAWED.into(),
            after: DOC_FIXED.into(),
        };
        let key = vec![
            key_issue(
                "planted-1",
                OPERATOR,
                spans[0].clone(),
                reversed(&planted[0]),
                Proof::Killed {
                    test: KILLING_TEST.into(),
                },
                Origin::Planted,
            ),
            key_issue(
                "planted-2",
                LITERALS,
                spans[1].clone(),
                reversed(&planted[1]),
                Proof::Killed {
                    test: OTHER_TEST.into(),
                },
                Origin::Planted,
            ),
            key_issue(
                "planted-3",
                "correctness.boundary_and_empty_input",
                spans[2].clone(),
                reversed(&planted[2]),
                Proof::Judged {
                    reason: "Forget raises for a key it never saw.".into(),
                },
                Origin::Planted,
            ),
            key_issue(
                "review-1",
                "readability.misleading_comment",
                LineSpan {
                    path: "src/limiter.py".into(),
                    start: 2,
                    end: 2,
                },
                comment,
                Proof::Judged {
                    reason: "The comment leaves out the unit of the window.".into(),
                },
                Origin::FoundInReview,
            ),
        ];
        let drill = Drill {
            id: "drill".into(),
            material: Material::code("python"),
            title: "A sliding-window rate limiter".into(),
            files: drilled,
            key,
        };
        (clean, drill)
    }

    /// The files with the defect planted in `forget` repaired.
    fn with_forget_repaired(files: &[DrillFile]) -> Vec<DrillFile> {
        rewritten(files, |text| text.replace(FORGET_FLAWED, FORGET_CLEAN))
    }

    /// A runner on which either defect fails the first killing test, the literals' defect fails the second, and the
    /// suite passes.
    fn either_defect_fails_the_first_test() -> FakeRunner {
        FakeRunner::new(|files, test| {
            let has = |text: &str| files.iter().any(|file| file.text.contains(text));
            let failed = match test {
                KILLING_TEST => has(OPERATOR_AFTER.trim()) || has(LITERALS_AFTER.trim()),
                OTHER_TEST => has(LITERALS_AFTER.trim()),
                _ => false,
            };
            if failed {
                RunOutcome::Failed {
                    output: OTHER_OUTPUT.into(),
                }
            } else {
                RunOutcome::Passed
            }
        })
    }

    /// A runner on which either defect fails the suite and the first killing test, the literals' defect fails the
    /// second killing test, and a rename of `default_limiter` fails the suite.
    fn defects_and_a_rename_fail_the_suite() -> FakeRunner {
        FakeRunner::new(|files, test| {
            let has = |text: &str| files.iter().any(|file| file.text.contains(text));
            let operator = has(OPERATOR_AFTER.trim());
            let literals = has(LITERALS_AFTER.trim());
            let failed = match test {
                KILLING_TEST => operator || literals,
                OTHER_TEST => literals,
                SUITE => operator || literals || has("def make_limiter"),
                _ => false,
            };
            if failed {
                RunOutcome::Failed {
                    output: OTHER_OUTPUT.into(),
                }
            } else {
                RunOutcome::Passed
            }
        })
    }

    fn shown(results: &[IssueFix]) -> serde_json::Value {
        serde_json::to_value(results).expect("results are JSON")
    }

    #[test]
    fn the_four_issue_drill_is_a_valid_drill() {
        let (clean, drill) = four_issues();
        assert_eq!(check_key(&clean, &drill), Ok(()));
    }

    #[tokio::test]
    async fn an_issue_is_graded_beside_the_fixes_of_the_issues_the_learner_left() -> Result<()> {
        let model = ScriptedModel::new(Vec::<String>::new());
        let runner = either_defect_fails_the_first_test();
        let learner = operator_fixed();
        let results = grade(&model, Some(&runner), Some(SUITE), &two_killed(), &learner).await?;

        // The learner fixed the operator and left the literals. The literals' defect fails the operator's test on
        // their files as written, and does not decide it.
        assert_eq!(results[0].outcome, FixOutcome::Fixed, "{:?}", results[0]);
        assert_eq!(results[0].decided_by, DecidedBy::Test);
        assert_eq!(results[1].outcome, FixOutcome::NotFixed, "{:?}", results[1]);
        assert_eq!(results[1].decided_by, DecidedBy::Test);
        assert_eq!(results[1].output.as_deref(), Some(OTHER_OUTPUT));

        let runs = runner.runs();
        assert!(
            runs.contains(&(both_fixed(), KILLING_TEST.to_owned())),
            "the first test ran beside the fix of the second issue"
        );
        assert!(
            runs.contains(&(learner, OTHER_TEST.to_owned())),
            "the second test ran on the learner's files, where the first issue is fixed already"
        );
        Ok(())
    }

    #[tokio::test]
    async fn the_result_says_which_fixes_the_issue_was_graded_beside() -> Result<()> {
        let model = ScriptedModel::new(Vec::<String>::new());
        let runner = either_defect_fails_the_first_test();
        let results = grade(
            &model,
            Some(&runner),
            Some(SUITE),
            &two_killed(),
            &operator_fixed(),
        )
        .await?;
        let shown = shown(&results);
        assert_eq!(shown[0]["gradedBeside"], json!([1]));
        assert_eq!(shown[1]["gradedBeside"], json!([]));
        Ok(())
    }

    #[tokio::test]
    async fn the_suite_runs_once_beside_the_fixes_of_the_planted_issues_the_learner_left(
    ) -> Result<()> {
        let model = ScriptedModel::new(Vec::<String>::new());
        let runner = either_defect_fails_the_first_test();
        let results = grade(
            &model,
            Some(&runner),
            Some(SUITE),
            &two_killed(),
            &operator_fixed(),
        )
        .await?;
        let suite_runs: Vec<Vec<DrillFile>> = runner
            .runs()
            .into_iter()
            .filter(|(_, test)| test == SUITE)
            .map(|(files, _)| files)
            .collect();
        assert_eq!(suite_runs, [both_fixed()]);
        assert_eq!(shown(&results)[0]["suite"]["ranBeside"], json!([1]));
        Ok(())
    }

    #[tokio::test]
    async fn a_defect_the_learner_left_does_not_make_the_suite_demote_the_issues_they_fixed(
    ) -> Result<()> {
        let model = ScriptedModel::new(Vec::<String>::new());
        let runner = defects_and_a_rename_fail_the_suite();
        let results = grade(
            &model,
            Some(&runner),
            Some(SUITE),
            &two_killed(),
            &operator_fixed(),
        )
        .await?;

        // The learner fixed one of the two planted issues. The other's defect fails the suite on their files as
        // written, and it is theirs to fix, not something their edits broke.
        assert_eq!(results[0].outcome, FixOutcome::Fixed, "{:?}", results[0]);
        assert_eq!(results[0].decided_by, DecidedBy::Test);
        let shown = shown(&results);
        assert_eq!(
            shown[0]["suite"],
            json!({"status": "passed", "ranBeside": [1]})
        );
        assert_eq!(
            shown[1]["suite"], shown[0]["suite"],
            "every result carries the one verdict"
        );
        Ok(())
    }

    #[tokio::test]
    async fn an_edit_of_the_learners_that_breaks_something_else_still_demotes_the_issues_they_fixed(
    ) -> Result<()> {
        let model = ScriptedModel::new(Vec::<String>::new());
        let runner = defects_and_a_rename_fail_the_suite();
        // The learner fixes the operator and renames the function the suite imports.
        let learner = rewritten(&operator_fixed(), |text| {
            text.replace("def default_limiter(clock):", "def make_limiter(clock):")
        });
        let results = grade(&model, Some(&runner), Some(SUITE), &two_killed(), &learner).await?;

        assert_eq!(results[0].outcome, FixOutcome::Partly, "{:?}", results[0]);
        assert_eq!(results[0].decided_by, DecidedBy::Suite);
        assert_eq!(results[0].output.as_deref(), Some(OTHER_OUTPUT));
        let shown = shown(&results);
        assert_eq!(shown[0]["suite"]["status"], "failed");
        assert_eq!(
            shown[0]["suite"]["ranBeside"],
            json!([1]),
            "the suite still fails beside the literals' fix, on the learner's own rename"
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_fix_for_lines_the_learner_changed_is_not_applied() -> Result<()> {
        let (_, drill) = four_issues();
        // The learner fixes the operator, rewrites the literals in a way of their own, and leaves `forget` and the
        // comment.
        let learner = rewritten(&drill.files, |text| {
            text.replace(OPERATOR_AFTER, OPERATOR_BEFORE)
                .replace(LITERALS_AFTER, REWRITTEN)
        });
        let model = ScriptedModel::new([
            fix_answer("not", "Forget still raises."),
            fix_answer("not", "The comment is as it was."),
        ]);
        let runner = FakeRunner::passing();
        let results = grade(&model, Some(&runner), Some(SUITE), &drill, &learner).await?;

        let beside = with_forget_repaired(&learner);
        assert!(
            beside.iter().any(|file| file.text.contains(REWRITTEN)),
            "the literals are as the learner left them"
        );
        let runs = runner.runs();
        for (name, test) in [
            ("the first test", KILLING_TEST),
            ("the second test", OTHER_TEST),
            ("the suite", SUITE),
        ] {
            assert!(
                runs.contains(&(beside.clone(), test.to_owned())),
                "{name} ran beside the repair of forget and not beside the key's fix for the literals"
            );
        }
        let shown = shown(&results);
        assert_eq!(shown[0]["gradedBeside"], json!([2]));
        assert_eq!(shown[1]["gradedBeside"], json!([2]));
        assert_eq!(shown[0]["suite"]["ranBeside"], json!([2]));
        Ok(())
    }

    #[tokio::test]
    async fn a_fix_for_lines_that_are_in_the_text_twice_is_not_applied() -> Result<()> {
        let (_, drill) = four_issues();
        let mut learner = rewritten(&drill.files, |text| {
            text.replace(OPERATOR_AFTER, OPERATOR_BEFORE)
        });
        learner[1].text.push_str(LITERALS_AFTER);
        let model = ScriptedModel::new([
            fix_answer("not", "Forget still raises."),
            fix_answer("not", "The comment is as it was."),
        ]);
        let runner = FakeRunner::passing();
        let results = grade(&model, Some(&runner), Some(SUITE), &drill, &learner).await?;

        let runs = runner.runs();
        assert!(
            runs.contains(&(with_forget_repaired(&learner), KILLING_TEST.to_owned())),
            "the key's fix for the literals does not say which of two places it means"
        );
        assert_eq!(shown(&results)[0]["gradedBeside"], json!([2]));
        Ok(())
    }

    #[tokio::test]
    async fn a_fix_found_in_review_is_not_applied_beside_the_test_of_a_planted_issue() -> Result<()>
    {
        let (_, drill) = four_issues();
        // The learner changed nothing.
        let learner = drill.files.clone();
        let model = ScriptedModel::new([
            fix_answer("not", "Forget raises."),
            fix_answer("not", "The comment is as it was."),
        ]);
        let runner = FakeRunner::passing();
        let results = grade(&model, Some(&runner), Some(SUITE), &drill, &learner).await?;

        // The comment was the review's finding. Its repair is never made for a planted issue's test, which passed on
        // the clean code with the comment as it was, nor for the suite.
        let runs = runner.runs();
        assert_eq!(runs.len(), 3, "the suite and the two killing tests");
        for (files, _) in &runs {
            assert!(
                files.iter().all(|file| !file.text.contains(DOC_FIXED)),
                "{files:?}"
            );
        }
        let beside = rewritten(&learner, |text| {
            text.replace(LITERALS_AFTER, LITERALS_BEFORE)
                .replace(FORGET_FLAWED, FORGET_CLEAN)
        });
        assert!(runs.contains(&(beside, KILLING_TEST.to_owned())));
        let shown = shown(&results);
        assert_eq!(shown[0]["gradedBeside"], json!([1, 2]));
        assert_eq!(shown[1]["gradedBeside"], json!([0, 2]));
        assert_eq!(shown[0]["suite"]["ranBeside"], json!([0, 1, 2]));
        Ok(())
    }

    #[tokio::test]
    async fn a_fix_that_touches_the_lines_of_the_issue_is_not_applied_to_its_test() -> Result<()> {
        let mut drill = drill();
        // A planted issue whose fix repairs the operator's line along with the lines around it. The build makes no
        // such key, and the rule holds all the same.
        let around: String = drill.files[0]
            .text
            .lines()
            .skip(11)
            .take(3)
            .map(|line| format!("{line}\n"))
            .collect();
        let repaired = around.replace(OPERATOR_AFTER.trim_end(), OPERATOR_BEFORE.trim_end());
        drill.key.push(key_issue(
            "planted-3",
            "correctness.stale_state",
            LineSpan {
                path: "src/limiter.py".into(),
                start: 12,
                end: 14,
            },
            Edit {
                path: "src/limiter.py".into(),
                before: around,
                after: repaired,
            },
            Proof::Judged {
                reason: "The window is read before the comparison.".into(),
            },
            Origin::Planted,
        ));
        let model = ScriptedModel::new([
            fix_answer("not", "The literals are as they were."),
            fix_answer("not", "The lines are as they were."),
        ]);
        let runner = FakeRunner::killing_the_operator();
        let learner = unchanged();
        let results = grade(&model, Some(&runner), Some(SUITE), &drill, &learner).await?;

        // The learner left the operator. Beside the literals' fix it still fails, and the other fix does not mend it.
        assert_eq!(results[0].outcome, FixOutcome::NotFixed, "{:?}", results[0]);
        assert_eq!(results[0].output.as_deref(), Some(OPERATOR_OUTPUT));
        let beside = rewritten(&learner, |text| {
            text.replace(LITERALS_AFTER, LITERALS_BEFORE)
        });
        assert!(runner.runs().contains(&(beside, KILLING_TEST.to_owned())));
        assert_eq!(shown(&results)[0]["gradedBeside"], json!([1]));
        Ok(())
    }

    #[cfg(feature = "learning-labs")]
    #[tokio::test]
    async fn a_defect_the_learner_left_fails_neither_the_test_nor_the_suite_of_an_issue_they_fixed_on_python(
    ) -> Result<()> {
        use crate::drills::runner::runner_for;
        use crate::embedded_runtime::BuiltinRuntimes;

        // The second issue forgets to save the calls it counted, so the first issue's test, and the suite, fail
        // whatever the operator.
        const STATE_TEST: &str = "from src.limiter import RateLimiter\n\nlimiter = RateLimiter(2, 10, lambda: 0)\nassert limiter.allow(\"a\")\nassert limiter.allow(\"a\")\nassert not limiter.allow(\"a\"), \"the third call inside the window must be refused\"\n";
        let state = Edit {
            path: "src/limiter.py".into(),
            before:
                "        recent.append(now)\n        self.hits[key] = recent\n        return True\n"
                    .into(),
            after: "        recent.append(now)\n        return True\n".into(),
        };
        let planted = [operator_edit(), state];
        let (drilled, spans) = apply(&files(), &planted).expect("the edits apply");
        let drill = Drill {
            id: "drill".into(),
            material: Material::code("python"),
            title: "A sliding-window rate limiter".into(),
            files: drilled,
            key: vec![
                key_issue(
                    "planted-1",
                    OPERATOR,
                    spans[0].clone(),
                    reversed(&planted[0]),
                    Proof::Killed {
                        test: KILLING_TEST.into(),
                    },
                    Origin::Planted,
                ),
                key_issue(
                    "planted-2",
                    "correctness.stale_state",
                    spans[1].clone(),
                    reversed(&planted[1]),
                    Proof::Killed {
                        test: STATE_TEST.into(),
                    },
                    Origin::Planted,
                ),
            ],
        };
        // The learner fixes the operator and leaves the second issue.
        let learner = rewritten(&drill.files, |text| {
            text.replace(OPERATOR_AFTER, OPERATOR_BEFORE)
        });

        let runner = runner_for("python", &BuiltinRuntimes::for_tests())
            .ok_or_else(|| AppError::InternalError("Python has no runner here".into()))?;
        let model = ScriptedModel::new(Vec::<String>::new());
        let cancel = CancellationToken::new();
        let tools = Tools {
            llm: &model,
            runner: Some(runner.as_ref()),
            cancel: &cancel,
        };
        let results = grade_fix(
            &CodeMaterial::new(tools.runner),
            tools,
            &FixCase {
                drill: &drill,
                learner: &learner,
                suite: Some(SUITE),
                source: None,
                assessment: &raised_all(&drill),
            },
        )
        .await?;

        assert_eq!(results[0].outcome, FixOutcome::Fixed, "{:?}", results[0]);
        assert_eq!(results[0].decided_by, DecidedBy::Test);
        let shown = shown(&results);
        assert_eq!(shown[0]["gradedBeside"], json!([1]));
        assert_eq!(
            shown[0]["suite"],
            json!({"status": "passed", "ranBeside": [1]})
        );
        assert_eq!(results[1].outcome, FixOutcome::NotFixed, "{:?}", results[1]);
        assert!(
            results[1].output.as_deref().is_some_and(
                |output| output.contains("the third call inside the window must be refused")
            ),
            "{:?}",
            results[1].output
        );
        Ok(())
    }
}
