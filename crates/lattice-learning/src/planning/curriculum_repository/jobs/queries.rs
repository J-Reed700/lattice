//! Read queries and projection mapping for learning generation jobs.

use super::super::generation_recovery::{ENDED_BY_UPDATE, LESSON_PREPARATION};
use super::super::{JobScope, LearningCurriculumRepository};
use super::PREPARABLE_LESSON;
use crate::course_plan::COURSE_PLAN;
use crate::curriculum::{
    self, LearningGenerationJob, LearningGenerationJobKind, LearningGenerationJobStatus,
};
use crate::planning::preparation_request::PreparationRequest;
use crate::quiz_bank_jobs::{QUIZ_BANK, QUIZ_GRADING};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{JobRecord, JobStatus, JobStore};
use sqlx::Row;
use std::collections::HashSet;

impl LearningCurriculumRepository {
    pub async fn jobs(&self, program_id: &str) -> Result<Vec<LearningGenerationJob>> {
        let mut read = self.db.begin_read().await?;
        let jobs = Self::jobs_on(&mut read, program_id).await?;
        read.commit().await?;
        Ok(jobs)
    }

    pub(in super::super) async fn jobs_on(
        connection: &mut sqlx::SqliteConnection,
        program_id: &str,
    ) -> Result<Vec<LearningGenerationJob>> {
        let scopes = job_scopes(&mut *connection, program_id).await?;
        let mut jobs = Vec::new();
        for kind in [LESSON_PREPARATION, QUIZ_BANK, QUIZ_GRADING, COURSE_PLAN] {
            let mut of_kind =
                JobStore::list_summaries_for_subject_in(&mut *connection, kind, program_id).await?;
            of_kind.reverse();
            jobs.extend(of_kind);
        }
        jobs.sort_by_key(|job| job.created_at);
        jobs.into_iter()
            .filter_map(|job| {
                let scope = scopes.get(&job.id)?.clone();
                Some(learning_job(job, scope))
            })
            .collect()
    }

    /// The lesson-preparation jobs of `content_revision` that have not finished,
    /// newest first, with the lessons each names.
    pub(crate) async fn unfinished_preparations(
        connection: &mut sqlx::SqliteConnection,
        program_id: &str,
        content_revision: i64,
    ) -> Result<Vec<(JobRecord, PreparationRequest)>> {
        let scopes = job_scopes(&mut *connection, program_id).await?;
        JobStore::list_summaries_for_subject_in(&mut *connection, LESSON_PREPARATION, program_id)
            .await?
            .into_iter()
            .filter(|job| {
                !job.status.is_finished()
                    && scopes
                        .get(&job.id)
                        .is_some_and(|scope| scope.content_revision == Some(content_revision))
            })
            .map(|job| {
                let request = PreparationRequest::parse(&job.requested.to_string())?;
                Ok((job, request))
            })
            .collect()
    }

    /// Which of `lesson_ids` can be prepared now.
    pub(crate) async fn preparable_lessons(
        &self,
        program_id: &str,
        lesson_ids: &[String],
    ) -> Result<HashSet<String>> {
        let mut read = self.db.begin_read().await?;
        let preparable = preparable_in(&mut read, program_id, lesson_ids).await?;
        read.commit().await?;
        Ok(preparable)
    }

    pub async fn job(&self, id: &str) -> Result<LearningGenerationJob> {
        let mut read = self.db.begin_read().await?;
        let job = Self::job_on(&mut read, id).await?;
        read.commit().await?;
        Ok(job)
    }

    pub(in super::super) async fn job_on(
        connection: &mut sqlx::SqliteConnection,
        id: &str,
    ) -> Result<LearningGenerationJob> {
        let not_found = || AppError::NotFound("Generation job not found".into());
        let scope = job_scope(&mut *connection, id)
            .await?
            .ok_or_else(not_found)?;
        let job = JobStore::find_summary_in(&mut *connection, id)
            .await?
            .ok_or_else(not_found)?;
        learning_job(job, scope)
    }

