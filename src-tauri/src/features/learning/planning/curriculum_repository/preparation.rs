//! Lesson drafts and checkpoints saved while preparing, and publishing the prepared lessons.
use super::jobs::job_scope;
use super::LearningCurriculumRepository;
use crate::features::learning::curriculum::{
    self, LearningCurriculumLessonState, LearningCurriculumRevision,
};
use crate::features::learning::persistence::{db, decode, encode, hash_text, now};
use crate::shared::error::{AppError, Result};
use crate::shared::runtime::jobs::{JobStatus, JobStore};
use sqlx::Row;

impl LearningCurriculumRepository {
    pub(in crate::features::learning) async fn lesson_draft(
        &self,
        job_id: &str,
        lesson_id: &str,
    ) -> Result<Option<crate::features::learning::lesson_drafts::Draft>> {
        let mut drafts: crate::features::learning::lesson_drafts::Drafts = self
            .jobs_store()
            .staged(job_id)
            .await?
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| AppError::Serialization(e.to_string()))?
            .unwrap_or_default();
        Ok(drafts.drafts.remove(lesson_id))
    }

    pub(in crate::features::learning) async fn save_lesson_draft(
        &self,
        job_id: &str,
        lesson_id: &str,
        draft: crate::features::learning::lesson_drafts::Draft,
    ) -> Result<()> {
        let saved = self
            .jobs_store()
            .update_staged(job_id, |staged| {
                let mut drafts: crate::features::learning::lesson_drafts::Drafts = staged
                    .map(serde_json::from_value)
                    .transpose()
                    .map_err(|e| AppError::Serialization(e.to_string()))?
                    .unwrap_or_default();
                drafts.drafts.insert(lesson_id.into(), draft);
                serde_json::to_value(&drafts).map_err(|e| AppError::Serialization(e.to_string()))
            })
            .await?;
        if !saved {
            return Err(AppError::InvalidState(
                "Lesson preparation is no longer running.".into(),
            ));
        }
        Ok(())
    }

    pub(in crate::features::learning) async fn lesson_checkpoint(
        &self,
        job_id: &str,
        lesson_id: &str,
        key: &str,
    ) -> Result<Option<serde_json::Value>> {
        self.jobs_store()
            .checkpoint(job_id, &lesson_checkpoint_key(lesson_id, key))
            .await
    }

    pub(in crate::features::learning) async fn lesson_checkpoints_with_prefix(
        &self,
        job_id: &str,
        lesson_id: &str,
        prefix: &str,
    ) -> Result<Vec<serde_json::Value>> {
        self.jobs_store()
            .checkpoints_with_prefix(job_id, &lesson_checkpoint_key(lesson_id, prefix))
            .await
    }

    pub(in crate::features::learning) async fn save_lesson_checkpoint(
        &self,
        job_id: &str,
        lesson_id: &str,
        key: &str,
        value: serde_json::Value,
    ) -> Result<()> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;
        let in_course: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM learning_jobs j JOIN learning_lessons l ON l.program_id=j.program_id WHERE j.job_id=? AND l.id=?)")
            .bind(job_id).bind(lesson_id).fetch_one(&mut *tx).await.map_err(db)?;
        if !in_course
            || !JobStore::put_checkpoint_in(
                &mut tx,
                job_id,
                &lesson_checkpoint_key(lesson_id, key),
                &value,
            )
            .await?
        {
            return Err(AppError::InvalidState(
                "Lesson preparation is no longer running or the lesson changed.".into(),
            ));
        }
        tx.commit().await.map_err(db)
    }

    pub async fn publish_prepared_lessons(
        &self,
        id: &str,
        expected_revision: i64,
        lessons: &[(
            String,
            crate::features::learning::dto::PreparedLearningLesson,
        )],
    ) -> Result<()> {
        if lessons.is_empty() || lessons.len() > 3 {
            return Err(AppError::InvalidInput(
                "A generation job may publish one to three prepared lessons.".into(),
            ));
        }
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;
        let not_found = || AppError::NotFound("Generation job not found".into());
        let scope = job_scope(&mut tx, id).await?.ok_or_else(not_found)?;
        let job = JobStore::find_in(&mut tx, id)
            .await?
            .ok_or_else(not_found)?;
        if job.status != JobStatus::Running {
            return Err(AppError::InvalidState(
                "Cancelled or finished jobs cannot publish staged lessons.".into(),
            ));
        }
        let program_id = scope.program_id.clone();
        if lessons.len() != job.progress_total as usize {
            return Err(AppError::InvalidInput(
                "Prepared lesson count must match the bounded job size.".into(),
            ));
        }
        let program =
            sqlx::query("SELECT revision,content_revision FROM learning_programs WHERE id=?")
                .bind(&program_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?;
        let current_revision: i64 = program.get("revision");
        // Completion and grading may advance the UI's concurrency token while
        // this job runs. Only unchanged teaching inputs can publish across that
        // progress; source bindings are still validated below in this transaction.
        if scope.content_revision != Some(program.get::<i64, _>("content_revision"))
            || expected_revision < scope.base_revision_number
            || expected_revision > current_revision
        {
            return Err(AppError::InvalidState(
                "Course content changed while this lesson was being prepared. Start preparation again for the updated course.".into(),
            ));
        }
        let changed=sqlx::query("UPDATE learning_programs SET revision=revision+1 WHERE id=? AND revision=? AND status='active'").bind(&program_id).bind(current_revision).execute(&mut *tx).await.map_err(db)?;
        if changed.rows_affected() != 1 {
            return Err(AppError::InvalidState(
                "Program changed before generation could publish.".into(),
            ));
        }
        let mut unique = std::collections::HashSet::new();
        for (lesson_id, prepared) in lessons {
            if !unique.insert(lesson_id) {
                return Err(AppError::InvalidInput(
                    "A preparation job cannot repeat lesson IDs.".into(),
                ));
            }
            let row =
                sqlx::query("SELECT preparation FROM learning_lessons WHERE id=? AND program_id=?")
                    .bind(lesson_id)
                    .bind(&program_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(db)?
                    .ok_or_else(|| AppError::NotFound("Lesson not found in this program".into()))?;
            if row.get::<String, _>("preparation") != "outline" {
                return Err(AppError::InvalidState(
                    "Only outline lessons can be published by this job.".into(),
                ));
            }
            crate::features::learning::content_verification::persist_report(
                &mut tx,
                &program_id,
                lesson_id,
                prepared,
            )
            .await?;
            for (ordinal, b) in prepared.blocks.iter().enumerate() {
                sqlx::query("INSERT INTO learning_blocks(lesson_id,ordinal,kind,title,body,source_ids_json,rubric_json) VALUES(?,?,?,?,?,?,?)").bind(lesson_id).bind(ordinal as i64).bind(crate::features::learning::repository::block_kind(b.kind.clone())).bind(&b.title).bind(&b.body).bind(encode(&b.source_ids)?).bind(encode(&b.rubric)?).execute(&mut *tx).await.map_err(db)?;
            }
            for (ordinal, q) in prepared.questions.iter().enumerate() {
                sqlx::query("INSERT INTO learning_questions(id,lesson_id,module_id,kind,prompt,options_json,source_ids_json,ordinal) SELECT ?,?,module_id,?,?,?,?,? FROM learning_lessons WHERE id=?").bind(&q.id).bind(lesson_id).bind(crate::features::learning::repository::assessment(q.kind.clone())).bind(&q.prompt).bind(encode(&q.options)?).bind(encode(&q.source_ids)?).bind(ordinal as i64).bind(lesson_id).execute(&mut *tx).await.map_err(db)?;
            }
            for key in &prepared.keys {
                sqlx::query("INSERT INTO learning_answer_keys(question_id,correct_index,explanation) VALUES(?,?,?)").bind(&key.question_id).bind(key.correct_index as i64).bind(&key.explanation).execute(&mut *tx).await.map_err(db)?;
            }
            sqlx::query("UPDATE learning_lessons SET preparation='ready',curriculum_state='ready' WHERE id=? AND program_id=?").bind(lesson_id).bind(&program_id).execute(&mut *tx).await.map_err(db)?;
        }
        // Publish a new immutable accepted snapshot together with the prepared
        // lesson records and the program revision. The old accepted snapshot
        // remains historical; readers never observe a half-prepared plan.
        let accepted_row = sqlx::query("SELECT id,snapshot_json,required_lesson_count,resume_lesson_id FROM learning_curriculum_revisions WHERE program_id=? AND status='accepted'")
            .bind(&program_id).fetch_optional(&mut *tx).await.map_err(db)?
            .ok_or_else(|| AppError::NotFound("Accepted curriculum not found".into()))?;
        let predecessor: String = accepted_row.get("id");
        let mut snapshot: LearningCurriculumRevision = decode(accepted_row.get("snapshot_json"))?;
        let mut found = std::collections::HashSet::new();
        for (lesson_id, _) in lessons {
            for lesson in snapshot
                .modules
                .iter_mut()
                .flat_map(|module| module.lessons.iter_mut())
            {
                if lesson.id == *lesson_id {
                    lesson.state = LearningCurriculumLessonState::Ready;
                    found.insert(lesson_id.as_str());
                }
            }
        }
        if found.len() != lessons.len() {
            return Err(AppError::InvalidState(
                "Prepared lessons must belong to the accepted curriculum snapshot.".into(),
            ));
        }
        snapshot.id = uuid::Uuid::new_v4().to_string();
        snapshot.revision_number = snapshot.revision_number.saturating_add(1);
        snapshot.parent_revision_id = Some(predecessor.clone());
        snapshot.reason = "Published prepared lesson material".into();
        snapshot.created_at = now();
        curriculum::validate_curriculum(&snapshot)?;
        let snapshot_json = encode(&snapshot)?;
        let snapshot_hash = hash_text(&snapshot_json);
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
        let resume: Option<String> = accepted_row.get("resume_lesson_id");
        sqlx::query("UPDATE learning_curriculum_revisions SET status='superseded' WHERE id=? AND status='accepted'")
            .bind(&predecessor).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("INSERT INTO learning_curriculum_revisions(id,program_id,revision,predecessor_id,status,reason,snapshot_json,snapshot_sha256,required_lesson_count,resume_lesson_id,created_at,accepted_at) VALUES(?,?,?,?,'accepted',?,?,?,?,?,?,?)")
            .bind(&snapshot.id).bind(&program_id).bind(snapshot.revision_number as i64).bind(&predecessor)
            .bind(&snapshot.reason).bind(snapshot_json).bind(snapshot_hash).bind(required).bind(resume)
            .bind(snapshot.created_at).bind(snapshot.created_at).execute(&mut *tx).await.map_err(db)?;
        JobStore::complete_in(&mut tx, id, &program_id, "Published prepared lessons").await?;
        tx.commit().await.map_err(db)
    }
}

/// Lesson checkpoints share their job's key space, one prefix per lesson.
pub(super) fn lesson_checkpoint_key(lesson_id: &str, key: &str) -> String {
    format!("{lesson_id}/{key}")
}
