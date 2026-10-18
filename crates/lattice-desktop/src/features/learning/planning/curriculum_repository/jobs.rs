//! Lesson generation jobs: starting, cancelling, retrying and reading them with their scope.
use super::generation_recovery::LESSON_PREPARATION;
use super::{
    curriculum_operation, stable_job_operation_id, validate_operation_id, JobScope,
    LearningCurriculumRepository,
};
use crate::features::learning::curriculum::{
    self, LearningGenerationJob, LearningGenerationJobKind, LearningGenerationJobStatus,
};
use crate::features::learning::persistence::{db, decode, hash};
use crate::features::learning::plan_dto::{
    LearningGenerationJobActionRequestDto, StartLearningGenerationJobRequestDto,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{JobRecord, JobStatus, JobStore, NewJob};
use sqlx::Row;

impl LearningCurriculumRepository {
    pub async fn jobs(&self, program_id: &str) -> Result<Vec<LearningGenerationJob>> {
        let mut connection = self.pool.acquire().await.map_err(db)?;
        Self::jobs_on(&mut connection, program_id).await
    }

    pub(super) async fn jobs_on(
        connection: &mut sqlx::SqliteConnection,
        program_id: &str,
    ) -> Result<Vec<LearningGenerationJob>> {
        let scopes = job_scopes(&mut *connection, program_id).await?;
        JobStore::list_for_subject_in(connection, LESSON_PREPARATION, program_id)
            .await?
            .into_iter()
            .filter_map(|job| {
                let scope = scopes.get(&job.id)?.clone();
                Some(learning_job(job, scope))
            })
            .collect()
    }

    /// Explicit lesson preparation may resume an existing retry chain. Concurrent
    /// clicks share one stable retry operation and only one worker can claim it.
    pub async fn start_lesson_job(
        &self,
        request: &StartLearningGenerationJobRequestDto,
    ) -> Result<LearningGenerationJob> {
        if request.progress_total != 1 {
            return Err(AppError::InvalidInput(
                "Select one lesson to prepare.".into(),
            ));
        }
        let mut job = self.start_job(request).await?;
        let jobs = self.jobs(&request.program_id).await?;
        let mut visited = std::collections::HashSet::new();
        while let Some(child) = jobs
            .iter()
            .find(|candidate| candidate.retry_of_job_id.as_deref() == Some(job.id.as_str()))
        {
            if !visited.insert(job.id.clone()) {
                return Err(AppError::InvalidState(
                    "The lesson retry history is invalid.".into(),
                ));
            }
            job = child.clone();
        }
        if matches!(
            job.status,
            LearningGenerationJobStatus::Failed
                | LearningGenerationJobStatus::Interrupted
                | LearningGenerationJobStatus::Cancelled
        ) {
            job = self
                .retry_job(&LearningGenerationJobActionRequestDto {
                    operation_id: stable_job_operation_id(
                        &request.program_id,
                        &format!("retry-{}", job.id),
                        request.expected_revision,
                    ),
                    program_id: request.program_id.clone(),
                    job_id: job.id.clone(),
                    expected_revision: request.expected_revision,
                })
                .await?;
        }
        Ok(job)
    }

    /// Saves a lesson job for the runtime to run. The caller dispatches it.
    pub async fn start_job(
        &self,
        request: &StartLearningGenerationJobRequestDto,
    ) -> Result<LearningGenerationJob> {
        validate_operation_id(&request.operation_id)?;
        if !(1..=3).contains(&request.progress_total) {
            return Err(AppError::InvalidInput(
                "Lesson preparation jobs must contain 1–3 bounded steps.".into(),
            ));
        }
        if request.kind != LearningGenerationJobKind::LessonPreparation {
            return Err(AppError::InvalidInput(
                "Only bounded lesson-preparation jobs are currently supported.".into(),
            ));
        }
        let parsed: serde_json::Value = decode(&request.request_json)?;
        if request.request_json.len() > 64 * 1024 {
            return Err(AppError::InvalidInput(
                "Generation job input is too large.".into(),
            ));
        }
        let lesson_ids = parsed
            .get("lessonIds")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| AppError::InvalidInput("Lesson jobs require lessonIds.".into()))?
            .iter()
            .map(|value| {
                let id = value
                    .as_str()
                    .ok_or_else(|| AppError::InvalidInput("Lesson IDs must be strings.".into()))?;
                uuid::Uuid::parse_str(id)
                    .map_err(|_| AppError::InvalidInput("Invalid lesson ID.".into()))?;
                Ok(id.to_string())
            })
            .collect::<Result<Vec<_>>>()?;
        if lesson_ids.len() != request.progress_total as usize
            || lesson_ids.is_empty()
            || lesson_ids.len() > 3
            || lesson_ids
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != lesson_ids.len()
        {
            return Err(AppError::InvalidInput(
                "A job must name one to three unique lessons matching progressTotal.".into(),
            ));
        }
        let digest = hash(&parsed)?;
        let payload = hash(request)?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;
        let operation = curriculum_operation(
            &request.operation_id,
            &request.program_id,
            "start_job",
            &payload,
            "Operation ID was reused with different job data.",
        );
        if let Some(id) = operation.replay::<String, _>(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return self.job(&id).await;
        }
        let program = sqlx::query(
            "SELECT revision,status,content_revision FROM learning_programs WHERE id=?",
        )
        .bind(&request.program_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Learning program not found".into()))?;
        if program.get::<i64, _>("revision") != request.expected_revision
            || program.get::<String, _>("status") != "active"
        {
            return Err(AppError::InvalidState(
                "Program changed or is not active; reload and retry.".into(),
            ));
        }
        let content_revision: i64 = program.get("content_revision");
        let next_lessons: Vec<String> = sqlx::query_scalar("SELECT l.id FROM learning_lessons l JOIN learning_modules m ON m.id=l.module_id WHERE l.program_id=? AND l.preparation='outline' AND l.completed=0 ORDER BY m.ordinal,l.ordinal")
            .bind(&request.program_id).fetch_all(&mut *tx).await.map_err(db)?;
        if (lesson_ids.len() == 1
            && lesson_ids
                .first()
                .is_some_and(|id| !next_lessons.contains(id)))
            || (lesson_ids.len() > 1
                && next_lessons.get(..lesson_ids.len()) != Some(lesson_ids.as_slice()))
        {
            return Err(AppError::InvalidInput(
                "Select an unfinished outline lesson; batch preparation must follow course order."
                    .into(),
            ));
        }
        // A new UI revision or operation ID must not create duplicate work for
        // a lesson already queued against the same teaching inputs.
        let scopes = job_scopes(&mut tx, &request.program_id).await?;
        let mut active =
            JobStore::list_for_subject_in(&mut tx, LESSON_PREPARATION, &request.program_id)
                .await?
                .into_iter()
                .filter(|job| {
                    !job.status.is_finished()
                        && scopes
                            .get(&job.id)
                            .is_some_and(|scope| scope.content_revision == Some(content_revision))
                })
                .collect::<Vec<_>>();
        active.reverse();
        for existing in active {
            let ids = existing
                .requested
                .get("lessonIds")
                .and_then(serde_json::Value::as_array);
            let overlaps = ids.is_some_and(|ids| {
                ids.iter().any(|id| {
                    id.as_str()
                        .is_some_and(|id| lesson_ids.iter().any(|wanted| wanted == id))
                })
            });
            if !overlaps {
                continue;
            }
            if !ids.is_some_and(|ids| {
                ids.len() == lesson_ids.len()
                    && ids.iter().all(|id| {
                        id.as_str()
                            .is_some_and(|id| lesson_ids.iter().any(|wanted| wanted == id))
                    })
            }) {
                return Err(AppError::InvalidState(
                    "A selected lesson already has preparation in progress. Let that job finish or cancel it before starting an overlapping batch.".into(),
                ));
            }
            operation.record(&mut *tx, &existing.id).await?;
            tx.commit().await.map_err(db)?;
            return self.job(&existing.id).await;
        }
        let admitted = JobStore::submit_in(
            &mut tx,
            &NewJob {
                kind: LESSON_PREPARATION.into(),
                subject_id: Some(request.program_id.clone()),
                operation_id: request.operation_id.clone(),
                payload_hash: digest,
                requested: parsed,
                progress_total: request.progress_total,
                message: "Queued".into(),
            },
        )
        .await?;
        if admitted.created {
            sqlx::query("INSERT INTO learning_jobs(job_id,program_id,base_revision_number,content_revision) VALUES(?,?,?,?)")
                .bind(&admitted.id).bind(&request.program_id).bind(request.expected_revision).bind(content_revision)
                .execute(&mut *tx).await.map_err(db)?;
        }
        operation.record(&mut *tx, &admitted.id).await?;
        tx.commit().await.map_err(db)?;
        self.job(&admitted.id).await
    }

    /// Commits the cancellation. The caller then signals the running worker.
    pub async fn cancel_job(
        &self,
        request: &LearningGenerationJobActionRequestDto,
    ) -> Result<LearningGenerationJob> {
        self.job_action(request, JobAction::Cancel).await
    }

    /// Saves a new attempt that inherits the job's saved work. The caller
    /// dispatches it.
    pub async fn retry_job(
        &self,
        request: &LearningGenerationJobActionRequestDto,
    ) -> Result<LearningGenerationJob> {
        self.job_action(request, JobAction::Retry).await
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
        self.job_action(request, JobAction::ContinueReview).await
    }

    async fn job_action(
        &self,
        request: &LearningGenerationJobActionRequestDto,
        action: JobAction,
    ) -> Result<LearningGenerationJob> {
        validate_operation_id(&request.operation_id)?;
        let payload = hash(request)?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;
        let operation = curriculum_operation(
            &request.operation_id,
            &request.program_id,
            action.operation_kind(),
            &payload,
            "Operation ID was reused with different job action data.",
        );
        if let Some(id) = operation.replay::<String, _>(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return self.job(&id).await;
        }
        let program =
            sqlx::query("SELECT revision,content_revision FROM learning_programs WHERE id=?")
                .bind(&request.program_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                .ok_or_else(|| AppError::NotFound("Learning program not found".into()))?;
        if program.get::<i64, _>("revision") != request.expected_revision {
            return Err(AppError::InvalidState(
                "Program changed; reload and retry.".into(),
            ));
        }
        let scope = job_scope(&mut tx, &request.job_id)
            .await?
            .filter(|scope| scope.program_id == request.program_id)
            .ok_or_else(|| AppError::NotFound("Generation job not found".into()))?;
        let old = JobStore::find_in(&mut tx, &request.job_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Generation job not found".into()))?;
        let result_id = match action {
            JobAction::Cancel => {
                JobStore::cancel_in(&mut tx, &request.job_id).await?;
                request.job_id.clone()
            }
            JobAction::Retry | JobAction::ContinueReview => {
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
                    Some(held_outlines(&mut tx, &request.program_id, &old).await?)
                };
                if scope.content_revision != Some(program.get::<i64, _>("content_revision")) {
                    return Err(AppError::InvalidState(
                        "Course content changed after this job was created. Start preparation again for the updated course.".into(),
                    ));
                }
                let admitted = match lessons {
                    None => {
                        JobStore::retry_in(
                            &mut tx,
                            &request.job_id,
                            &request.operation_id,
                            "Retry queued",
                        )
                        .await?
                    }
                    Some(lessons) => {
                        JobStore::continue_in(
                            &mut tx,
                            &request.job_id,
                            &request.operation_id,
                            "Resuming verification",
                            &serde_json::json!({ "lessonIds": lessons }),
                            u32::try_from(lessons.len()).unwrap_or(u32::MAX),
                        )
                        .await?
                    }
                };
                if admitted.created {
                    sqlx::query("INSERT INTO learning_jobs(job_id,program_id,base_revision_number,content_revision) SELECT ?,program_id,base_revision_number,content_revision FROM learning_jobs WHERE job_id=?")
                        .bind(&admitted.id).bind(&request.job_id).execute(&mut *tx).await.map_err(db)?;
                }
                admitted.id
            }
        };
        operation.record(&mut *tx, &result_id).await?;
        tx.commit().await.map_err(db)?;
        self.job(&result_id).await
    }

    pub async fn job(&self, id: &str) -> Result<LearningGenerationJob> {
        let mut connection = self.pool.acquire().await.map_err(db)?;
        let not_found = || AppError::NotFound("Generation job not found".into());
        let scope = job_scope(&mut connection, id)
            .await?
            .ok_or_else(not_found)?;
        let job = JobStore::find_in(&mut connection, id)
            .await?
            .ok_or_else(not_found)?;
        learning_job(job, scope)
    }

    pub(in crate::features::learning) async fn job_content_is_current(
        &self,
        id: &str,
    ) -> Result<bool> {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM learning_jobs j JOIN learning_programs p ON p.id=j.program_id WHERE j.job_id=? AND j.content_revision=p.content_revision)")
            .bind(id).fetch_one(&self.pool).await.map_err(db)
    }

    pub(super) fn jobs_store(&self) -> JobStore {
        JobStore::new(self.pool.clone())
    }
}

/// What a job action does: the user's cancel, retry or resume (§6.2).
#[derive(Clone, Copy, PartialEq, Eq)]
enum JobAction {
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
        .and_then(crate::features::learning::content_verification::held_lessons)
        .filter(|_| job.status == JobStatus::Completed)
        .ok_or_else(|| {
            AppError::InvalidState("No lesson of this job waits for your review.".into())
        })?;
    let mut lessons = Vec::new();
    for lesson_id in held {
        let outline: Option<bool> = sqlx::query_scalar(
            "SELECT preparation='outline' FROM learning_lessons WHERE id=? AND program_id=?",
        )
        .bind(&lesson_id)
        .bind(program_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(db)?;
        if outline == Some(true) {
            lessons.push(lesson_id);
        }
    }
    if lessons.is_empty() {
        return Err(AppError::InvalidState(
            "Every lesson this job held for review was prepared since.".into(),
        ));
    }
    Ok(lessons)
}

fn job_scope_row(row: &sqlx::sqlite::SqliteRow) -> JobScope {
    JobScope {
        program_id: row.get("program_id"),
        base_revision_number: row.get("base_revision_number"),
        content_revision: row.get("content_revision"),
    }
}

pub(super) async fn job_scope(
    connection: &mut sqlx::SqliteConnection,
    job_id: &str,
) -> Result<Option<JobScope>> {
    Ok(sqlx::query(
        "SELECT program_id,base_revision_number,content_revision FROM learning_jobs WHERE job_id=?",
    )
    .bind(job_id)
    .fetch_optional(connection)
    .await
    .map_err(db)?
    .as_ref()
    .map(job_scope_row))
}

async fn job_scopes(
    connection: &mut sqlx::SqliteConnection,
    program_id: &str,
) -> Result<std::collections::HashMap<String, JobScope>> {
    Ok(sqlx::query("SELECT job_id,program_id,base_revision_number,content_revision FROM learning_jobs WHERE program_id=?")
        .bind(program_id)
        .fetch_all(connection)
        .await
        .map_err(db)?
        .iter()
        .map(|row| (row.get("job_id"), job_scope_row(row)))
        .collect())
}

fn learning_job(job: JobRecord, scope: JobScope) -> Result<LearningGenerationJob> {
    use LearningGenerationJobStatus as S;
    if job.kind != LESSON_PREPARATION {
        return Err(AppError::Database("Invalid generation job kind".into()));
    }
    let job = LearningGenerationJob {
        id: job.id,
        program_id: scope.program_id,
        operation_id: job.operation_id,
        kind: LearningGenerationJobKind::LessonPreparation,
        payload_sha256: job.payload_hash,
        base_revision_number: scope.base_revision_number as u32,
        status: match job.status {
            JobStatus::Pending => S::Pending,
            JobStatus::Running => S::Running,
            JobStatus::Completed => S::Completed,
            JobStatus::Failed => S::Failed,
            JobStatus::Cancelled => S::Cancelled,
            JobStatus::Interrupted => S::Interrupted,
        },
        progress_completed: job.progress_current,
        progress_total: job.progress_total,
        progress_message: job.progress_message,
        activity: job
            .activity
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| AppError::Serialization(e.to_string()))?,
        result_id: job.result_ref,
        error: job.error_message,
        retry_of_job_id: job.retry_of_job_id,
        created_at: job.created_at,
        started_at: job.started_at,
        finished_at: job.finished_at,
    };
    curriculum::validate_generation_job(&job)?;
    Ok(job)
}
