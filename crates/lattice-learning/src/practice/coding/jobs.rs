//! Background jobs for coding sessions: building and proving a problem before it is shown, then saving it as an
//! activity whose problem file only the app reads.

#![allow(unused_imports)]
use crate::drills::runner::runner_for;
use crate::embedded_runtime::{BuiltinRuntimes, LearningBuiltinRuntime};
use crate::persistence::{hash, now};
use crate::practical_generation::{GeneratedPracticalActivity, GeneratedPracticalFile, PracticalFileRole};
use crate::practice::coding::dto::StartCodingBuildRequestDto;
use crate::practice::coding::harness::{harness_for, new_nonce, CaseHarness};
use crate::practice::coding::patterns::targets_in;
use crate::practice::coding::problem::{Problem, ProblemDifficulty, PROBLEM_FILE_PATH, SOLUTION_PATH};
use crate::practice::coding::source::{
    BuildSteps, GeneratedProblems, LessonBrief, ProblemRequest, ProblemSource, STEP_SAVING,
};
use crate::practice::practical_dto::{GenerateLearningPracticalActivityRequestDto, LearningPracticalActivityKind};
use crate::practice::practical_repository::LearningPracticalRepository;
use crate::practice_repository::profiles::profile_in;
use crate::quiz_bank_jobs::ModelLoader;
use async_trait::async_trait;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Transactions;
use lattice_core::shared::runtime::jobs::{
    Backoff, JobContext, JobHandler, JobKindConfig, JobOutcome, JobStore, NewJob, RecoveryPolicy,
};
use sqlx::SqlitePool;
use uuid::Uuid;

#[cfg(test)]
mod tests;

/// The job identifier for coding problem builds.
pub const CODING_BUILD: &str = "learning.coding_build";

/// The steps of a coding problem build, for its progress: writing, proving and saving.
pub const CODING_BUILD_STEPS: u32 = 3;

/// How many patterns an easy problem calls for. An easy problem is one idea; a harder one combines two.
const EASY_PATTERNS: usize = 1;

/// How many patterns a harder problem calls for.
const HARDER_PATTERNS: usize = 2;

/// One build per program at a time; a running build starts again after a restart, and a disconnected model defers it
/// rather than failing it.
pub fn coding_job_config() -> JobKindConfig {
    JobKindConfig::new(RecoveryPolicy::Requeue)
        .exclusive_per_subject()
        .backoff(Backoff::default())
}

/// Queues a build of a problem for a program and returns its job id. The same operation id is the same job.
pub async fn queue_coding_build(
    _pool: &SqlitePool,
    _request: &StartCodingBuildRequestDto,
) -> Result<String> {
    Err(AppError::InvalidState("Builds are not queued yet.".into()))
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
impl BuildSteps for JobSteps<'_> {
    async fn step(&self, current: u32, message: &str) -> Result<bool> {
        self.context.progress(current, message, None).await
    }
}

/// Builds coding problems, on the Learning model, pinned for the whole build.
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

/// What a proven problem is saved as.
pub(crate) struct ProblemSave<'a> {
    pub job_id: &'a str,
    pub program_id: &'a str,
    pub lesson_id: &'a str,
    pub focus: Option<&'a str>,
    pub language: &'a str,
    pub model: &'a str,
}

/// Saves a proven problem as a coding activity and returns the activity id.
pub(crate) async fn save_problem(
    _pool: &SqlitePool,
    _save: &ProblemSave<'_>,
    _problem: &Problem,
    _harness: &dyn CaseHarness,
) -> Result<String> {
    Err(AppError::InvalidState("Problems are not saved yet.".into()))
}

#[async_trait]
impl JobHandler for CodingBuildWorker {
    async fn run(&self, _context: &JobContext) -> Result<JobOutcome> {
        Err(AppError::InvalidState("Builds do not run yet.".into()))
    }
}
