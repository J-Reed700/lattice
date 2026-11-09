//! The `course_plan` job: plans what each lesson teaches as soon as the
//! outline exists, so the learner can read the topics before any lesson is
//! written.

use crate::quiz_bank_jobs::ModelLoader;
use lattice_core::shared::error::Result;
use lattice_core::shared::runtime::jobs::{
    Backoff, JobContext, JobHandler, JobKindConfig, JobOutcome, RecoveryPolicy,
};
use sqlx::SqlitePool;

pub const COURSE_PLAN: &str = "learning.course_plan";

/// Not exclusive: the model call is background work, and an overlapping plan
/// is saved once by the first to commit.
pub fn course_plan_job_config() -> JobKindConfig {
    JobKindConfig::new(RecoveryPolicy::Requeue).backoff(Backoff::default())
}

/// Queue planning the course's current outline.
pub async fn queue(_pool: &SqlitePool, _program_id: &str) -> Result<Option<String>> {
    Ok(None)
}

/// Plans the course's topics on the Learning model.
#[derive(Clone)]
pub struct CoursePlanWorker {
    pub pool: SqlitePool,
    pub load_llm: ModelLoader,
}

#[async_trait::async_trait]
impl JobHandler for CoursePlanWorker {
    async fn run(&self, _context: &JobContext) -> Result<JobOutcome> {
        Ok(JobOutcome::Completed {
            result_ref: "course_plan:none".into(),
            message: "Nothing was planned.".into(),
        })
    }
}
