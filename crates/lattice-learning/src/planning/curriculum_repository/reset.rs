//! Resetting lesson preparation back to a clean outline state.
use super::generation_recovery::{ENDED_BY_UPDATE, LESSON_PREPARATION};
use super::jobs::{check_program_open, validate_program_and_lessons};
use super::{curriculum_operation, validate_operation_id, LearningCurriculumRepository};
use crate::curriculum::{self, LearningCurriculumLessonState, LearningCurriculumRevision};
use crate::persistence::{decode, encode, now};
use crate::plan_dto::{
    ResetLearningLessonPreparationRequestDto, ResetLearningLessonPreparationResultDto,
};
use crate::planning::preparation_request::PreparationRequest;
use crate::quiz_bank_jobs::QUIZ_BANK;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::hash::sha256_hex;
use lattice_core::shared::persistence::WriteTx;
use lattice_core::shared::runtime::jobs::{JobRecord, JobStatus, JobStore};
use sqlx::Row;
use std::collections::HashSet;

impl LearningCurriculumRepository {
    /// Cancels active preparation/quiz-bank jobs, clears saved drafts,
    /// checkpoints, claims, reviews, audits, teaching findings, verifications,
    /// blocks, and reverts selected lessons back to a clean `outline` state.
    pub async fn reset_lesson_preparation(
        &self,
        request: &ResetLearningLessonPreparationRequestDto,
    ) -> Result<ResetLearningLessonPreparationResultDto> {
        validate_operation_id(&request.operation_id)?;
        let deduped = validate_program_and_lessons(&request.program_id, &request.lesson_ids)?;
        let payload = crate::persistence::hash(request)?;
        let operation = curriculum_operation(
            &request.operation_id,
            &request.program_id,
            "reset_lesson_preparation",
            &payload,
            "Operation ID was reused with different reset data.",
        );

        let mut tx = self.db.begin_write().await?;
        if let Some(replayed) = operation
            .replay::<ResetLearningLessonPreparationResultDto>(&mut tx)
            .await?
        {
            tx.commit().await?;
            return Ok(replayed);
        }

        check_program_open(&mut tx, &request.program_id, request.expected_revision).await?;
        let lesson_json = serde_json::to_string(&deduped)?;
        let lesson_rows = sqlx::query(
            "SELECT l.id, l.preparation, l.curriculum_state, l.completed \
             FROM learning_lessons l \
             JOIN learning_modules m ON m.id = l.module_id \
             WHERE l.program_id = ? \
               AND l.id IN (SELECT value FROM json_each(?)) \
             ORDER BY m.ordinal, l.ordinal",
        )
        .bind(&request.program_id)
        .bind(&lesson_json)
        .fetch_all(&mut *tx)
        .await?;
        if lesson_rows.len() != deduped.len() {
            return Err(AppError::InvalidInput(
                "Selected lessons must belong to this course.".into(),
            ));
        }

        let ordered_lesson_ids: Vec<String> = lesson_rows.iter().map(|row| row.get("id")).collect();
        let reset_set: HashSet<String> = ordered_lesson_ids.iter().cloned().collect();
        let any_lesson_was_published = lesson_rows.iter().any(|row| {
            row.get::<String, _>("preparation") != "outline"
                || row.get::<String, _>("curriculum_state") != "outline"
                || row.get::<i64, _>("completed") != 0
        });

        Self::seed_on(&mut tx, &request.program_id).await?;
        let accepted_row = sqlx::query(
            "SELECT id, snapshot_json, resume_lesson_id \
             FROM learning_curriculum_revisions \
             WHERE program_id = ? AND status = 'accepted'",
        )
        .bind(&request.program_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound("Accepted curriculum not found".into()))?;
        let mut snapshot: LearningCurriculumRevision = decode(accepted_row.get("snapshot_json"))?;
        let any_snapshot_non_outline = snapshot
            .modules
            .iter()
            .flat_map(|module| module.lessons.iter())
            .any(|lesson| {
                reset_set.contains(&lesson.id)
                    && lesson.state != LearningCurriculumLessonState::Outline
            });
        let needs_revision_bump = any_lesson_was_published || any_snapshot_non_outline;

        let mut cancelled_job_ids = reset_preparation_jobs(
            &mut tx,
            &request.program_id,
            &ordered_lesson_ids,
            &reset_set,
        )
        .await?;
        let cancelled_quiz_ids =
            cancel_lesson_quiz_bank_jobs(&mut tx, &request.program_id, &reset_set).await?;
        cancelled_job_ids.extend(cancelled_quiz_ids);

        delete_lesson_preparation_artifacts(&mut tx, &request.program_id, &lesson_json).await?;

        sqlx::query(
            "UPDATE learning_lessons \
             SET preparation = 'outline', curriculum_state = 'outline', completed = 0 \
             WHERE program_id = ? AND id IN (SELECT value FROM json_each(?))",
        )
        .bind(&request.program_id)
        .bind(&lesson_json)
        .execute(&mut *tx)
        .await?;

        let program_revision = if needs_revision_bump {
            publish_reset_revision(
                &mut tx,
                &request.program_id,
                request.expected_revision,
                accepted_row.get("id"),
                accepted_row.get("resume_lesson_id"),
                &mut snapshot,
                &reset_set,
            )
            .await?
        } else {
            request.expected_revision
        };

        let result = ResetLearningLessonPreparationResultDto {
            reset_lesson_ids: ordered_lesson_ids,
            cancelled_job_ids,
            program_revision,
        };
        operation.record(&mut tx, &result).await?;
        tx.commit().await?;
        Ok(result)
    }
}

