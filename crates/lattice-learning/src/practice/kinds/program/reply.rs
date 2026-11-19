//! What a program's counterpart adds to its reply: the clarifications it answers from.

use crate::practice::coding::solution_check::{code_blocks, passes_every_case};
use crate::practice::kinds::program::shape::{problem_of, spec_of};
use crate::practice::kinds::program::toolchain::Toolchains;
use crate::practice::session::kind::{Known, ReplyGuard};
use async_trait::async_trait;
use lattice_core::domain::learning::practice::problem_patterns::named_in;
use lattice_core::domain::learning::practice::work::{Brief, Exercise};
use lattice_core::shared::error::{AppError, Result};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// What the app adds to a reply that answers with the problem's own "your call" line.
const YOUR_CALL: &str = "Your call: state your assumption.";

pub struct ProgramReply {
    pub tools: Arc<dyn Toolchains>,
}

fn refuse(reason: impl Into<String>) -> AppError {
    AppError::InvalidInput(reason.into())
}

#[async_trait]
impl ReplyGuard for ProgramReply {
    fn rules(&self, _brief: &Brief) -> String {
        "When the learner asks a question the statement leaves open, answer it from the clarifications you are given and point clarification at the one that covers it, as {\"kind\": \"from\", \"which\": that clarification's handle}; the app adds its answer after your words, so do not write the answer out. When none covers it, set clarification to {\"kind\": \"your_call\"} and the app adds \"Your call: state your assumption.\" after your words; otherwise set clarification to {\"kind\": \"none\"}."
            .to_owned()
    }

    fn format(&self) -> String {
        "parts are the handles of the lines of the learner's code that you refer to, or none; clarification is as described above"
            .to_owned()
    }

    fn known(&self, brief: &Brief) -> Result<Vec<Known>> {
        Ok(spec_of(brief)?
            .clarifications
            .iter()
            .map(|clarification| Known {
                question: clarification.question.clone(),
                answer: clarification.answer.clone(),
            })
            .collect())
    }

    fn otherwise(&self) -> Option<String> {
        Some(YOUR_CALL.to_owned())
    }

    fn check_words(&self, exercise: &Exercise, content: &str) -> Result<()> {
        let spec = spec_of(&exercise.brief)?;
        if let Some(pattern) = named_in(content, &spec.patterns) {
            return Err(refuse(format!(
                "The reply names the technique \"{}\". Do not name it; ask about the idea instead.",
                pattern.name
            )));
        }
        Ok(())
    }

    async fn check_unsolved(
        &self,
        exercise: &Exercise,
        content: &str,
        cancel: CancellationToken,
    ) -> Result<()> {
        let blocks = code_blocks(content);
        if blocks.is_empty() {
            return Ok(());
        }
        let problem = problem_of(exercise)?;
        let runner = self.tools.runner(&problem.spec.language)?;
        let harness = self.tools.harness(&problem.spec.language)?;
        for block in blocks {
            let solves = passes_every_case(
                runner.as_ref(),
                harness.as_ref(),
                &problem,
                &block,
                cancel.clone(),
            )
            .await?;
            if solves {
                return Err(refuse(
                    "A code block in the reply passes every case, so it solves the problem. Do not write a working solution; describe the step in words, or show a fragment that leaves the problem unsolved.",
                ));
            }
        }
        Ok(())
    }
}
