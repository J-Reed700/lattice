//! Accepting programs and recording prepared and completed lessons.
use super::*;

impl LearningRepository {
    pub async fn accept(&self, req: &AcceptLearningProgramRequestDto) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let pending: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM learning_outline_drafts WHERE program_id=? AND json_extract(state_json,'$.review.status') IS NOT 'passed')").bind(&req.program_id).fetch_one(&mut *tx).await?;
        if pending {
            return Err(AppError::InvalidInput(
                "Resolve the saved outline findings before accepting this course.".into(),
            ));
        }
        let r=sqlx::query("UPDATE learning_programs SET title=?,status='active',revision=revision+1 WHERE id=? AND revision=? AND status='draft'").bind(req.title.trim()).bind(&req.program_id).bind(req.expected_revision).execute(&mut *tx).await.map_err(db)?;
        if r.rows_affected() != 1 {
            return Err(AppError::InvalidInput(
                "Program changed or is no longer a draft; reload and retry.".into(),
            ));
        }
        tx.commit().await.map_err(db)
    }

    pub async fn prepare(
        &self,
        program_id: &str,
        lesson_id: &str,
        expected_revision: i64,
        prepared: &PreparedLearningLesson,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let row =
            sqlx::query("SELECT preparation FROM learning_lessons WHERE id=? AND program_id=?")
                .bind(lesson_id)
                .bind(program_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                .ok_or_else(|| AppError::NotFound("Lesson not found in this program".into()))?;
        if row.get::<String, _>("preparation") == "ready" {
            tx.rollback().await.map_err(db)?;
            return Err(AppError::InvalidInput(
                "Ready lessons are immutable.".into(),
            ));
        }
        let changed=sqlx::query("UPDATE learning_programs SET revision=revision+1 WHERE id=? AND revision=? AND status='active'").bind(program_id).bind(expected_revision).execute(&mut *tx).await.map_err(db)?;
        if changed.rows_affected() != 1 {
            return Err(AppError::InvalidInput(
                "Program changed or is not active; reload and retry.".into(),
            ));
        }
        crate::features::learning::content_verification::persist_report(
            &mut tx, program_id, lesson_id, prepared,
        )
        .await?;
        for (i, b) in prepared.blocks.iter().enumerate() {
            sqlx::query("INSERT INTO learning_blocks(lesson_id,ordinal,kind,title,body,source_ids_json,rubric_json) VALUES(?,?,?,?,?,?,?)").bind(lesson_id).bind(i as i64).bind(block_kind(b.kind.clone())).bind(&b.title).bind(&b.body).bind(json(&b.source_ids)?).bind(json(&b.rubric)?).execute(&mut *tx).await.map_err(db)?;
        }
        sqlx::query("UPDATE learning_lessons SET preparation='ready' WHERE id=? AND program_id=?")
            .bind(lesson_id)
            .bind(program_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        let outcome_id = crate::features::learning::quiz_blueprints::ensure_lesson_outcome_in(
            &mut tx, program_id, lesson_id, None,
        )
        .await?;
        let seeds = crate::features::learning::quiz_bank_jobs::lesson_seed_items(
            lesson_id,
            &outcome_id,
            &prepared.questions,
            &prepared.keys,
        );
        crate::features::learning::assessment_repository::insert_bank_items_in(
            &mut tx, program_id, &seeds, None,
        )
        .await?;
        crate::features::learning::quiz_blueprints::ensure_blueprint_in(
            &mut tx,
            program_id,
            &LearningQuizScopeDto {
                kind: LearningQuizScopeKind::Lesson,
                id: Some(lesson_id.to_string()),
            },
        )
        .await?;
        tx.commit().await.map_err(db)
    }

    pub async fn complete(&self, req: &CompleteLearningLessonRequestDto) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let changed=sqlx::query("UPDATE learning_programs SET revision=revision+1 WHERE id=? AND revision=? AND status='active'").bind(&req.program_id).bind(req.expected_revision).execute(&mut *tx).await.map_err(db)?;
        if changed.rows_affected() != 1 {
            return Err(AppError::InvalidInput(
                "Program changed or is not active; reload and retry.".into(),
            ));
        }
        let changed=sqlx::query("UPDATE learning_lessons SET completed=1 WHERE id=? AND program_id=? AND preparation='ready' AND completed=0").bind(&req.lesson_id).bind(&req.program_id).execute(&mut *tx).await.map_err(db)?;
        if changed.rows_affected() != 1 {
            return Err(AppError::InvalidInput(
                "Only an incomplete, prepared lesson in this program can be completed.".into(),
            ));
        }
        let next:Option<String>=sqlx::query_scalar("SELECT id FROM learning_lessons WHERE program_id=? AND completed=0 ORDER BY (SELECT ordinal FROM learning_modules WHERE id=module_id),ordinal LIMIT 1").bind(&req.program_id).fetch_optional(&mut *tx).await.map_err(db)?;
        sqlx::query("UPDATE learning_programs SET current_lesson_id=? WHERE id=?")
            .bind(next)
            .bind(&req.program_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)
    }
}
