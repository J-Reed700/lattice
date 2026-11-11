//! The build template, the same for every kind: the author drafts, the proof checks, and a draft that does not hold
//! is written again with the reason. A lesson with nothing of the kind to make is remembered and not asked again.

use crate::practice::session::author::{Authored, Commission, Failed, Lesson};
use crate::practice::session::kind::Kind;
use crate::practice::session::proof::{prove, Proof};
use async_trait::async_trait;
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::practice::work::{Exercise, Form, KindId};
use lattice_core::shared::error::{AppError, Result};
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
/// reason, up to `MAX_PROOF_REASKS` times. A lesson the author says has nothing of the kind to make is remembered, and
/// a lesson already remembered is not asked.
pub async fn build(
    kind: &Kind,
    making: &Making<'_>,
    llm: &dyn LLMPort,
    store: &dyn NothingStore,
    steps: &dyn BuildSteps,
    cancel: CancellationToken,
) -> Result<Built> {
    let target = making.lesson.map(|lesson| Target {
        program_id: making.program_id,
        lesson_id: &lesson.id,
        kind: &kind.id,
        form: making.form,
    });
    if let Some(target) = &target {
        if let Some(reason) = store.nothing_known(target).await? {
            return Ok(Built::NothingToMake(reason));
        }
    }

    let mut failed: Vec<Failed> = Vec::new();
    loop {
        // Progress only moves forward, so a draft asked for again is written during the proving step.
        report(steps, u32::from(!failed.is_empty()), STEP_WRITING).await?;
        let authored = kind
            .author
            .draft(&Commission {
                program_id: making.program_id,
                lesson: making.lesson,
                form: making.form,
                options: making.options,
                facts: making.facts,
                llm,
                failed: &failed,
            })
            .await?;
        let (draft, raw) = match authored {
            Authored::Draft { draft, raw } => (*draft, raw),
            Authored::NothingToMake(reason) => {
                if let Some(target) = &target {
                    store.record_nothing(target, &reason).await?;
                }
                return Ok(Built::NothingToMake(reason));
            }
        };
        report(steps, 1, STEP_PROVING).await?;
        match prove(kind, draft, cancel.clone()).await? {
            Proof::Proven(exercise) => return Ok(Built::Exercise(exercise)),
            Proof::Unproven(reason) => {
                if failed.len() >= MAX_PROOF_REASKS {
                    return Err(AppError::ModelAnswerUnusable(reason));
                }
                failed.push(Failed { raw, reason });
            }
        }
    }
}

/// Tells the build's steps where it is; a build nobody waits for any more stops.
async fn report(steps: &dyn BuildSteps, current: u32, message: &str) -> Result<()> {
    if steps.step(current, message).await? {
        Ok(())
    } else {
        Err(AppError::InvalidState(
            "The practice build was stopped.".into(),
        ))
    }
}
