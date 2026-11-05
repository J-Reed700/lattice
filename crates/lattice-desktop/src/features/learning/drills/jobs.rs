//! The `learning.drill_build` job: builds one drill from clean code, planting and proving its defects, and stores it
//! in the bank (docs/design/2026-11-04-code-review-drills.md section 2).

use super::build::DrillBuild;
use super::grader::{Review, ReviewGrader};
use super::runner::runner_for;
use super::spec::DrillSpec;
use crate::features::learning::embedded_runtime::BuiltinRuntimes;
use crate::features::learning::quiz_bank_jobs::ModelLoader;
use async_trait::async_trait;
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::drills::Assessment;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{
    Backoff, JobContext, JobHandler, JobKindConfig, JobOutcome, JobRecord, RecoveryPolicy,
};
use sqlx::SqlitePool;
use std::sync::Arc;

pub const DRILL_BUILD: &str = "learning.drill_build";

/// One build per language at a time; a running build resumes from its checkpoints after a restart, and a
/// disconnected model defers it rather than failing it.
pub fn drill_job_config() -> JobKindConfig {
    JobKindConfig::new(RecoveryPolicy::Requeue)
        .exclusive_per_subject()
        .backoff(Backoff::default())
}

/// Makes the grader of a build's blind review from the model the build runs on.
pub type GraderFactory = Arc<dyn Fn(Arc<dyn LLMPort>) -> Box<dyn ReviewGrader> + Send + Sync>;

/// Builds drills, on the Learning model, pinned for the whole build.
#[derive(Clone)]
pub struct DrillBuildWorker {
    pub pool: SqlitePool,
    pub load_llm: ModelLoader,
    pub runtimes: BuiltinRuntimes,
    pub grader: GraderFactory,
}

/// The spec a job was queued with.
pub fn spec_of(job: &JobRecord) -> Result<DrillSpec> {
    let spec: DrillSpec =
        serde_json::from_value(job.requested.get("spec").cloned().unwrap_or_default())
            .map_err(|_| AppError::InvalidInput("The drill job names no drill to build.".into()))?;
    spec.validate()?;
    Ok(spec)
}

#[async_trait]
impl JobHandler for DrillBuildWorker {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        let spec = spec_of(context.job())?;
        let Some(pin) = context.until_cancelled((self.load_llm)()).await else {
            return Ok(JobOutcome::Stopped);
        };
        let pin = pin?;
        let runner = runner_for(&spec.language, &self.runtimes);
        let grader = (self.grader)(Arc::clone(&pin.llm));
        DrillBuild::new(
            pin.llm.as_ref(),
            runner.as_deref(),
            grader.as_ref(),
            &self.pool,
            context.cancellation().clone(),
        )
        .run(context, &spec)
        .await
    }
}

/// The grader of a build until the judge that grades reviews (step 3) is connected: it refuses every review, so a
/// build stops at its blind review and fails with the reason instead of storing a key nobody judged.
pub fn unconnected_grader(_llm: Arc<dyn LLMPort>) -> Box<dyn ReviewGrader> {
    Box::new(Unconnected)
}

struct Unconnected;

#[async_trait]
impl ReviewGrader for Unconnected {
    async fn assess(&self, _review: &Review<'_>) -> Result<Assessment> {
        Err(AppError::InvalidState(
            "The judge that grades a review is not connected to the drill build yet.".into(),
        ))
    }
}