async fn reset_preparation_jobs(
    tx: &mut WriteTx,
    program_id: &str,
    ordered_lesson_ids: &[String],
    reset_set: &HashSet<String>,
) -> Result<Vec<String>> {
    let jobs = JobStore::list_for_subject_in(tx, LESSON_PREPARATION, program_id).await?;
    let mut cancelled_job_ids = Vec::new();
    for job in jobs {
        let Ok(parsed) = PreparationRequest::parse(&job.requested.to_string()) else {
            continue;
        };
        if !parsed.lesson_ids.iter().any(|id| reset_set.contains(id)) {
            continue;
        }
        let covers_only_reset = !parsed.lesson_ids.is_empty()
            && parsed.lesson_ids.iter().all(|id| reset_set.contains(id));
        if covers_only_reset {
            if reset_single_lesson_job(tx, &job).await? {
                cancelled_job_ids.push(job.id);
            }
        } else {
            strip_lessons_from_multi_lesson_job(tx, &job, ordered_lesson_ids, reset_set).await?;
        }
    }
    Ok(cancelled_job_ids)
}

async fn reset_single_lesson_job(tx: &mut WriteTx, job: &JobRecord) -> Result<bool> {
    let was_active = matches!(job.status, JobStatus::Pending | JobStatus::Running);
    if matches!(
        job.status,
        JobStatus::Pending | JobStatus::Running | JobStatus::Interrupted
    ) {
        JobStore::cancel_in(tx, &job.id).await?;
    } else if job.status != JobStatus::Cancelled
        || job.error_code.as_deref() != Some(ENDED_BY_UPDATE)
    {
        let stamp = now();
        sqlx::query(
            "INSERT INTO job_events(job_id,ordinal,status,progress_current,message,created_at) \
             SELECT j.id,(SELECT coalesce(max(ordinal),-1)+1 FROM job_events WHERE job_id=j.id),\
             'cancelled',j.progress_current,'Preparation reset',? FROM jobs j WHERE j.id=?",
        )
        .bind(stamp)
        .bind(&job.id)
        .execute(&mut **tx)
        .await?;
    }
    let stamp = now();
    sqlx::query(
        "UPDATE jobs \
         SET status = 'cancelled', \
             finished_at = coalesce(finished_at, ?), \
             error_code = ?, \
             error_message = NULL, \
             cause_code = NULL, \
             result_ref = NULL, \
             staged_result_json = NULL, \
             activity_json = NULL, \
             saved_work = 0, \
             recovery_mark = 0, \
             unsaved_starts = 0, \
             retry_not_before = NULL, \
             progress_message = 'Preparation reset' \
         WHERE id = ?",
    )
    .bind(stamp)
    .bind(ENDED_BY_UPDATE)
    .bind(&job.id)
    .execute(&mut **tx)
    .await?;
    sqlx::query("DELETE FROM job_checkpoints WHERE job_id = ?")
        .bind(&job.id)
        .execute(&mut **tx)
        .await?;
    Ok(was_active)
}

