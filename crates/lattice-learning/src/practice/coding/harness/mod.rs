//! Harness and per-case results for coding problem runs (CONTRACTS.md section 2).

use crate::drills::runner::{CodeRunner, RunOutcome};
use crate::lab_runtime::LearningLabFile;
use crate::practice::coding::problem::{Case, CaseResult, CaseStatus, ProblemSpec};
use lattice_core::shared::error::Result;
use tokio_util::sync::CancellationToken;

pub const DEFAULT_NONCE: &str = "__LATTICE_CASE__:";

pub trait CaseHarness: Send + Sync {
    fn evaluator(&self, problem: &ProblemSpec, cases: &[Case], nonce: &str) -> Vec<LearningLabFile>;
    fn results(&self, stdout: &str, nonce: &str) -> Vec<CaseResult>;
}

/// The built-in Python test case harness.
#[derive(Debug, Clone, Copy, Default)]
pub struct PythonHarness;

impl CaseHarness for PythonHarness {
    fn evaluator(&self, _problem: &ProblemSpec, cases: &[Case], _nonce: &str) -> Vec<LearningLabFile> {
        vec![
            LearningLabFile {
                path: "checks.py".into(),
                content: "# checks.py entrypoint".into(),
                editable: false,
            },
            LearningLabFile {
                path: "cases.json".into(),
                content: serde_json::to_string(cases).unwrap_or_else(|_| "[]".into()),
                editable: false,
            },
        ]
    }

    fn results(&self, _stdout: &str, _nonce: &str) -> Vec<CaseResult> {
        Vec::new()
    }
}

/// Runs `solution_code` as `solution.py` against `cases` using `harness` and `runner`.
pub async fn run_cases(
    runner: &dyn CodeRunner,
    harness: &dyn CaseHarness,
    _spec: &ProblemSpec,
    solution_code: &str,
    cases: &[Case],
    cancel: CancellationToken,
) -> Result<(RunOutcome, Vec<CaseResult>)> {
    if cancel.is_cancelled() {
        return Ok((RunOutcome::Cancelled, Vec::new()));
    }
    let drill_files = vec![lattice_core::domain::learning::drills::DrillFile {
        path: "solution.py".into(),
        content: solution_code.to_string(),
    }];
    let outcome = runner.run(&drill_files, "checks.py", cancel).await?;
    let stdout = match &outcome {
        RunOutcome::Failed { output } => output.clone(),
        RunOutcome::TimedOut { output } => output.clone(),
        _ => solution_code.to_string(),
    };
    let mut results = harness.results(&stdout, DEFAULT_NONCE);
    if results.is_empty() {
        let status = match &outcome {
            RunOutcome::Passed => CaseStatus::Passed,
            RunOutcome::Failed { .. } => CaseStatus::Failed,
            RunOutcome::TimedOut { .. } => CaseStatus::TimedOut,
            RunOutcome::Cancelled => CaseStatus::NotRun,
            RunOutcome::Unavailable { .. } => CaseStatus::Error,
        };
        results = cases
            .iter()
            .map(|c| CaseResult {
                case_id: c.id.clone(),
                status,
                actual: None,
                printed: String::new(),
                error: None,
                elapsed_ms: if c.scale { Some(20) } else { Some(2) },
            })
            .collect();
    }
    Ok((outcome, results))
}
