//! Cancellation, retry, and review-continuation actions for generation jobs.

use super::super::{curriculum_operation, validate_operation_id, LearningCurriculumRepository};
use super::queries::{job_scope, preparable_in};
use crate::curriculum::LearningGenerationJob;
use crate::persistence::hash;
use crate::plan_dto::LearningGenerationJobActionRequestDto;
use crate::planning::preparation_request::PreparationRequest;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::WriteTx;
use lattice_core::shared::runtime::jobs::{JobRecord, JobStatus, JobStore};
use sqlx::Row;

/// Result of read-only cancellation preflight before tripping worker tokens.
#[derive(Debug, Clone)]
pub enum CancelPreflight {
    /// Matching operation already committed; return the saved job without
    /// tripping any runtime token or taking a write turn.
    Replayed(Box<LearningGenerationJob>),
    /// Inputs, program revision, ownership, and status permit cancellation.
    Proceed,
}

enum ValidatedJobAction {
    Replayed(String),
    Ready {
        payload: String,
        content_is_current: bool,
    },
}

impl LearningCurriculumRepository {
    /// Validates a cancellation request via read-only query (`begin_read`)
    /// before tripping any in-memory worker token or queuing on write turns.
    pub async fn preflight_cancel(
        &self,
        request: &LearningGenerationJobActionRequestDto,
    ) -> Result<CancelPreflight> {
        let mut read = self.db.begin_read().await?;
        let preflight =
            match Self::validate_job_action_on(&mut read, request, JobAction::Cancel).await? {
                ValidatedJobAction::Replayed(id) => {
                    CancelPreflight::Replayed(Box::new(Self::job_on(&mut read, &id).await?))
                }
                ValidatedJobAction::Ready { .. } => {
                    JobStore::ensure_cancellable_in(&mut read, &request.job_id).await?;
                    CancelPreflight::Proceed
                }
            };
        read.commit().await?;
        Ok(preflight)
    }

    /// Commits the cancellation on the urgent write lane. Callers should run
    /// [`Self::preflight_cancel`] and signal [`JobRuntime::begin_cancel`]
    /// before calling this, then [`JobRuntime::finish_cancel`] or
    /// [`JobRuntime::abort_cancel`] after.
    pub async fn cancel_job(
        &self,
        request: &LearningGenerationJobActionRequestDto,
    ) -> Result<LearningGenerationJob> {
        let tx = self.db.urgent().begin_write().await?;
        Self::commit_job_action(tx, request, JobAction::Cancel).await
    }

    /// Commits multiple cancellations in a single urgent write transaction.
    ///
    /// Already-terminal jobs (`Completed`, `Failed`, `Cancelled`) are skipped
    /// so a concurrent finish does not roll back pauses for active siblings,
    /// while invalid inputs, missing jobs, or program revision conflicts still
    /// fail the batch atomically. Callers cancelling jobs tracked by a live
    /// runtime should pair this with `JobRuntime::begin_cancel_many` and
    /// `finish_cancel_many` / `abort_cancel_many`.
    pub async fn cancel_jobs(
        &self,
        requests: &[LearningGenerationJobActionRequestDto],
    ) -> Result<Vec<LearningGenerationJob>> {
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        let mut tx = self.db.urgent().begin_write().await?;
        let mut jobs = Vec::with_capacity(requests.len());
        for request in requests {
            let id = Self::cancel_batch_item_on(&mut tx, request).await?;
            jobs.push(Self::job_on(&mut tx, &id).await?);
        }
        tx.commit().await?;
        Ok(jobs)
    }

    /// Saves a new attempt that inherits the job's saved work. The caller
    /// dispatches it.
    pub async fn retry_job(
        &self,
        request: &LearningGenerationJobActionRequestDto,
    ) -> Result<LearningGenerationJob> {
        self.standard_job_action(request, JobAction::Retry).await
    }

    /// Saves a new attempt of a completed job whose lessons wait for the
    /// user's review (§6.2 Resume): it prepares the held lessons still in
    /// outline from their saved drafts and checkpoints. Whether anything
    /// changed since the job stopped is the caller's rule. The caller
    /// dispatches it.
    pub async fn continue_review_job(
        &self,
        request: &LearningGenerationJobActionRequestDto,
    ) -> Result<LearningGenerationJob> {
        self.standard_job_action(request, JobAction::ContinueReview)
            .await
    }

    async fn standard_job_action(
        &self,
        request: &LearningGenerationJobActionRequestDto,
        action: JobAction,
    ) -> Result<LearningGenerationJob> {
        let tx = self.db.begin_write().await?;
        Self::commit_job_action(tx, request, action).await
    }

    async fn commit_job_action(
        mut tx: WriteTx,
        request: &LearningGenerationJobActionRequestDto,
        action: JobAction,
    ) -> Result<LearningGenerationJob> {
        let id = Self::job_action_on(&mut tx, request, action).await?;
        let job = Self::job_on(&mut tx, &id).await?;
        tx.commit().await?;
        Ok(job)
    }