    pub(crate) async fn job_content_is_current(&self, id: &str) -> Result<bool> {
        let mut read = self.db.begin_read().await?;
        let current = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM learning_jobs j JOIN learning_programs p ON p.id=j.program_id WHERE j.job_id=? AND j.content_revision=p.content_revision)")
            .bind(id).fetch_one(&mut *read).await?;
        read.commit().await?;
        Ok(current)
    }

    pub(in super::super) fn jobs_store(&self) -> JobStore {
        JobStore::new(self.db.clone())
    }
}

pub(in super::super) async fn preparable_in(
    connection: &mut sqlx::SqliteConnection,
    program_id: &str,
    lesson_ids: &[String],
) -> Result<HashSet<String>> {
    let marks = vec!["?"; lesson_ids.len()].join(",");
    let sql = format!(
        "SELECT id FROM learning_lessons WHERE program_id=? AND id IN ({marks}) AND {PREPARABLE_LESSON}"
    );
    let mut query =
        sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql.as_str())).bind(program_id);
    for id in lesson_ids {
        query = query.bind(id);
    }
    Ok(query.fetch_all(connection).await?.into_iter().collect())
}

fn job_scope_row(row: &sqlx::sqlite::SqliteRow) -> JobScope {
    JobScope {
        program_id: row.get("program_id"),
        base_revision_number: row.get("base_revision_number"),
        content_revision: row.get("content_revision"),
    }
}

pub(in super::super) async fn job_scope(
    connection: &mut sqlx::SqliteConnection,
    job_id: &str,
) -> Result<Option<JobScope>> {
    Ok(sqlx::query(
        "SELECT program_id,base_revision_number,content_revision FROM learning_jobs WHERE job_id=?",
    )
    .bind(job_id)
    .fetch_optional(connection)
    .await?
    .as_ref()
    .map(job_scope_row))
}

pub(super) async fn job_scopes(
    connection: &mut sqlx::SqliteConnection,
    program_id: &str,
) -> Result<std::collections::HashMap<String, JobScope>> {
    Ok(sqlx::query("SELECT job_id,program_id,base_revision_number,content_revision FROM learning_jobs WHERE program_id=?")
        .bind(program_id)
        .fetch_all(connection)
        .await?
        .iter()
        .map(|row| (row.get("job_id"), job_scope_row(row)))
        .collect())
}

pub(in super::super) fn learning_job(
    job: JobRecord,
    scope: JobScope,
) -> Result<LearningGenerationJob> {
    use LearningGenerationJobStatus as S;
    let kind = match job.kind.as_str() {
        LESSON_PREPARATION => LearningGenerationJobKind::LessonPreparation,
        QUIZ_BANK => LearningGenerationJobKind::QuizBank,
        QUIZ_GRADING => LearningGenerationJobKind::QuizGrading,
        COURSE_PLAN => LearningGenerationJobKind::CoursePlan,
        _ => return Err(AppError::Database("Invalid generation job kind".into())),
    };
    let ended_by_update = job.error_code.as_deref() == Some(ENDED_BY_UPDATE);
    let lesson_ids = if kind == LearningGenerationJobKind::LessonPreparation {
        PreparationRequest::parse(&job.requested.to_string())?.lesson_ids
    } else {
        Vec::new()
    };
    let held_lesson_ids = job
        .result_ref
        .as_deref()
        .and_then(crate::content_verification::held_lessons)
        .unwrap_or_default();
    let job = LearningGenerationJob {
        id: job.id,
        program_id: scope.program_id,
        operation_id: job.operation_id,
        kind,
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
        error_code: job
            .cause_code
            .and_then(|code| serde_json::from_value(serde_json::Value::String(code)).ok()),
        ended_by_update,
        lesson_ids,
        held_lesson_ids,
        rank: job.rank,
        retry_of_job_id: job.retry_of_job_id,
        created_at: job.created_at,
        started_at: job.started_at,
        finished_at: job.finished_at,
    };
    curriculum::validate_generation_job(&job)?;
    Ok(job)
}
