//! Generation-job state transitions: start, advance, finish, fail, cancel and
//! recovery after a restart, each validated against the job rules.

use super::{
    id, text, LearningGenerationJob, LearningGenerationJobKind, LearningGenerationJobStatus,
};
use crate::shared::error::{AppError, Result};

pub fn validate_generation_job(job: &LearningGenerationJob) -> Result<()> {
    id(&job.id, "generation job")?;
    id(&job.program_id, "program")?;
    id(&job.operation_id, "generation operation")?;
    if job.payload_sha256.len() != 64
        || !job
            .payload_sha256
            .bytes()
            .all(|value| value.is_ascii_hexdigit())
    {
        return Err(AppError::InvalidInput(
            "Generation jobs require a SHA-256 payload digest.".into(),
        ));
    }
    if job.progress_total == 0 || job.progress_completed > job.progress_total {
        return Err(AppError::InvalidInput(
            "Generation-job progress is invalid.".into(),
        ));
    }
    let finished = matches!(
        job.status,
        LearningGenerationJobStatus::Completed
            | LearningGenerationJobStatus::Failed
            | LearningGenerationJobStatus::Cancelled
            | LearningGenerationJobStatus::Interrupted
    );
    if finished != job.finished_at.is_some() {
        return Err(AppError::InvalidInput(
            "Generation-job completion time does not match its status.".into(),
        ));
    }
    if (job.status == LearningGenerationJobStatus::Completed) != job.result_id.is_some() {
        return Err(AppError::InvalidInput(
            "Only completed generation jobs may publish a result.".into(),
        ));
    }
    if job.status == LearningGenerationJobStatus::Completed
        && job.progress_completed != job.progress_total
    {
        return Err(AppError::InvalidInput(
            "Completed generation jobs must report complete progress.".into(),
        ));
    }
    let requires_error = matches!(
        job.status,
        LearningGenerationJobStatus::Failed | LearningGenerationJobStatus::Interrupted
    );
    let waiting_after_outage = job.status == LearningGenerationJobStatus::Pending
        && job.kind == LearningGenerationJobKind::LessonPreparation;
    if (requires_error && job.error.is_none())
        || (!requires_error && !waiting_after_outage && job.error.is_some())
    {
        return Err(AppError::InvalidInput(
            "Errors require a failed or interrupted job, or a lesson waiting to reconnect.".into(),
        ));
    }
    if let Some(error) = &job.error {
        text(error, "Generation error", 2_000)?;
    }
    Ok(())
}

pub fn start_generation_job(job: &mut LearningGenerationJob, now: i64) -> Result<()> {
    if !matches!(
        job.status,
        LearningGenerationJobStatus::Pending | LearningGenerationJobStatus::Interrupted
    ) {
        return Err(AppError::InvalidState(
            "Only pending or interrupted generation jobs can start.".into(),
        ));
    }
    job.status = LearningGenerationJobStatus::Running;
    job.started_at = Some(now);
    job.finished_at = None;
    job.error = None;
    job.result_id = None;
    validate_generation_job(job)
}

pub fn advance_generation_job(job: &mut LearningGenerationJob, completed: u32) -> Result<()> {
    if job.status != LearningGenerationJobStatus::Running || completed < job.progress_completed {
        return Err(AppError::InvalidState(
            "Only a running job can advance, and progress cannot move backward.".into(),
        ));
    }
    job.progress_completed = completed.min(job.progress_total);
    validate_generation_job(job)
}

pub fn finish_generation_job(
    job: &mut LearningGenerationJob,
    result_id: String,
    now: i64,
) -> Result<()> {
    if job.status != LearningGenerationJobStatus::Running {
        return Err(AppError::InvalidState(
            "Only a running generation job can publish a result.".into(),
        ));
    }
    id(&result_id, "generation result")?;
    job.status = LearningGenerationJobStatus::Completed;
    job.progress_completed = job.progress_total;
    job.result_id = Some(result_id);
    job.finished_at = Some(now);
    job.error = None;
    validate_generation_job(job)
}

pub fn fail_generation_job(job: &mut LearningGenerationJob, error: String, now: i64) -> Result<()> {
    if job.status != LearningGenerationJobStatus::Running {
        return Err(AppError::InvalidState(
            "Only a running generation job can fail.".into(),
        ));
    }
    text(&error, "Generation error", 2_000)?;
    job.status = LearningGenerationJobStatus::Failed;
    job.error = Some(error);
    job.result_id = None;
    job.finished_at = Some(now);
    validate_generation_job(job)
}

pub fn cancel_generation_job(job: &mut LearningGenerationJob, now: i64) -> Result<()> {
    if !matches!(
        job.status,
        LearningGenerationJobStatus::Pending
            | LearningGenerationJobStatus::Running
            | LearningGenerationJobStatus::Interrupted
    ) {
        return Err(AppError::InvalidState(
            "This generation job has already finished.".into(),
        ));
    }
    job.status = LearningGenerationJobStatus::Cancelled;
    job.result_id = None;
    job.error = None;
    job.finished_at = Some(now);
    validate_generation_job(job)
}

pub fn recover_interrupted_jobs(jobs: &mut [LearningGenerationJob]) {
    for job in jobs {
        if job.status == LearningGenerationJobStatus::Running {
            job.status = LearningGenerationJobStatus::Interrupted;
            job.result_id = None;
            job.error = Some("Generation stopped before publishing a result.".into());
            job.finished_at = Some(chrono::Utc::now().timestamp_millis());
        }
    }
}
