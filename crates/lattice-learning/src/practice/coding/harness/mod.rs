//! The case harness: the evaluator a coding run adds beside the learner's code, and the per-case results read
//! back from what it printed.

use crate::drills::runner::{CodeRunner, RunOutcome};
use crate::lab_runtime::LearningLabFile;
use crate::practice::coding::problem::{CaseResult, ProblemCase, ProblemSpec, SOLUTION_PATH};
use lattice_core::shared::error::Result;
use tokio_util::sync::CancellationToken;

pub mod python;
pub use python::{lattice_check_file, PythonHarness};

#[cfg(test)]
pub(crate) mod fake;
#[cfg(test)]
mod tests;

/// How a run ended, as far as reading its cases is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEnd {
    Finished,
    TimedOut,
    Stopped,
}

impl RunEnd {
    pub fn of(outcome: &RunOutcome) -> Self {
        match outcome {
            RunOutcome::Passed | RunOutcome::Failed { .. } => Self::Finished,
            RunOutcome::TimedOut { .. } => Self::TimedOut,
            RunOutcome::Cancelled | RunOutcome::Unavailable { .. } => Self::Stopped,
        }
    }
}

pub trait CaseHarness: Send + Sync {
    /// The files a run adds beside the learner's: the entrypoint `checks.py` and `cases.json`.
    fn evaluator(
        &self,
        spec: &ProblemSpec,
        cases: &[ProblemCase],
        nonce: &str,
    ) -> Result<Vec<LearningLabFile>>;

    /// One result per case, in case order. A case with no line is TimedOut when it is the first such case of a run
    /// that timed out, otherwise NotRun.
    fn results(
        &self,
        cases: &[ProblemCase],
        stdout: &str,
        nonce: &str,
        end: RunEnd,
    ) -> Vec<CaseResult>;

    /// `stdout` without the harness's own lines.
    fn program_output(&self, stdout: &str, nonce: &str) -> String;
}

/// The harness for `language`, where there is one.
pub fn harness_for(language: &str) -> Option<Box<dyn CaseHarness>> {
    match language {
        "python" => Some(Box::new(PythonHarness::new())),
        _ => None,
    }
}

/// A fresh random nonce for one run (128 bits, hex).
pub fn new_nonce() -> String {
    String::new()
}

/// Runs `code` as `solution.py` against `cases` under a fresh nonce. Cases run in the order given.
pub async fn run_cases(
    runner: &dyn CodeRunner,
    harness: &dyn CaseHarness,
    spec: &ProblemSpec,
    code: &str,
    cases: &[ProblemCase],
    cancel: CancellationToken,
) -> Result<(RunOutcome, Vec<CaseResult>)> {
    let nonce = new_nonce();
    let mut files = harness.evaluator(spec, cases, &nonce)?;
    let read_only_paths: Vec<String> = files.iter().map(|file| file.path.clone()).collect();
    files.push(LearningLabFile {
        path: SOLUTION_PATH.to_owned(),
        content: code.to_owned(),
    });
    let (outcome, stdout) = runner
        .run_capturing(&files, &read_only_paths, cancel)
        .await?;
    let results = harness.results(cases, &stdout, &nonce, RunEnd::of(&outcome));
    Ok((outcome, results))
}
