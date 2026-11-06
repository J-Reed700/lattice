//! The two drill jobs. `learning.drill_build` builds one drill from clean code, planting and proving its defects, and
//! stores it in the bank (docs/design/2026-11-04-code-review-drills.md section 2). `learning.drill_grading` grades one
//! submitted attempt against the drill's key, running or judging the learner's fix (section 4).

use super::attempt_grading::{give_up, AttemptGrading};
use super::attempts;
use super::build::DrillBuild;
use super::grader::ReviewGrader;
use super::grading::JudgeGrader;
use super::runner::runner_for;
use super::spec::DrillSpec;
use crate::features::learning::embedded_runtime::BuiltinRuntimes;
use crate::features::learning::quiz_bank_jobs::ModelLoader;
use async_trait::async_trait;
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{
    Backoff, JobContext, JobHandler, JobKindConfig, JobOutcome, JobRecord, RecoveryPolicy,
};
use sqlx::SqlitePool;

pub const DRILL_BUILD: &str = "learning.drill_build";
pub const DRILL_GRADING: &str = "learning.drill_grading";

/// The steps of a grading job, for its progress: the review, the fix and the save.
pub const GRADING_STEPS: u32 = 3;

/// One build per language at a time; a running build resumes from its checkpoints after a restart, and a
/// disconnected model defers it rather than failing it.
pub fn drill_job_config() -> JobKindConfig {
    JobKindConfig::new(RecoveryPolicy::Requeue)
        .exclusive_per_subject()
        .backoff(Backoff::default())
}

/// Makes the grader of a build's blind review on the model the build runs on.
pub type GraderFactory = for<'a> fn(&'a dyn LLMPort) -> Box<dyn ReviewGrader + 'a>;

/// The judge that grades a build's blind review on the model the build runs on: the grader every review goes
/// through, whoever made it.
pub fn judge_grader(llm: &dyn LLMPort) -> Box<dyn ReviewGrader + '_> {
    Box::new(JudgeGrader::new(llm))
}

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
        let llm = pin.llm.as_ref();
        let runner = runner_for(&spec.language, &self.runtimes);
        let grader = (self.grader)(llm);
        DrillBuild::new(
            llm,
            runner.as_deref(),
            grader.as_ref(),
            &self.pool,
            context.cancellation().clone(),
        )
        .run(context, &spec)
        .await
    }
}

/// One grading job per attempt at a time; a running job grades again from the start after a restart, and a stopped one
/// leaves its attempt submitted.
pub fn grading_job_config() -> JobKindConfig {
    JobKindConfig::new(RecoveryPolicy::Requeue)
        .exclusive_per_subject()
        .backoff(Backoff::default())
}

/// Grades submitted attempts, on the Learning model, pinned for the whole job.
#[derive(Clone)]
pub struct DrillGradingWorker {
    pub pool: SqlitePool,
    pub load_llm: ModelLoader,
    pub runtimes: BuiltinRuntimes,
    pub grader: GraderFactory,
}

/// The attempt a job was queued to grade.
pub fn attempt_of(job: &JobRecord) -> Result<String> {
    job.requested
        .get("attemptId")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| AppError::InvalidInput("The grading job names no review to grade.".into()))
}

#[async_trait]
impl JobHandler for DrillGradingWorker {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        let id = attempt_of(context.job())?;
        let Some((attempt, stored)) = attempts::waiting(&self.pool, &id).await? else {
            return Ok(JobOutcome::Completed {
                result_ref: format!("attempt:{id}"),
                message: "No review is waiting for this grade.".into(),
            });
        };
        let Some(pin) = context.until_cancelled((self.load_llm)()).await else {
            return Ok(JobOutcome::Stopped);
        };
        let pin = match pin {
            Ok(pin) => pin,
            Err(error) => return give_up(&self.pool, context, &id, None, &error).await,
        };
        let llm = pin.llm.as_ref();
        let runner = runner_for(&stored.drill.language, &self.runtimes);
        let grader = (self.grader)(llm);
        AttemptGrading::new(
            llm,
            runner.as_deref(),
            grader.as_ref(),
            &self.pool,
            context.cancellation().clone(),
        )
        .run(context, &attempt, &stored)
        .await
    }
}
