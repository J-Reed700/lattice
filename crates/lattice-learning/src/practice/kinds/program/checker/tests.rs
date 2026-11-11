#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::practice::coding::proof::fakes::{
    proven_cases, spec, Behaviour, FakeHarness, FakeRunner,
};
use crate::practice::kinds::program::shape::{brief_of, case_of, solution_piece};
use crate::practice::kinds::program::toolchain::FakeToolchains;
use lattice_core::domain::learning::practice::work::{CaseResult as WorkResult, CaseStatus};
use lattice_core::shared::error::AppError;

async fn check(tools: &Arc<FakeToolchains>, cancel: CancellationToken) -> Result<CheckReport> {
    let cases: Vec<Case> = proven_cases()
        .iter()
        .map(|case| case_of(case).unwrap())
        .collect();
    ProgramChecker {
        tools: tools.clone(),
    }
    .check(
        &brief_of(&spec()).unwrap(),
        &solution_piece("code").unwrap(),
        &cases,
        cancel,
    )
    .await
}

async fn run_directly(tools: &FakeToolchains) -> Vec<WorkResult> {
    let runner = tools.runner("python").unwrap();
    let (_, results) = run_cases(
        runner.as_ref(),
        &FakeHarness,
        &spec(),
        "code",
        &proven_cases(),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    results
        .iter()
        .map(|result| result_of(result).unwrap())
        .collect()
}

#[tokio::test]
async fn a_passing_behaviour_reports_what_run_cases_reports() {
    let tools = Arc::new(FakeToolchains::new(FakeRunner::new(|_, _| {
        Behaviour::Passes { ms: 3 }
    })));
    let report = check(&tools, CancellationToken::new()).await.unwrap();
    assert_eq!(report.results, run_directly(&tools).await);
    assert!(report.all_passed());
    assert!(report.marks.is_empty());
    assert_eq!(report.detail, serde_json::Value::Null);
}

#[tokio::test]
async fn a_failing_behaviour_reports_what_run_cases_reports() {
    let tools = Arc::new(FakeToolchains::new(FakeRunner::new(|_, case| {
        if case.id == "hidden-1" {
            Behaviour::Fails
        } else {
            Behaviour::Passes { ms: 1 }
        }
    })));
    let report = check(&tools, CancellationToken::new()).await.unwrap();
    assert_eq!(report.results, run_directly(&tools).await);
    assert!(!report.all_passed());
    let failed: Vec<&str> = report
        .results
        .iter()
        .filter(|result| result.status == CaseStatus::Failed)
        .map(|result| result.case_id.as_str())
        .collect();
    assert_eq!(failed, ["hidden-1"]);
    assert!(report.marks.is_empty());
}

#[tokio::test]
async fn a_timed_out_behaviour_reports_what_run_cases_reports() {
    let tools = Arc::new(FakeToolchains::timing_out(FakeRunner::new(|_, _| {
        Behaviour::Passes { ms: 1 }
    })));
    let report = check(&tools, CancellationToken::new()).await.unwrap();
    assert_eq!(report.results, run_directly(&tools).await);
    assert!(report.marks.is_empty());
}

#[tokio::test]
async fn a_runtime_that_is_not_there_is_an_error() {
    let tools = Arc::new(FakeToolchains::new(FakeRunner::unavailable(
        "no interpreter",
    )));
    let result = check(&tools, CancellationToken::new()).await;
    assert!(matches!(result, Err(AppError::ServiceNotAvailable(_))));
}

#[tokio::test]
async fn a_cancelled_check_runs_nothing() {
    let tools = Arc::new(FakeToolchains::new(FakeRunner::new(|_, _| {
        Behaviour::Fails
    })));
    let cancel = CancellationToken::new();
    cancel.cancel();
    let result = check(&tools, cancel).await;
    assert!(matches!(result, Err(AppError::InvalidState(_))));
    assert!(tools.runner.codes().is_empty());
}
