//! The model and runner calls a session makes, behind one seam so a session's tests need neither.
use crate::drills::runner::CodeRunner;
use crate::practice::coding::counterpart::{self, CounterpartReply};
use crate::practice::coding::dto::CodeHintRung;
use crate::practice::coding::examples::{self, ExampleCheck};
use crate::practice::coding::harness::{harness_for, CaseHarness};
use crate::practice::coding::hints::{write_hint, CodeHint};
use crate::practice::coding::observation::WorkspaceObservation;
use crate::practice::coding::policy::{CodingCue, CodingPolicy};
use crate::practice::coding::problem::{CaseResult, Problem};
use async_trait::async_trait;
use lattice_core::application::ports::llm_port::LLMPort;
use lattice_core::shared::error::{AppError, Result};
use tokio_util::sync::CancellationToken;

fn harness_of(problem: &Problem) -> Result<Box<dyn CaseHarness>> {
    harness_for(&problem.spec.language).ok_or_else(|| {
        AppError::InvalidState(format!("No case harness for {}.", problem.spec.language))
    })
}

#[async_trait]
pub trait CodingAids: Send + Sync {
    /// The model that writes the counterpart's words and the hints.
    fn model_name(&self) -> &str;

    async fn check_example(
        &self,
        problem: &Problem,
        args: &[String],
        expected: &str,
    ) -> Result<ExampleCheck>;

    /// What the reference returns for `args`.
    async fn reference_answer(&self, problem: &Problem, args: &[String]) -> Result<CaseResult>;

    async fn hint(
        &self,
        problem: &Problem,
        observation: &WorkspaceObservation,
        rung: CodeHintRung,
        in_role: bool,
    ) -> Result<CodeHint>;

    /// The counterpart's checked reply on `cue`, or None when it chose silence.
    async fn reply(
        &self,
        problem: &Problem,
        policy: &CodingPolicy,
        cue: &CodingCue,
        observation: &WorkspaceObservation,
    ) -> Result<Option<CounterpartReply>>;
}

/// The model and runner the app runs.
pub struct LiveAids<'a> {
    pub llm: &'a dyn LLMPort,
    pub runner: &'a dyn CodeRunner,
    pub cancel: CancellationToken,
}

#[async_trait]
impl CodingAids for LiveAids<'_> {
    fn model_name(&self) -> &str {
        self.llm.model_name()
    }

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

    async fn hint(
        &self,
        problem: &Problem,
        observation: &WorkspaceObservation,
        rung: CodeHintRung,
        in_role: bool,
    ) -> Result<CodeHint> {
        let harness = harness_of(problem)?;
        write_hint(
            self.llm,
            self.runner,
            harness.as_ref(),
            problem,
            observation,
            rung,
            in_role,
            self.cancel.clone(),
        )
        .await
    }

    async fn reply(
        &self,
        problem: &Problem,
        policy: &CodingPolicy,
        cue: &CodingCue,
        observation: &WorkspaceObservation,
    ) -> Result<Option<CounterpartReply>> {
        let harness = harness_of(problem)?;
        counterpart::respond(
            self.llm,
            self.runner,
            harness.as_ref(),
            problem,
            policy,
            cue,
            observation,
            self.cancel.clone(),
        )
        .await
    }
}
