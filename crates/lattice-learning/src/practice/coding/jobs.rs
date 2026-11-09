//! Background jobs for coding sessions: building and proving a problem before it is shown.

use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::embedded_runtime::BuiltinRuntimes;
use crate::practice::coding::source::BuildSteps;
use crate::quiz_bank_jobs::ModelLoader;
use lattice_core::shared::error::Result;
use lattice_core::shared::runtime::jobs::{
    Backoff, JobContext, JobHandler, JobKindConfig, JobOutcome, RecoveryPolicy,
};

/// The job identifier for coding problem builds.
pub const CODING_BUILD: &str = "learning.coding_build";

/// The number of progress steps in a coding problem build.
pub const CODING_BUILD_STEPS: u32 = 3;

/// Job configuration for coding problem builds: exclusive per subject and requeued on failure.
pub fn coding_job_config() -> JobKindConfig {
    JobKindConfig::new(RecoveryPolicy::Requeue)
        .exclusive_per_subject()
        .backoff(Backoff::default())
}

/// Bridges `BuildSteps` to `JobContext::progress`.
pub struct JobSteps<'a> {
    pub context: &'a JobContext,
}

impl<'a> JobSteps<'a> {
    pub fn new(context: &'a JobContext) -> Self {
        Self { context }
    }
}

#[async_trait]
impl<'a> BuildSteps for JobSteps<'a> {
    async fn step(&self, index: u32, message: &str) -> Result<bool> {
        self.context.progress(index, message, None).await
    }
}

/// Worker that executes coding problem build jobs.
#[derive(Clone)]
pub struct CodingBuildWorker {
    pub pool: SqlitePool,
    pub load_llm: ModelLoader,
    pub runtimes: BuiltinRuntimes,
}

impl CodingBuildWorker {
    pub fn new(pool: SqlitePool, load_llm: ModelLoader, runtimes: BuiltinRuntimes) -> Self {
        Self {
            pool,
            load_llm,
            runtimes,
        }
    }
}

#[async_trait]
impl JobHandler for CodingBuildWorker {
    async fn run(&self, _context: &JobContext) -> Result<JobOutcome> {
        // Stage 1 non-panicking stub: returns completed with no output.
        Ok(JobOutcome::Completed {
            result_ref: None,
            message: String::new(),
        })
    }
}
