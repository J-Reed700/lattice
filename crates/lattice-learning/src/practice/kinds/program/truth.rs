//! What makes a program's reference right: running it.

use crate::practice::coding::problem::ProblemCase;
use crate::practice::coding::proof::{establish_reference, Referenced};
use crate::practice::kinds::program::shape::{
    case_of, problem_case_of, proof_of, solution_of, spec_of,
};
use crate::practice::kinds::program::toolchain::Toolchains;
use crate::practice::session::kind::{Established, Truth};
use async_trait::async_trait;
use lattice_core::domain::learning::practice::work::{Draft, Exercise};
use lattice_core::shared::error::Result;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[cfg(test)]
mod tests;

pub struct ProgramTruth {
    pub tools: Arc<dyn Toolchains>,
}

#[async_trait]
impl Truth for ProgramTruth {
    async fn establish(&self, draft: &Draft, cancel: CancellationToken) -> Result<Established> {
        let spec = spec_of(&draft.brief)?;
        let reference = solution_of(&draft.reference)?;
        let cases = draft
            .cases
            .iter()
            .map(problem_case_of)
            .collect::<Result<Vec<ProblemCase>>>()?;
        let runner = self.tools.runner(&spec.language)?;
        let harness = self.tools.harness(&spec.language)?;
        let referenced = establish_reference(
            runner.as_ref(),
            harness.as_ref(),
            &spec,
            &reference,
            &cases,
            cancel,
        )
        .await?;
        match referenced {
            Referenced::Right {
                cases,
                reference_ms,
            } => Ok(Established::Right {
                cases: cases.iter().map(case_of).collect::<Result<Vec<_>>>()?,
                proof: proof_of(&reference_ms)?,
            }),
            Referenced::Wrong(note) => Ok(Established::Wrong(note)),
        }
    }

    async fn unambiguous(
        &self,
        _exercise: &Exercise,
        _cancel: CancellationToken,
    ) -> Result<Option<String>> {
        Ok(None)
    }
}
