//! A learner's example before any code: run on the reference to say whether the problem agrees, and on each
//! misreading to say which mistakes it would catch. Nothing here runs the learner's code.

use crate::drills::runner::{CodeRunner, RunOutcome};
use crate::lab_runtime::LearningLabFile;
use crate::practice::coding::dto::ExampleVerdict;
use crate::practice::coding::harness::{run_cases, CaseHarness};
use crate::practice::coding::problem::{
    CaseResult, CaseStatus, CaseVisibility, Problem, ProblemCase,
};
use lattice_core::shared::error::{AppError, Result};
use tokio_util::sync::CancellationToken;

#[cfg(test)]
mod tests;

const LITERALS_PY: &str = include_str!("examples/literals.py");
const EXAMPLE_ID: &str = "example";
const INPUT_ID: &str = "input";

/// What the reference and the misreadings say about one example.
#[derive(Debug, Clone, PartialEq)]
pub struct ExampleCheck {
    pub verdict: ExampleVerdict,
    /// What the reference returned, when it disagreed.
    pub reference_actual: Option<String>,
    pub why_invalid: Option<String>,
    /// The ids of the misreadings this example catches. Empty unless the verdict is Right.
    pub catches: Vec<String>,
}

fn invalid(why: String) -> ExampleCheck {
    ExampleCheck {
        verdict: ExampleVerdict::Invalid,
        reference_actual: None,
        why_invalid: Some(why),
        catches: Vec::new(),
    }
}

fn one_case(id: &str, problem: &Problem, args: &[String], expected: Option<&str>) -> ProblemCase {
    ProblemCase {
        id: id.to_owned(),
        category: "Your example".to_owned(),
        args: args.to_vec(),
        expected: expected.map(str::to_owned),
        compare: problem.default_compare(),
        visibility: CaseVisibility::Example,
        scale: false,
        digest: None,
    }
}

/// Runs `case` on `code` and returns its result, failing when the run says nothing about the code.
async fn result_of(
    runner: &dyn CodeRunner,
    harness: &dyn CaseHarness,
    problem: &Problem,
    code: &str,
    case: ProblemCase,
    cancel: CancellationToken,
) -> Result<CaseResult> {
    let (outcome, results) = run_cases(
        runner,
        harness,
        &problem.spec,
        code,
        std::slice::from_ref(&case),
        cancel,
    )
    .await?;
    if let Some(why) = outcome.says_nothing() {
        return Err(AppError::InvalidState(why));
    }
    results
        .into_iter()
        .next()
        .ok_or_else(|| AppError::InternalError("The harness returned no result.".into()))
}

/// For each source, why it is not a Python literal, or None when it is.
async fn literal_problems(
    runner: &dyn CodeRunner,
    sources: &[String],
    cancel: CancellationToken,
) -> Result<Vec<Option<String>>> {
    let entrypoint = runner.entrypoint().to_owned();
    let files = [
        LearningLabFile {
            path: entrypoint.clone(),
            content: LITERALS_PY.to_owned(),
        },
        LearningLabFile {
            path: "literals.json".to_owned(),
            content: serde_json::to_string(sources)
                .map_err(|error| AppError::Serialization(error.to_string()))?,
        },
    ];
    let (outcome, stdout) = runner.run_capturing(&files, &[entrypoint], cancel).await?;
    if let RunOutcome::Cancelled | RunOutcome::Unavailable { .. } = outcome {
        return Err(AppError::InvalidState(
            outcome
                .says_nothing()
                .unwrap_or_else(|| "The run did not finish.".to_owned()),
        ));
    }
    let problems: Vec<Option<String>> = serde_json::from_str(stdout.trim()).map_err(|error| {
        AppError::InvalidState(format!("The literal check did not answer: {error}"))
    })?;
    if problems.len() == sources.len() {
        Ok(problems)
    } else {
        Err(AppError::InvalidState(
            "The literal check answered for the wrong number of values.".into(),
        ))
    }
}

/// Checks the example `args` -> `expected` against the reference, and when the reference agrees, against each
/// misreading.
pub async fn check_example(
    runner: &dyn CodeRunner,
    harness: &dyn CaseHarness,
    problem: &Problem,
    args: &[String],
    expected: &str,
    cancel: CancellationToken,
) -> Result<ExampleCheck> {
    let params = &problem.spec.signature.params;
    if args.len() != params.len() {
        return Ok(invalid(format!(
            "{} takes {} argument(s), and this example gives {}.",
            problem.spec.signature.function,
            params.len(),
            args.len()
        )));
    }
    let mut sources = args.to_vec();
    sources.push(expected.to_owned());
    let problems = literal_problems(runner, &sources, cancel.clone()).await?;
    let refused = problems
        .iter()
        .enumerate()
        .find_map(|(index, found)| found.as_ref().map(|found| (index, found)));
    if let Some((index, found)) = refused {
        return Ok(invalid(match params.get(index) {
            Some(param) => format!(
                "The argument for {} is not a Python literal ({found}).",
                param.name
            ),
            None => format!("The expected result is not a Python literal ({found})."),
        }));
    }

    let case = one_case(EXAMPLE_ID, problem, args, Some(expected));
    let reference = result_of(
        runner,
        harness,
        problem,
        &problem.reference,
        case.clone(),
        cancel.clone(),
    )
    .await?;
    match reference.status {
        CaseStatus::Passed => {}
        CaseStatus::Failed => {
            return Ok(ExampleCheck {
                verdict: ExampleVerdict::Wrong,
                reference_actual: reference.actual,
                why_invalid: None,
                catches: Vec::new(),
            })
        }
        CaseStatus::Error => {
            return Ok(invalid(format!(
                "The reference solution raises on these arguments ({}).",
                reference.error.unwrap_or_default()
            )))
        }
        CaseStatus::Ran | CaseStatus::TimedOut | CaseStatus::NotRun => {
            return Err(AppError::InvalidState(
                "The reference solution did not settle on this example.".into(),
            ))
        }
    }

    let mut catches = Vec::new();
    for misreading in &problem.misreadings {
        let result = result_of(
            runner,
            harness,
            problem,
            &misreading.code,
            case.clone(),
            cancel.clone(),
        )
        .await?;
        if !matches!(result.status, CaseStatus::Passed | CaseStatus::Ran) {
            catches.push(misreading.id.clone());
        }
    }
    Ok(ExampleCheck {
        verdict: ExampleVerdict::Right,
        reference_actual: None,
        why_invalid: None,
        catches,
    })
}

/// What the reference returns for `args` (Try an input, Coached).
pub async fn reference_answer(
    runner: &dyn CodeRunner,
    harness: &dyn CaseHarness,
    problem: &Problem,
    args: &[String],
    cancel: CancellationToken,
) -> Result<CaseResult> {
    result_of(
        runner,
        harness,
        problem,
        &problem.reference,
        one_case(INPUT_ID, problem, args, None),
        cancel,
    )
    .await
}
