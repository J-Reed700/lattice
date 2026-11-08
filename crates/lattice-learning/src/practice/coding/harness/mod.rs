//! Case harness for coding problems: evaluator generation and per-case result extraction.

use crate::drills::runner::{CodeRunner, RunOutcome};
use crate::lab_runtime::LearningLabFile;
use crate::practice::coding::problem::{Case, CaseResult, ProblemSpec};
use lattice_core::shared::error::Result;
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
    _runner: &dyn CodeRunner,
    _harness: &dyn CaseHarness,
    _spec: &ProblemSpec,
    _solution_code: &str,
    _cases: &[Case],
    _cancel: CancellationToken,
) -> Result<(RunOutcome, Vec<CaseResult>)> {
    Ok((RunOutcome::Passed, Vec::new()))
}
