//! Checks a program by running it against the cases.

use crate::practice::coding::harness::run_cases;
use crate::practice::coding::problem::ProblemCase;
use crate::practice::coding::proof::{cancelled, run_end};
use crate::practice::kinds::program::shape::{problem_case_of, result_of, solution_of, spec_of};
use crate::practice::kinds::program::toolchain::Toolchains;
use crate::practice::session::kind::Checker;
use async_trait::async_trait;
use lattice_core::domain::learning::practice::work::{Brief, Case, CheckReport, Piece};
use lattice_core::shared::error::Result;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[cfg(test)]
mod tests;

pub struct ProgramChecker {
    pub tools: Arc<dyn Toolchains>,
}

#[async_trait]
impl Checker for ProgramChecker {
    async fn check(
        &self,
        brief: &Brief,
        piece: &Piece,
        cases: &[Case],
        cancel: CancellationToken,
    ) -> Result<CheckReport> {
        let spec = spec_of(brief)?;
        let code = solution_of(piece)?;
        let cases = cases
            .iter()
            .map(problem_case_of)
            .collect::<Result<Vec<ProblemCase>>>()?;
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        let runner = self.tools.runner(&spec.language)?;
        let harness = self.tools.harness(&spec.language)?;
        let (outcome, results) = run_cases(
            runner.as_ref(),
            harness.as_ref(),
            &spec,
            &code,
            &cases,
            cancel,
        )
        .await?;
        run_end(&outcome)?;
        Ok(CheckReport {
            results: results.iter().map(result_of).collect::<Result<Vec<_>>>()?,
            marks: Vec::new(),
            detail: serde_json::Value::Null,
        })
    }
}
