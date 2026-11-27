//! Prioritizing queued lesson preparation jobs and computing lesson queue rank.
use super::generation_recovery::LESSON_PREPARATION;
use super::jobs::{check_program_open, job_scope, learning_job};
use super::{curriculum_operation, validate_operation_id, LearningCurriculumRepository};
use crate::curriculum::LearningGenerationJob;
use crate::persistence::hash;
use crate::plan_dto::LearningGenerationJobActionRequestDto;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{JobStatus, JobStore};

impl LearningCurriculumRepository {
    /// Moves a queued pending lesson-preparation job to the front of its course
    /// queue by lowering its rank strictly below all other unfinished lesson
    /// jobs for that course. Running jobs are an idempotent no-op.
    pub async fn prioritize_job(
        &self,
        request: &LearningGenerationJobActionRequestDto,
    ) -> Result<LearningGenerationJob> {
        validate_operation_id(&request.operation_id)?;
        uuid::Uuid::parse_str(&request.program_id)
            .map_err(|_| AppError::InvalidInput("Invalid program ID.".into()))?;
        uuid::Uuid::parse_str(&request.job_id)
            .map_err(|_| AppError::InvalidInput("Invalid generation job ID.".into()))?;
        let payload = hash(request)?;
        let operation = curriculum_operation(
            &request.operation_id,
            &request.program_id,
            "prioritize_job",
            &payload,
            "Operation ID was reused with different job action data.",
        );
        let mut tx = self.db.begin_write().await?;
        if let Some(id) = operation.replay::<String>(&mut tx).await? {
            let job = Self::job_on(&mut tx, &id).await?;
            tx.commit().await?;
            return Ok(job);
        }
        check_program_open(&mut tx, &request.program_id, request.expected_revision).await?;
        let scope = job_scope(&mut tx, &request.job_id)
            .await?
            .filter(|scope| scope.program_id == request.program_id)
            .ok_or_else(|| AppError::NotFound("Generation job not found".into()))?;
        let existing = JobStore::find_in(&mut tx, &request.job_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Generation job not found".into()))?;
        if existing.kind != LESSON_PREPARATION {
            return Err(AppError::InvalidState(
                "Only lesson preparation jobs can be prioritized.".into(),
            ));
        }
        match existing.status {
            JobStatus::Running => {
                operation.record(&mut tx, &existing.id).await?;
                let job = learning_job(existing, scope)?;
                tx.commit().await?;
                return Ok(job);
            }
            JobStatus::Pending => {}
            _ => {
                return Err(AppError::InvalidState(
                    "Only queued or running lesson preparation jobs can be prioritized.".into(),
                ));
            }
        }
        let other_min: Option<i64> = sqlx::query_scalar(
            "SELECT MIN(j.rank) FROM jobs j JOIN learning_jobs lj ON lj.job_id=j.id WHERE lj.program_id=? AND j.kind=? AND j.status IN ('pending','running') AND j.id<>? AND j.rank IS NOT NULL",
        )
        .bind(&request.program_id)
        .bind(LESSON_PREPARATION)
        .bind(&request.job_id)
        .fetch_one(&mut *tx)
        .await?;
        let already_strict_min = matches!(
            (existing.rank, other_min),
            (Some(current), Some(other)) if current < other
        ) || matches!((existing.rank, other_min), (Some(_), None));
        if !already_strict_min {
            let all_min: Option<i64> = sqlx::query_scalar(
                "SELECT MIN(j.rank) FROM jobs j JOIN learning_jobs lj ON lj.job_id=j.id WHERE lj.program_id=? AND j.kind=? AND j.status IN ('pending','running') AND j.rank IS NOT NULL",
            )
            .bind(&request.program_id)
            .bind(LESSON_PREPARATION)
            .fetch_one(&mut *tx)
            .await?;
            let new_rank = all_min.unwrap_or(0).saturating_sub(1);
            JobStore::set_rank_in(&mut tx, &request.job_id, new_rank).await?;
        }
        operation.record(&mut tx, &request.job_id).await?;
        let job = Self::job_on(&mut tx, &request.job_id).await?;
        tx.commit().await?;
        Ok(job)
    }
}

/// A lesson's place in its course, counting from 0 through every lesson in
/// course order (module ordinal, then lesson ordinal). It is the rank of the
/// lesson's jobs: the model serves a course's backlog lowest rank first.
pub(crate) async fn lesson_rank_in(
    connection: &mut sqlx::SqliteConnection,
    lesson_id: &str,
) -> Result<Option<i64>> {
    sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM learning_lessons earlier JOIN learning_modules earlier_module ON earlier_module.id=earlier.module_id WHERE earlier.program_id=lesson.program_id AND (earlier_module.ordinal<own_module.ordinal OR (earlier_module.ordinal=own_module.ordinal AND earlier.ordinal<lesson.ordinal))) FROM learning_lessons lesson JOIN learning_modules own_module ON own_module.id=lesson.module_id WHERE lesson.id=?",
    )
    .bind(lesson_id)
    .fetch_optional(connection)
    .await
    .map_err(AppError::from)
}