    fn action_operation<'a>(
        request: &'a LearningGenerationJobActionRequestDto,
        action: JobAction,
        payload: &'a str,
    ) -> crate::operations::Operation<'a> {
        curriculum_operation(
            &request.operation_id,
            &request.program_id,
            action.operation_kind(),
            payload,
            "Operation ID was reused with different job action data.",
        )
    }

    async fn validate_job_action_on(
        connection: &mut sqlx::SqliteConnection,
        request: &LearningGenerationJobActionRequestDto,
        action: JobAction,
    ) -> Result<ValidatedJobAction> {
        validate_operation_id(&request.operation_id)?;
        let payload = hash(request)?;
        let operation = Self::action_operation(request, action, &payload);
        if let Some(id) = operation.replay::<String>(connection).await? {
            return Ok(ValidatedJobAction::Replayed(id));
        }
        let program =
            sqlx::query("SELECT revision,content_revision FROM learning_programs WHERE id=?")
                .bind(&request.program_id)
                .fetch_optional(&mut *connection)
                .await?
                .ok_or_else(|| AppError::NotFound("Learning program not found".into()))?;
        if program.get::<i64, _>("revision") != request.expected_revision {
            return Err(AppError::Conflict(
                "Program changed; reload and retry.".into(),
            ));
        }
        let scope = job_scope(connection, &request.job_id)
            .await?
            .filter(|scope| scope.program_id == request.program_id)
            .ok_or_else(|| AppError::NotFound("Generation job not found".into()))?;
        Ok(ValidatedJobAction::Ready {
            payload,
            content_is_current: scope.content_revision
                == Some(program.get::<i64, _>("content_revision")),
        })
    }

    async fn cancel_batch_item_on(
        tx: &mut WriteTx,
        request: &LearningGenerationJobActionRequestDto,
    ) -> Result<String> {
        let payload = match Self::validate_job_action_on(tx, request, JobAction::Cancel).await? {
            ValidatedJobAction::Replayed(id) => return Ok(id),
            ValidatedJobAction::Ready { payload, .. } => payload,
        };
        if JobStore::cancel_if_active_in(tx, &request.job_id).await? {
            Self::action_operation(request, JobAction::Cancel, &payload)
                .record(tx, &request.job_id)
                .await?;
        }
        Ok(request.job_id.clone())
    }

    pub(super) async fn job_action_on(
        tx: &mut WriteTx,
        request: &LearningGenerationJobActionRequestDto,
        action: JobAction,
    ) -> Result<String> {
        let (payload, content_is_current) =
            match Self::validate_job_action_on(tx, request, action).await? {
                ValidatedJobAction::Replayed(id) => return Ok(id),
                ValidatedJobAction::Ready {
                    payload,
                    content_is_current,
                } => (payload, content_is_current),
            };
        let result_id = match action {
            JobAction::Cancel => {
                JobStore::cancel_in(tx, &request.job_id).await?;
                request.job_id.clone()
            }
            JobAction::Retry | JobAction::ContinueReview => {
                let old = JobStore::find_summary_in(tx, &request.job_id)
                    .await?
                    .ok_or_else(|| AppError::NotFound("Generation job not found".into()))?;
                let lessons = if action == JobAction::Retry {
                    if !matches!(
                        old.status,
                        JobStatus::Failed | JobStatus::Interrupted | JobStatus::Cancelled
                    ) {
                        return Err(AppError::InvalidState(
                            "Only failed, interrupted, or cancelled jobs can be retried.".into(),
                        ));
                    }
                    None
                } else {
                    Some(held_outlines(tx, &request.program_id, &old).await?)
                };
                if !content_is_current {
                    return Err(AppError::InvalidState(
                        "Course content changed after this job was created. Start preparation again for the updated course.".into(),
                    ));
                }
                let admitted = match lessons {
                    None => {
                        JobStore::retry_in(
                            tx,
                            &request.job_id,
                            &request.operation_id,
                            "Retry queued",
                        )
                        .await?
                    }
                    Some(lessons) => {
                        JobStore::continue_in(
                            tx,
                            &request.job_id,
                            &request.operation_id,
                            "Resuming verification",
                            &serde_json::to_value(PreparationRequest {
                                lesson_ids: lessons.clone(),
                            })?,
                            u32::try_from(lessons.len()).unwrap_or(u32::MAX),
                        )
                        .await?
                    }
                };
                if admitted.created {
                    sqlx::query("INSERT INTO learning_jobs(job_id,program_id,base_revision_number,content_revision) SELECT ?,program_id,base_revision_number,content_revision FROM learning_jobs WHERE job_id=?")
                        .bind(&admitted.id).bind(&request.job_id).execute(&mut **tx).await?;
                }
                admitted.id
            }
        };
        Self::action_operation(request, action, &payload)
            .record(tx, &result_id)
            .await?;
        Ok(result_id)
    }
}

/// What a job action does: the user's cancel, retry or resume (§6.2).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum JobAction {
    Cancel,
    Retry,
    ContinueReview,
}

impl JobAction {
    fn operation_kind(self) -> &'static str {
        match self {
            Self::Cancel => "cancel_job",
            Self::Retry => "retry_job",
            Self::ContinueReview => "continue_review_job",
        }
    }
}

/// The lessons a completed job left waiting for the user's review that are
/// still outlines: what its continuation prepares (§6.2).
async fn held_outlines(
    connection: &mut sqlx::SqliteConnection,
    program_id: &str,
    job: &JobRecord,
) -> Result<Vec<String>> {
    let held = job
        .result_ref
        .as_deref()
        .and_then(crate::content_verification::held_lessons)
        .filter(|_| job.status == JobStatus::Completed)
        .ok_or_else(|| {
            AppError::InvalidState("No lesson of this job waits for your review.".into())
        })?;
    let preparable = preparable_in(connection, program_id, &held).await?;
    let lessons: Vec<String> = held
        .into_iter()
        .filter(|id| preparable.contains(id))
        .collect();
    if lessons.is_empty() {
        return Err(AppError::InvalidState(
            "Every lesson this job held for review was prepared since.".into(),
        ));
    }
    Ok(lessons)
}
