//! The runner calls Program's own commands make, behind one seam so a session's tests need no runner.
use crate::drills::runner::CodeRunner;
use crate::practice::coding::examples::{self, ExampleCheck};
use crate::practice::coding::harness::{harness_for, CaseHarness};
use crate::practice::coding::problem::{CaseResult, Problem};
use async_trait::async_trait;
use lattice_core::shared::error::{AppError, Result};
use tokio_util::sync::CancellationToken;

fn harness_of(problem: &Problem) -> Result<Box<dyn CaseHarness>> {
    harness_for(&problem.spec.language).ok_or_else(|| {
        AppError::InvalidState(format!("No case harness for {}.", problem.spec.language))
    })
}

#[async_trait]
pub trait CodingAids: Send + Sync {
    async fn check_example(
        &self,
        problem: &Problem,
        args: &[String],
        expected: &str,
    ) -> Result<ExampleCheck>;

    /// What the reference returns for `args`.
    async fn reference_answer(&self, problem: &Problem, args: &[String]) -> Result<CaseResult>;
}

/// The runner the app runs examples on.
pub struct LiveCodingAids<'a> {
    pub runner: &'a dyn CodeRunner,
    pub cancel: CancellationToken,
}

#[async_trait]
impl CodingAids for LiveCodingAids<'_> {
    async fn check_example(
        &self,
        problem: &Problem,
        args: &[String],
        expected: &str,
    ) -> Result<ExampleCheck> {
        let harness = harness_of(problem)?;
        examples::check_example(
            self.runner,
            harness.as_ref(),
            problem,
            args,
            expected,
            self.cancel.clone(),
        )
        .await
    }

    async fn reference_answer(&self, problem: &Problem, args: &[String]) -> Result<CaseResult> {
        let harness = harness_of(problem)?;
        examples::reference_answer(
            self.runner,
            harness.as_ref(),
            problem,
            args,
            self.cancel.clone(),
        )
        .await
    }
}
