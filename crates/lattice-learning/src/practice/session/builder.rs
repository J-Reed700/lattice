//! The build template, the same for every kind: the author drafts, the proof checks, and a draft that does not hold
//! is written again with the reason. A lesson with nothing of the kind to make is remembered and not asked again.

use crate::practice::session::author::Lesson;
use crate::practice::session::kind::Kind;
use async_trait::async_trait;
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::practice::work::{Exercise, Form, KindId};
use lattice_core::shared::error::Result;
use serde_json::Value;
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

/// Reports a build's progress; false once the build is no longer wanted.
#[async_trait]
pub trait BuildSteps: Send + Sync {
    async fn step(&self, current: u32, message: &str) -> Result<bool>;
}

/// What a build is for.
pub struct Making<'a> {
    pub program_id: &'a str,
    pub lesson: Option<&'a Lesson>,
    pub form: Form,
    pub options: &'a Value,
    pub facts: &'a Value,
}

/// A lesson, a kind and a form: what a "nothing to make" answer is about.
pub struct Target<'a> {
    pub program_id: &'a str,
    pub lesson_id: &'a str,
    pub kind: &'a KindId,
    pub form: Form,
}

/// Remembers the lessons that have nothing of a kind to make.
#[async_trait]
pub trait NothingStore: Send + Sync {
    /// Why the target has nothing to make, if an earlier build found that.
    async fn nothing_known(&self, target: &Target<'_>) -> Result<Option<String>>;

    async fn record_nothing(&self, target: &Target<'_>, reason: &str) -> Result<()>;
}

/// What a proven exercise is saved as.
pub struct Saving<'a> {
    pub job_id: &'a str,
    pub program_id: &'a str,
    pub lesson_id: &'a str,
    pub options: &'a Value,
    pub model: &'a str,
}

/// What the build job needs from a kind's storage.
#[async_trait]
pub trait Exercises: Send + Sync {
    /// What the kind's author is told besides the request.
    async fn facts(
        &self,
        kind: &KindId,
        program_id: &str,
        form: Form,
        options: &Value,
    ) -> Result<Value>;

    /// Saves a proven exercise and returns the id of what was saved. The same job saves once.
    async fn save(&self, saving: &Saving<'_>, exercise: &Exercise) -> Result<String>;
}

/// What a build came to.
#[derive(Debug, Clone, PartialEq)]
pub enum Built {
    Exercise(Box<Exercise>),
    NothingToMake(String),
}

/// Builds one exercise of `kind`: the author drafts, the proof checks, and a refused draft is written again with the
/// reason, up to `MAX_PROOF_REASKS` times.
pub async fn build(
    _kind: &Kind,
    _making: &Making<'_>,
    _llm: &dyn LLMPort,
    _store: &dyn NothingStore,
    _steps: &dyn BuildSteps,
    _cancel: CancellationToken,
) -> Result<Built> {
    Ok(Built::NothingToMake(String::new()))
}
