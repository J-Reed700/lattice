//! Where coding problems come from. `GeneratedProblems` has the model write a problem, proves it by running the
//! reference and the misreadings, and asks again with what failed.

#![allow(unused_imports)]

use crate::drills::build::ask::{ask_request, refuse};
use crate::drills::runner::CodeRunner;
use crate::generation::parse_json;
use crate::lessons::structured_call::reask;
use crate::model_call;
use crate::practice::coding::harness::CaseHarness;
use crate::practice::coding::problem::{
    CaseCompare, CaseVisibility, CodingTask, Misreading, Problem, ProblemCase, ProblemClarification,
    ProblemDifficulty, ProblemSignature, ProblemSpec, SignatureParam,
};
use crate::practice::coding::proof::{prove, Proof};
use async_trait::async_trait;
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::practice::problem_patterns::{
    named_in, problem_pattern, PROBLEM_PATTERNS,
};
use lattice_core::shared::error::{AppError, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use tokio_util::sync::CancellationToken;

#[cfg(test)]
mod tests;

/// The three steps of a build, in the order the job reports them.
pub const STEP_WRITING: &str = "Writing the problem";
pub const STEP_PROVING: &str = "Proving it";
pub const STEP_SAVING: &str = "Saving";

/// How many times a draft that fails its proof is asked for again with what failed. Each attempt is a whole problem,
/// so this is a model-call budget: the design asks that 27 of 30 builds prove within it.
pub const MAX_PROOF_REASKS: usize = 2;

/// The fewest misreadings a draft offers. The proof drops those that pass every case and needs two that fail one.
pub const MIN_MISREADINGS: usize = 3;

/// The fewest cases the statement shows, enough to show the shape of the input and of the answer.
pub const MIN_EXAMPLE_CASES: usize = 2;

/// The fewest hidden cases, one for each edge the misreadings can be told apart by.
pub const MIN_HIDDEN_CASES: usize = 3;

/// The fewest scale cases: one large input is what tests the complexity the statement asks for.
pub const MIN_SCALE_CASES: usize = 1;

/// What an answer reserves: a statement, a reference, several misreadings and about a dozen cases. A reservation
/// sizes the request; an answer cut off at it is asked for again with twice the room.
const PROBLEM_ANSWER_TOKENS: usize = 8_000;

/// The lesson a problem is built to practise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LessonBrief {
    pub title: String,
    pub objective: String,
}

/// What a problem is built for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProblemRequest {
    pub program_id: String,
    pub lesson: Option<LessonBrief>,
    pub task: CodingTask,
    pub language: String,
    pub difficulty: ProblemDifficulty,
    /// Ids from the pattern catalogue that the problem must call for.
    pub patterns: Vec<String>,
    pub focus: Option<String>,
    /// What the learner has said about what they prepare for.
    pub details: Option<String>,
}

/// Reports a build's progress; false once the build is no longer wanted.
#[async_trait]
pub trait BuildSteps: Send + Sync {
    async fn step(&self, current: u32, message: &str) -> Result<bool>;
}

/// Where a problem comes from. The model writes one today; a pasted problem is the next source.
#[async_trait]
pub trait ProblemSource: Send + Sync {
    async fn problem(
        &self,
        request: &ProblemRequest,
        steps: &dyn BuildSteps,
        cancel: CancellationToken,
    ) -> Result<Problem>;
}

/// Problems the model writes and the app proves.
pub struct GeneratedProblems<'a> {
    pub llm: &'a dyn LLMPort,
    pub runner: &'a dyn CodeRunner,
    pub harness: &'a dyn CaseHarness,
}

#[async_trait]
impl ProblemSource for GeneratedProblems<'_> {
    async fn problem(
        &self,
        _request: &ProblemRequest,
        _steps: &dyn BuildSteps,
        _cancel: CancellationToken,
    ) -> Result<Problem> {
        Err(AppError::InvalidState("Problems are not built yet.".into()))
    }
}

/// The schema of the model's answer.
fn schema() -> Value {
    json!({})
}
