//! A learner's example before any code, checked on the reference and the misreadings.

use crate::drills::runner::CodeRunner;
use crate::practice::coding::dto::ExampleVerdict;
use crate::practice::coding::harness::CaseHarness;
use crate::practice::coding::problem::{CaseResult, CaseStatus, Problem};
use lattice_core::shared::error::{AppError, Result};
use tokio_util::sync::CancellationToken;

#[cfg(test)]
mod tests;

/// What the reference and the misreadings say about one example.
#[derive(Debug, Clone, PartialEq)]
pub struct ExampleCheck {
    pub verdict: ExampleVerdict,
    pub reference_actual: Option<String>,
    pub why_invalid: Option<String>,
    pub catches: Vec<String>,
}

pub async fn check_example(
    _runner: &dyn CodeRunner,
    _harness: &dyn CaseHarness,
    _problem: &Problem,
    _args: &[String],
    _expected: &str,
    _cancel: CancellationToken,
) -> Result<ExampleCheck> {
    Err(AppError::InvalidState("not built yet".into()))
}

/// What the reference returns for `args` (Try an input, Coached).
pub async fn reference_answer(
    _runner: &dyn CodeRunner,
    _harness: &dyn CaseHarness,
    _problem: &Problem,
    _args: &[String],
    _cancel: CancellationToken,
) -> Result<CaseResult> {
    Err(AppError::InvalidState("not built yet".into()))
}
