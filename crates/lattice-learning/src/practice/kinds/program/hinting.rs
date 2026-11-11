//! The hints of a program: today's ladder, asked through the kind's role.

use crate::practice::coding::hints::write_hint_from;
use crate::practice::kinds::program::parts::part_of;
use crate::practice::kinds::program::shape::{files_of, problem_of};
use crate::practice::kinds::program::toolchain::Toolchains;
use crate::practice::session::kind::{Hint, HintAsk, Hinting};
use async_trait::async_trait;
use lattice_core::shared::error::Result;
use std::sync::Arc;

pub struct ProgramHinting {
    pub tools: Arc<dyn Toolchains>,
}

#[async_trait]
impl Hinting for ProgramHinting {
    async fn hint(&self, ask: HintAsk<'_>) -> Result<Option<Hint>> {
        let problem = problem_of(ask.exercise)?;
        let draft = files_of(&ask.observation.piece)?;
        let runner = self.tools.runner(&problem.spec.language)?;
        let harness = self.tools.harness(&problem.spec.language)?;
        let hint = write_hint_from(
            ask.llm,
            runner.as_ref(),
            harness.as_ref(),
            &problem,
            ask.observed,
            ask.observation.setting,
            &draft,
            ask.rung,
            ask.in_role,
            ask.cancel,
        )
        .await?;
        let parts = match &hint.path {
            Some(path) => hint.lines.iter().map(|line| part_of(path, *line)).collect(),
            None => Vec::new(),
        };
        Ok(Some(Hint {
            rung: hint.rung,
            text: hint.text,
            reason: hint.reason,
            parts,
        }))
    }
}
