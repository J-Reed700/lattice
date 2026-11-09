//! Checks on code a model wrote, run in the sandbox.

use crate::drills::runner::CodeRunner;
use crate::practice::coding::harness::CaseHarness;
use crate::practice::coding::problem::Problem;
use lattice_core::shared::error::{AppError, Result};
use tokio_util::sync::CancellationToken;

#[cfg(test)]
mod tests;

/// The fenced code blocks of a markdown text, in order.
pub fn code_blocks(_markdown: &str) -> Vec<String> {
    Vec::new()
}

/// Whether `code`, as `solution.py`, passes every case of `problem`.
pub async fn passes_every_case(
    _runner: &dyn CodeRunner,
    _harness: &dyn CaseHarness,
    _problem: &Problem,
    _code: &str,
    _cancel: CancellationToken,
) -> Result<bool> {
    Err(AppError::InvalidState("not built yet".into()))
}

/// Whether `code` parses as Python (run in the sandbox with `ast.parse`).
pub async fn parses_as_python(
    _runner: &dyn CodeRunner,
    _code: &str,
    _cancel: CancellationToken,
) -> Result<bool> {
    Err(AppError::InvalidState("not built yet".into()))
}