async fn strip_lessons_from_multi_lesson_job(
    tx: &mut WriteTx,
    job: &JobRecord,
    ordered_lesson_ids: &[String],
    reset_set: &HashSet<String>,
) -> Result<()> {
    for lesson_id in ordered_lesson_ids {
        let prefix = format!("{lesson_id}/");
        sqlx::query(
            "DELETE FROM job_checkpoints WHERE job_id = ? AND substr(key, 1, length(?)) = ?",
        )
        .bind(&job.id)
        .bind(&prefix)
        .bind(&prefix)
        .execute(&mut **tx)
        .await?;
    }
    if let Some(staged) = job.staged_result.clone() {
        let mut drafts: crate::lesson_drafts::Drafts =
            serde_json::from_value(staged).map_err(|e| AppError::Serialization(e.to_string()))?;
        for lesson_id in reset_set {
            drafts.drafts.remove(lesson_id);
        }
        let next_staged = if drafts.drafts.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&drafts)?)
        };
        sqlx::query("UPDATE jobs SET staged_result_json = ? WHERE id = ?")
            .bind(next_staged)
            .bind(&job.id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

async fn cancel_lesson_quiz_bank_jobs(
    tx: &mut WriteTx,
    program_id: &str,
    reset_set: &HashSet<String>,
) -> Result<Vec<String>> {
    let jobs = JobStore::list_for_subject_in(tx, QUIZ_BANK, program_id).await?;
    let mut cancelled = Vec::new();
    for job in jobs {
        if !matches!(job.status, JobStatus::Pending | JobStatus::Running) {
            continue;
        }
        let Some(scope) = job.requested.get("scope") else {
            continue;
        };
        let is_lesson_scope =
            scope.get("kind").and_then(serde_json::Value::as_str) == Some("lesson");
        let lesson_id = scope.get("id").and_then(serde_json::Value::as_str);
        if is_lesson_scope && lesson_id.is_some_and(|id| reset_set.contains(id)) {
            JobStore::cancel_in(tx, &job.id).await?;
            sqlx::query(
                "UPDATE jobs \
                 SET staged_result_json = NULL, saved_work = 0, recovery_mark = 0, unsaved_starts = 0 \
                 WHERE id = ?",
            )
            .bind(&job.id)
            .execute(&mut **tx)
            .await?;
            sqlx::query("DELETE FROM job_checkpoints WHERE job_id = ?")
                .bind(&job.id)
                .execute(&mut **tx)
                .await?;
            cancelled.push(job.id);
        }
    }
    Ok(cancelled)
}

async fn delete_lesson_preparation_artifacts(
    tx: &mut WriteTx,
    program_id: &str,
    lesson_json: &str,
) -> Result<()> {
    sqlx::query(
        "DELETE FROM learning_lesson_claim_events \
         WHERE claim_id IN ( \
             SELECT claim_id FROM learning_lesson_claims \
             WHERE lesson_id IN (SELECT value FROM json_each(?)) \
         )",
    )
    .bind(lesson_json)
    .execute(&mut **tx)
    .await?;
    for sql in [
        "DELETE FROM learning_lesson_claims WHERE lesson_id IN (SELECT value FROM json_each(?))",
        "DELETE FROM learning_lesson_audits WHERE lesson_id IN (SELECT value FROM json_each(?))",
        "DELETE FROM learning_lesson_reviews WHERE lesson_id IN (SELECT value FROM json_each(?))",
        "DELETE FROM learning_lesson_teaching_findings WHERE lesson_id IN (SELECT value FROM json_each(?))",
        "DELETE FROM learning_lesson_verifications WHERE lesson_id IN (SELECT value FROM json_each(?))",
        "DELETE FROM learning_blocks WHERE lesson_id IN (SELECT value FROM json_each(?))",
    ] {
        sqlx::query(sql)
            .bind(lesson_json)
            .execute(&mut **tx)
            .await?;
    }
    sqlx::query(
        "DELETE FROM learning_checker_examples \
         WHERE program_id = ? AND lesson_id IN (SELECT value FROM json_each(?))",
    )
    .bind(program_id)
    .bind(lesson_json)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "DELETE FROM learning_assessment_candidates \
         WHERE program_id = ? \
           AND lesson_id IN (SELECT value FROM json_each(?)) \
           AND NOT EXISTS ( \
               SELECT 1 FROM learning_assessment_form_items fi \
               WHERE fi.candidate_id = learning_assessment_candidates.id \
           )",
    )
    .bind(program_id)
    .bind(lesson_json)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn publish_reset_revision(
    tx: &mut WriteTx,
    program_id: &str,
    expected_revision: i64,
    predecessor: String,
    resume_lesson_id: Option<String>,
    snapshot: &mut LearningCurriculumRevision,
    reset_set: &HashSet<String>,
) -> Result<i64> {
    let changed = sqlx::query(
        "UPDATE learning_programs SET revision = revision + 1 \
         WHERE id = ? AND revision = ? AND status = 'active'",
    )
    .bind(program_id)
    .bind(expected_revision)
    .execute(&mut **tx)
    .await?;
    if changed.rows_affected() != 1 {
        return Err(AppError::Conflict(
            "Program changed before lesson preparation could reset.".into(),
        ));
    }
    for lesson in snapshot
        .modules
        .iter_mut()
        .flat_map(|module| module.lessons.iter_mut())
    {
        if reset_set.contains(&lesson.id) {
            lesson.state = LearningCurriculumLessonState::Outline;
            lesson.replacement_lesson_id = None;
        }
    }
    snapshot.id = uuid::Uuid::new_v4().to_string();
    snapshot.revision_number = snapshot.revision_number.saturating_add(1);
    snapshot.parent_revision_id = Some(predecessor.clone());
    snapshot.reason = "Reset lesson preparation".into();
    snapshot.created_at = now();
    curriculum::validate_curriculum(snapshot)?;
    let snapshot_json = encode(snapshot)?;
    let snapshot_hash = sha256_hex(&snapshot_json);
    let required = snapshot
        .modules
        .iter()
        .flat_map(|module| &module.lessons)
        .filter(|lesson| {
            !matches!(
                lesson.state,
                LearningCurriculumLessonState::Skipped
                    | LearningCurriculumLessonState::Replaced
                    | LearningCurriculumLessonState::Challenged
            )
        })
        .count() as i64;
    sqlx::query(
        "UPDATE learning_curriculum_revisions SET status = 'superseded' \
         WHERE id = ? AND status = 'accepted'",
    )
    .bind(&predecessor)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO learning_curriculum_revisions(\
             id, program_id, revision, predecessor_id, status, reason, \
             snapshot_json, snapshot_sha256, required_lesson_count, \
             resume_lesson_id, created_at, accepted_at\
         ) VALUES(?, ?, ?, ?, 'accepted', ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&snapshot.id)
    .bind(program_id)
    .bind(snapshot.revision_number as i64)
    .bind(&predecessor)
    .bind(&snapshot.reason)
    .bind(snapshot_json)
    .bind(snapshot_hash)
    .bind(required)
    .bind(resume_lesson_id)
    .bind(snapshot.created_at)
    .bind(snapshot.created_at)
    .execute(&mut **tx)
    .await?;
    Ok(expected_revision + 1)
}
