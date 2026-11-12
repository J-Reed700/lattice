//! The `course_plan` job: plans what each lesson teaches as soon as the
//! outline exists, so the learner can read the topics before any lesson is
//! written.

use super::repository::{self, content_revision, latest_attempt, outline_lessons, saved_entries};
use super::{ensure, outline_revision};
use crate::lessons::course_setup::CourseSetups;
use crate::persistence::db;
use crate::quiz_bank_jobs::ModelLoader;
use crate::repository::LearningRepository;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;
use lattice_core::shared::runtime::jobs::{
    Backoff, JobContext, JobHandler, JobKindConfig, JobOutcome, JobStatus, JobStore, NewJob,
    RecoveryPolicy,
};
use serde_json::json;
use sha2::{Digest, Sha256};

pub const COURSE_PLAN: &str = "learning.course_plan";

/// Not exclusive: the model call is background work, and callers of one course
/// plan it once through the course lock.
pub fn course_plan_job_config() -> JobKindConfig {
    JobKindConfig::new(RecoveryPolicy::Requeue).backoff(Backoff::default())
}

fn derived_uuid(seed: &str) -> String {
    let hash = Sha256::digest(seed.as_bytes());
    let mut bytes = [0u8; 16];
    for (slot, byte) in bytes.iter_mut().zip(hash.iter()) {
        *slot = *byte;
    }
    uuid::Builder::from_random_bytes(bytes)
        .into_uuid()
        .to_string()
}

fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// Queue planning the course's current outline: the job that is waiting or
/// running for it, a new attempt of one that failed or stopped, or a new job.
/// `None` when every lesson already has its topics. The caller tells the
/// runtime once the job commits.
pub async fn queue(database: &Database, program_id: &str) -> Result<Option<String>> {
    let mut tx = database.begin_write().await.map_err(db)?;
    let (status, revision) = repository::program_state(&mut tx, program_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Learning program not found".into()))?;
    if status != "draft" && status != "active" {
        return Err(AppError::InvalidInput(
            "Only a draft or an active course can have its topics planned.".into(),
        ));
    }
    let outline = content_revision(&mut tx, program_id).await?;
    let lessons = outline_lessons(&mut tx, program_id).await?;
    let saved = saved_entries(&mut tx, program_id, outline).await?;
    if !lessons.is_empty() && lessons.iter().all(|lesson| saved.contains_key(&lesson.id)) {
        return Ok(None);
    }
    let seed = format!("{COURSE_PLAN}:{program_id}:{outline}");
    let latest = latest_attempt(&mut tx, program_id, outline).await?;
    let (id, created, copy_from) = match latest {
        Some(job) if !job.status.is_finished() => (job.id, false, None),
        Some(job)
            if matches!(
                job.status,
                JobStatus::Failed | JobStatus::Interrupted | JobStatus::Cancelled
            ) =>
        {
            let admitted = JobStore::retry_in(
                &mut tx,
                &job.id,
                &derived_uuid(&format!("{seed}:retry:{}", job.id)),
                "Waiting to plan the course",
            )
            .await?;
            (admitted.id, admitted.created, Some(job.id))
        }
        other => {
            let operation_id = match other {
                Some(job) => derived_uuid(&format!("{seed}:again:{}", job.id)),
                None => derived_uuid(&seed),
            };
            let requested = json!({"programId":program_id,"outlineRevision":outline});
            let admitted = JobStore::submit_in(
                &mut tx,
                &NewJob {
                    kind: COURSE_PLAN.into(),
                    subject_id: Some(program_id.into()),
                    operation_id,
                    payload_hash: digest(&requested.to_string()),
                    requested,
                    progress_total: 1,
                    message: "Waiting to plan the course".into(),
                },
            )
            .await?;
            if admitted.created {
                repository::insert_job_row(
                    &mut tx,
                    &admitted.id,
                    program_id,
                    revision.max(0),
                    outline,
                )
                .await?;
            }
            (admitted.id, false, None)
        }
    };
    if let (true, Some(old)) = (created, copy_from) {
        repository::copy_job_row(&mut tx, &id, &old).await?;
    }
    tx.commit().await.map_err(db)?;
    Ok(Some(id))
}

/// Plans the course's topics on the Learning model.
#[derive(Clone)]
pub struct CoursePlanWorker {
    pub database: Database,
    pub load_llm: ModelLoader,
    pub course_setups: CourseSetups,
}

#[async_trait::async_trait]
impl JobHandler for CoursePlanWorker {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        let job = context.job();
        let program_id = job
            .requested
            .get("programId")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| AppError::InvalidInput("The plan job names no course.".into()))?
            .to_owned();
        let revision = job
            .requested
            .get("outlineRevision")
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(|| AppError::InvalidInput("The plan job names no outline.".into()))?;
        let _activity = lattice_core::shared::runtime::user_activity::UserActivity::begin(
            "Planning what each lesson teaches",
        );
        let program = LearningRepository::new(self.database.clone())
            .get(&program_id)
            .await?;
        if outline_revision(&self.database, &program_id).await? != revision {
            return Ok(JobOutcome::Completed {
                result_ref: format!("course_plan:stale:{revision}"),
                message: "The outline changed, so this plan is no longer needed.".into(),
            });
        }
        let Some(pin) = context.until_cancelled((self.load_llm)()).await else {
            return Ok(JobOutcome::Stopped);
        };
        let pin = pin?;
        let Some(planned) = context
            .until_cancelled(ensure(
                &self.database,
                pin.llm.as_ref(),
                &program,
                revision,
                &self.course_setups,
            ))
            .await
        else {
            return Ok(JobOutcome::Stopped);
        };
        planned?;
        Ok(JobOutcome::Completed {
            result_ref: format!("course_plan:{revision}"),
            message: "Each lesson's topics are planned.".into(),
        })
    }
}
