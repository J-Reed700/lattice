//! Case harness for coding problems: evaluator generation and per-case result extraction.

use crate::drills::runner::{CodeRunner, RunOutcome};
use crate::lab_runtime::LearningLabFile;
use crate::practice::coding::problem::{Case, CaseResult, CaseStatus, ProblemSpec};
use lattice_core::shared::error::Result;
use std::collections::HashMap;
use tokio_util::sync::CancellationToken;

pub mod python;
pub use python::PythonHarness;

#[cfg(test)]
mod tests;

pub const DEFAULT_NONCE: &str = "__LATTICE_CASE__:";

pub trait CaseHarness: Send + Sync {
    fn evaluator(&self, problem: &ProblemSpec, cases: &[Case], nonce: &str) -> Vec<LearningLabFile>;
    fn results(&self, stdout: &str, nonce: &str) -> Vec<CaseResult>;
}

/// Runs `solution_code` as `solution.py` against `cases` using `harness` and `runner`.
/// When the run times out, the first unfinished case in execution order (non-scale then scale)
/// gets `CaseStatus::TimedOut` and subsequent unfinished cases get `CaseStatus::NotRun`.
pub async fn run_cases(
    runner: &dyn CodeRunner,
    harness: &dyn CaseHarness,
    spec: &ProblemSpec,
    solution_code: &str,
    cases: &[Case],
    cancel: CancellationToken,
) -> Result<(RunOutcome, Vec<CaseResult>)> {
    let mut ordered_cases: Vec<&Case> = Vec::with_capacity(cases.len());
    for case in cases {
        if !case.scale {
            ordered_cases.push(case);
        }
    }
    for case in cases {
        if case.scale {
            ordered_cases.push(case);
        }
    }

    let ordered_cases_vec: Vec<Case> = ordered_cases.iter().map(|&c| c.clone()).collect();
    let mut files = harness.evaluator(spec, &ordered_cases_vec, DEFAULT_NONCE);
    files.push(LearningLabFile {
        path: crate::practice::coding::problem::SOLUTION_PATH.to_owned(),
        content: solution_code.to_owned(),
    });

    let read_only_paths: Vec<String> = files
        .iter()
        .filter(|f| f.path != crate::practice::coding::problem::SOLUTION_PATH)
        .map(|f| f.path.clone())
        .collect();

    let (outcome, stdout) = runner.run_capturing(&files, &read_only_paths, cancel).await?;
    let finished_results = harness.results(&stdout, DEFAULT_NONCE);

    let finished_map: HashMap<&str, &CaseResult> = finished_results
        .iter()
        .map(|r| (r.case_id.as_str(), r))
        .collect();

    let is_timeout = matches!(outcome, RunOutcome::TimedOut { .. });
    let mut found_inflight = false;
    let mut final_results = Vec::with_capacity(ordered_cases.len());

    for case in &ordered_cases {
        if let Some(finished) = finished_map.get(case.id.as_str()) {
            final_results.push((*finished).clone());
        } else if is_timeout {
            if !found_inflight {
                found_inflight = true;
                final_results.push(CaseResult {
                    case_id: case.id.clone(),
                    status: CaseStatus::TimedOut,
                    actual: None,
                    printed: String::new(),
                    error: Some("The run exceeded the runtime's time limit and was stopped.".to_owned()),
                    elapsed_ms: None,
                });
            } else {
                final_results.push(CaseResult {
                    case_id: case.id.clone(),
                    status: CaseStatus::NotRun,
                    actual: None,
                    printed: String::new(),
                    error: None,
                    elapsed_ms: None,
                });
            }
        } else {
            final_results.push(CaseResult {
                case_id: case.id.clone(),
                status: CaseStatus::NotRun,
                actual: None,
                printed: String::new(),
                error: None,
                elapsed_ms: None,
            });
        }
    }

    Ok((outcome, final_results))
}
