//! Accepting programs, recording prepared and completed lessons, and grading attempts.
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
        for (i, q) in prepared.questions.iter().enumerate() {
            sqlx::query("INSERT INTO learning_questions(id,lesson_id,module_id,kind,prompt,options_json,source_ids_json,ordinal) SELECT ?,?,module_id,?,?,?,?,? FROM learning_lessons WHERE id=?")
                .bind(&q.id).bind(lesson_id).bind(assessment(q.kind.clone())).bind(&q.prompt).bind(json(&q.options)?).bind(json(&q.source_ids)?).bind(i as i64).bind(lesson_id).execute(&mut *tx).await.map_err(db)?;
        }
        for k in &prepared.keys {
            sqlx::query("INSERT INTO learning_answer_keys(question_id,correct_index,explanation) VALUES(?,?,?)").bind(&k.question_id).bind(k.correct_index as i64).bind(&k.explanation).execute(&mut *tx).await.map_err(db)?;
        }
        sqlx::query("UPDATE learning_lessons SET preparation='ready' WHERE id=? AND program_id=?")
            .bind(lesson_id)
            .bind(program_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
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

    pub async fn submit(
        &self,
        req: &SubmitLearningAttemptRequestDto,
    ) -> Result<LearningAttemptDto> {
        let mut normalized_answers = req.answers.clone();
        normalized_answers.sort_by(|a, b| a.question_id.cmp(&b.question_id));
        let payload = json(&(
            req.program_id.as_str(),
            req.module_id.as_str(),
            req.lesson_id.as_deref(),
            assessment(req.kind.clone()),
            normalized_answers,
        ))?;
        let hash = hash_text(&payload);
        let mut tx = self.pool.begin().await.map_err(db)?;
        if let Some(row) = sqlx::query("SELECT * FROM learning_attempts WHERE id=?")
            .bind(&req.attempt_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
        {
            if row.get::<String, _>("payload_hash") != hash {
                return Err(AppError::InvalidInput(
                    "This attempt ID was already used with different answers.".into(),
                ));
            }
            tx.rollback().await.map_err(db)?;
            return attempt_from_row(&row);
        }
        let p = sqlx::query("SELECT revision,status FROM learning_programs WHERE id=?")
            .bind(&req.program_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Learning program not found".into()))?;
        if p.get::<i64, _>("revision") != req.expected_revision
            || p.get::<String, _>("status") != "active"
        {
            return Err(AppError::InvalidInput(
                "Program changed or is not active; reload and retry.".into(),
            ));
        }
        // Module tests also carry the module's lesson count, for their coverage.
        let (question_rows, scope_lesson, module_lessons): (
            Vec<sqlx::sqlite::SqliteRow>,
            Option<String>,
            Option<i64>,
        ) = match req.kind {
            LearningAssessmentKind::Practice | LearningAssessmentKind::Quiz => {
                let lesson_id = req.lesson_id.as_ref().ok_or_else(|| {
                    AppError::InvalidInput("Practice and quiz attempts require a lesson ID.".into())
                })?;
                let exists:i64=sqlx::query_scalar("SELECT count(*) FROM learning_lessons WHERE id=? AND program_id=? AND module_id=? AND preparation='ready'").bind(lesson_id).bind(&req.program_id).bind(&req.module_id).fetch_one(&mut *tx).await.map_err(db)?;
                if exists != 1 {
                    return Err(AppError::InvalidInput(
                        "Lesson does not belong to this active program module or is not ready."
                            .into(),
                    ));
                }
                (sqlx::query("SELECT q.*,k.correct_index,k.explanation FROM learning_questions q JOIN learning_answer_keys k ON k.question_id=q.id WHERE q.lesson_id=? AND q.kind=? ORDER BY q.ordinal").bind(lesson_id).bind(assessment(req.kind.clone())).fetch_all(&mut *tx).await.map_err(db)?,Some(lesson_id.clone()),None)
            }
            LearningAssessmentKind::Test => {
                if req.lesson_id.is_some() {
                    return Err(AppError::InvalidInput(
                        "Module tests do not accept a lesson ID.".into(),
                    ));
                }
                let module_exists: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM learning_modules WHERE id=? AND program_id=?",
                )
                .bind(&req.module_id)
                .bind(&req.program_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?;
                if module_exists != 1 {
                    return Err(AppError::InvalidInput(
                        "Module does not belong to this program.".into(),
                    ));
                }
                // A module test covers whichever of its lessons are prepared
                // so far; lessons prepared later add their questions to it.
                let (module_lessons, ready_lessons): (i64, i64) = sqlx::query_as(
                    "SELECT count(*),coalesce(sum(preparation='ready'),0) FROM learning_lessons WHERE module_id=?",
                )
                .bind(&req.module_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?;
                if ready_lessons == 0 {
                    return Err(AppError::InvalidInput(
                        "Prepare at least one lesson in this module before taking its test".into(),
                    ));
                }
                (sqlx::query("SELECT q.*,k.correct_index,k.explanation FROM learning_questions q JOIN learning_answer_keys k ON k.question_id=q.id JOIN learning_lessons l ON l.id=q.lesson_id WHERE q.module_id=? AND q.kind='test' AND l.preparation='ready' ORDER BY l.ordinal,q.ordinal").bind(&req.module_id).fetch_all(&mut *tx).await.map_err(db)?,None,Some(module_lessons))
            }
        };
        if question_rows.is_empty() {
            return Err(AppError::InvalidInput(
                "No ready questions exist for this assessment.".into(),
            ));
        }
        if req.answers.len() != question_rows.len() {
            return Err(AppError::InvalidInput(
                "Submit one answer for every question in the assessment.".into(),
            ));
        }
        let expected: std::collections::HashSet<String> =
            question_rows.iter().map(|q| q.get("id")).collect();
        let coverage = module_lessons.map(|module_lessons| {
            let covered: std::collections::HashSet<String> =
                question_rows.iter().map(|q| q.get("lesson_id")).collect();
            LearningModuleTestCoverageDto {
                covered_lessons: covered.len() as i64,
                module_lessons,
            }
        });
        let mut seen = std::collections::HashSet::new();
        for a in &req.answers {
            if !seen.insert(a.question_id.clone()) {
                return Err(AppError::InvalidInput(
                    "Duplicate question answers are not allowed.".into(),
                ));
            }
            if !expected.contains(&a.question_id) {
                return Err(AppError::InvalidInput(
                    "Answer includes a question outside this assessment.".into(),
                ));
            }
        }
        let mut results = Vec::new();
        let mut correct = 0i64;
        for q in question_rows {
            let id: String = q.get("id");
            let answer = req
                .answers
                .iter()
                .find(|a| a.question_id == id)
                .ok_or_else(|| AppError::InvalidInput("Missing question answer.".into()))?;
            let options: Vec<String> = decode(q.get("options_json"))?;
            let correct_index: i64 = q.get("correct_index");
            if answer.selected_index >= options.len()
                || usize::try_from(correct_index)
                    .ok()
                    .filter(|i| *i < options.len())
                    .is_none()
            {
                return Err(AppError::InvalidInput(
                    "Selected answer is outside the available options.".into(),
                ));
            }
            if answer.selected_index as i64 == correct_index {
                correct += 1;
            }
            results.push(LearningQuestionResultDto {
                question_id: id,
                prompt: q.get("prompt"),
                options,
                selected_index: answer.selected_index,
                correct_index: correct_index as usize,
                explanation: q.get("explanation"),
                source_ids: decode(q.get("source_ids_json"))?,
            });
        }
        let changed=sqlx::query("UPDATE learning_programs SET revision=revision+1 WHERE id=? AND revision=? AND status='active'").bind(&req.program_id).bind(req.expected_revision).execute(&mut *tx).await.map_err(db)?;
        if changed.rows_affected() != 1 {
            return Err(AppError::InvalidInput(
                "Program changed or is not active; reload and retry.".into(),
            ));
        }
        let submitted_at = chrono::Utc::now().timestamp_millis();
        let total = results.len() as i64;
        sqlx::query("INSERT INTO learning_attempts(id,program_id,module_id,lesson_id,kind,correct,total,answers_json,results_json,submitted_at,payload_hash,covered_lessons,module_lessons) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&req.attempt_id).bind(&req.program_id).bind(&req.module_id).bind(&scope_lesson).bind(assessment(req.kind.clone())).bind(correct).bind(total).bind(json(&req.answers)?).bind(json(&results)?).bind(submitted_at).bind(hash).bind(coverage.map(|c| c.covered_lessons)).bind(coverage.map(|c| c.module_lessons)).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(LearningAttemptDto {
            id: req.attempt_id.clone(),
            module_id: req.module_id.clone(),
            lesson_id: scope_lesson,
            kind: req.kind.clone(),
            correct,
            total,
            results,
            submitted_at,
            coverage,
        })
    }

    /// Each question's answer explanation in `lesson_id`, by question id:
    /// the part of a published lesson only its answer key holds.
    pub async fn answer_explanations(
        &self,
        lesson_id: &str,
    ) -> Result<std::collections::HashMap<String, String>> {
        let rows = sqlx::query("SELECT q.id,k.explanation FROM learning_questions q JOIN learning_answer_keys k ON k.question_id=q.id WHERE q.lesson_id=?")
            .bind(lesson_id)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        Ok(rows
            .into_iter()
            .map(|row| (row.get("id"), row.get("explanation")))
            .collect())
    }
}
