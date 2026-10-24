//! Starting practice sessions, saving artifact revisions and changing modes.
use super::*;

impl LearningPracticeRepository {
    pub async fn start(
        &self,
        req: &StartLearningPracticeSessionRequestDto,
        payload_hash: &str,
    ) -> Result<LearningPracticeWorkspaceDto> {
        Self::validate_ids(&req.program_id, &req.session_id, Some(&req.operation_id))?;
        uuid(&req.lesson_id, "lesson")?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &req.program_id).await?;
        if practice_operation(
            &req.operation_id,
            &req.program_id,
            &req.session_id,
            "start",
            payload_hash,
        )
        .seen(&mut *tx)
        .await?
        {
            tx.commit().await.map_err(db)?;
            return self.workspace(&req.program_id).await;
        }
        Self::verify_active(&mut tx, &req.program_id).await?;
        let program_revision: Option<i64> =
            sqlx::query_scalar("SELECT revision FROM learning_programs WHERE id=?")
                .bind(&req.program_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        if program_revision != Some(req.expected_program_revision) {
            return Err(invalid("Program changed; reload and retry."));
        }
        let lesson = sqlx::query(
            "SELECT title,objective,preparation FROM learning_lessons WHERE program_id=? AND id=?",
        )
        .bind(&req.program_id)
        .bind(&req.lesson_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Lesson not found in this program".into()))?;
        if lesson.get::<String, _>("preparation") != "ready" {
            return Err(invalid(
                "Prepare this lesson before starting a practice attempt.",
            ));
        }
        let mut source_ids = Vec::<String>::new();
        for row in sqlx::query("SELECT source_ids_json FROM learning_blocks WHERE lesson_id=? UNION ALL SELECT source_version_ids_json AS source_ids_json FROM learning_assessment_candidates WHERE lesson_id=? AND origin IN ('lesson','migrated')").bind(&req.lesson_id).bind(&req.lesson_id).fetch_all(&mut *tx).await.map_err(db)? {
            let ids:Vec<String>=decode(row.get("source_ids_json"))?; source_ids.extend(ids);
        }
        source_ids.sort();
        source_ids.dedup();
        if source_ids.len() > 32 {
            return Err(invalid(
                "A practice attempt can freeze no more than 32 source versions.",
            ));
        }
        for version_id in &source_ids {
            let owned: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM learning_source_versions WHERE program_id=? AND id=?",
            )
            .bind(&req.program_id)
            .bind(version_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
            if owned.is_none() {
                return Err(invalid(
                    "Lesson source references must resolve to immutable versions owned by this program.",
                ));
            }
        }
        let title: String = lesson.get("title");
        let objective: String = lesson.get("objective");
        let fallback_prompt=format!("{}\n\nCreate a short written artifact that demonstrates your reasoning. State what you would do and why; use a new example where useful.",objective.trim());
        let block_kind = match req.task_kind {
            LearningPracticeTaskKind::Guided => "guided_practice",
            LearningPracticeTaskKind::Independent => "independent_practice",
        };
        let block = sqlx::query("SELECT body,rubric_json FROM learning_blocks WHERE lesson_id=? AND kind=? ORDER BY ordinal LIMIT 1")
            .bind(&req.lesson_id).bind(block_kind).fetch_optional(&mut *tx).await.map_err(db)?;
        if req.task_kind == LearningPracticeTaskKind::Guided && block.is_none() {
            return Err(invalid(
                "This lesson has no guided exercise. Use its independent assignment.",
            ));
        }
        let mut task_prompt = block
            .as_ref()
            .map(|b| b.get::<String, _>("body"))
            .unwrap_or(fallback_prompt);
        let mut rubric: Vec<LearningPracticeCriterionDto> = block
            .as_ref()
            .map(|b| decode(b.get("rubric_json")))
            .transpose()?
            .unwrap_or_default();
        if rubric.is_empty() {
            rubric = default_rubric();
        }
        let mut initial_text = String::new();
        let mut inherited_assistance = None;
        if let Some(parent_id) = &req.revises_session_id {
            uuid(parent_id, "previous attempt")?;
            if req.mode == LearningPracticeMode::Demonstrate {
                return Err(invalid(
                    "Revisions use previous feedback and must remain assisted practice.",
                ));
            }
            let parent = sqlx::query("SELECT s.task_prompt,s.task_kind,s.rubric_json,s.source_version_ids_json,sub.artifact_text,sub.criteria_json,sub.assistance_json FROM learning_practice_sessions s JOIN learning_practice_submissions sub ON sub.session_id=s.id WHERE s.id=? AND s.program_id=? AND s.lesson_id=?")
                .bind(parent_id).bind(&req.program_id).bind(&req.lesson_id).fetch_optional(&mut *tx).await.map_err(db)?
                .ok_or_else(||invalid("Only a submitted attempt from this lesson can be revised."))?;
            if parse_task_kind(parent.get("task_kind"))? != req.task_kind {
                return Err(invalid("A revision must keep the original exercise."));
            }
            task_prompt = parent.get("task_prompt");
            rubric = decode(parent.get("rubric_json"))?;
            source_ids = decode(parent.get("source_version_ids_json"))?;
            initial_text = parent.get("artifact_text");
            let previous_assistance: Vec<LearningPracticeAssistanceEventDto> =
                decode(parent.get("assistance_json"))?;
            let assistance_summary: Vec<_> = previous_assistance
                .iter()
                .map(|event| serde_json::json!({"kind":event.kind,"mode":event.mode}))
                .collect();
            inherited_assistance = Some(
                serde_json::json!({"revisesSessionId":parent_id,"previousFeedback":decode::<serde_json::Value>(parent.get("criteria_json"))?,"previousAssistance":assistance_summary}),
            );
        }
        let timestamp = now();
        sqlx::query("INSERT INTO learning_practice_sessions(id,program_id,lesson_id,lesson_title,lesson_objective,task_prompt,status,mode,revision,source_version_ids_json,rubric_json,created_at,updated_at,task_kind,revises_session_id) VALUES(?,?,?,?,?,?,'active',?,0,?,?,?,?,?,?)")
            .bind(&req.session_id).bind(&req.program_id).bind(&req.lesson_id).bind(&title).bind(&objective).bind(&task_prompt).bind(mode(&req.mode)).bind(json(&source_ids)?).bind(json(&rubric)?).bind(timestamp).bind(timestamp).bind(task_kind(&req.task_kind)).bind(&req.revises_session_id).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("INSERT INTO learning_practice_artifact_revisions(program_id,session_id,revision,text,sha256,operation_id,created_at) VALUES(?,?,0,?,?,?,?)").bind(&req.program_id).bind(&req.session_id).bind(&initial_text).bind(hash_text(&initial_text)).bind(&req.operation_id).bind(timestamp).execute(&mut *tx).await.map_err(db)?;
        if let Some(details) = inherited_assistance {
            Self::insert_assistance(
                &mut tx,
                &req.operation_id,
                &req.program_id,
                &req.session_id,
                "tutor_response",
                mode(&req.mode),
                0,
                &details,
                timestamp,
            )
            .await?;
        }
        practice_operation(
            &req.operation_id,
            &req.program_id,
            &req.session_id,
            "start",
            payload_hash,
        )
        .record(&mut *tx, &())
        .await?;
        tx.commit().await.map_err(db)?;
        self.workspace(&req.program_id).await
    }

    pub async fn save_artifact(
        &self,
        req: &SaveLearningPracticeArtifactRequestDto,
        payload_hash: &str,
    ) -> Result<LearningPracticeWorkspaceDto> {
        if req.text.chars().count() > MAX_ARTIFACT_CHARS {
            return Err(invalid("Practice artifact is too long."));
        }
        let (summary, _, _, _, artifact_revision) = self
            .session_header(&req.program_id, &req.session_id)
            .await?;
        if self
            .preflight_replay(
                &req.operation_id,
                &req.program_id,
                &req.session_id,
                "save_artifact",
                payload_hash,
            )
            .await?
        {
            return self.workspace(&req.program_id).await;
        }
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &req.program_id).await?;
        if practice_operation(
            &req.operation_id,
            &req.program_id,
            &req.session_id,
            "save_artifact",
            payload_hash,
        )
        .seen(&mut *tx)
        .await?
        {
            tx.commit().await.map_err(db)?;
            return self.workspace(&req.program_id).await;
        }
        Self::verify_active(&mut tx, &req.program_id).await?;
        if summary.status != LearningPracticeSessionStatus::Active {
            return Err(invalid("Submitted practice attempts are immutable."));
        }
        if summary.revision != req.expected_revision {
            return Err(invalid("Practice session changed; reload and retry."));
        }
        let next = artifact_revision + 1;
        let t = now();
        let update=sqlx::query("UPDATE learning_practice_sessions SET revision=revision+1,updated_at=? WHERE id=? AND program_id=? AND revision=? AND status='active'").bind(t).bind(&req.session_id).bind(&req.program_id).bind(req.expected_revision).execute(&mut *tx).await.map_err(db)?;
        if update.rows_affected() != 1 {
            return Err(invalid("Practice session changed; reload and retry."));
        }
        sqlx::query("INSERT INTO learning_practice_artifact_revisions(program_id,session_id,revision,text,sha256,operation_id,created_at) VALUES(?,?,?,?,?,?,?)").bind(&req.program_id).bind(&req.session_id).bind(next).bind(&req.text).bind(hash_text(&req.text)).bind(&req.operation_id).bind(t).execute(&mut *tx).await.map_err(db)?;
        practice_operation(
            &req.operation_id,
            &req.program_id,
            &req.session_id,
            "save_artifact",
            payload_hash,
        )
        .record(&mut *tx, &())
        .await?;
        tx.commit().await.map_err(db)?;
        self.workspace(&req.program_id).await
    }

    pub async fn change_mode(
        &self,
        req: &ChangeLearningPracticeModeRequestDto,
        payload_hash: &str,
    ) -> Result<LearningPracticeWorkspaceDto> {
        Self::validate_ids(&req.program_id, &req.session_id, Some(&req.operation_id))?;
        if self
            .preflight_replay(
                &req.operation_id,
                &req.program_id,
                &req.session_id,
                "change_mode",
                payload_hash,
            )
            .await?
        {
            return self.workspace(&req.program_id).await;
        }
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &req.program_id).await?;
        if practice_operation(
            &req.operation_id,
            &req.program_id,
            &req.session_id,
            "change_mode",
            payload_hash,
        )
        .seen(&mut *tx)
        .await?
        {
            tx.commit().await.map_err(db)?;
            return self.workspace(&req.program_id).await;
        }
        let (current, artifact_revision) = self
            .current_state_in_tx(&mut tx, &req.program_id, &req.session_id)
            .await?;
        if current.status != LearningPracticeSessionStatus::Active {
            return Err(invalid("Submitted practice attempts are immutable."));
        }
        if current.revision != req.expected_revision {
            return Err(invalid("Practice session changed; reload and retry."));
        }
        if current.revises_session_id.is_some() && req.mode == LearningPracticeMode::Demonstrate {
            return Err(invalid(
                "Revisions use previous feedback and cannot become independent demonstrations.",
            ));
        }
        let old_mode = mode(&current.mode);
        let t = now();
        let update=sqlx::query("UPDATE learning_practice_sessions SET mode=?,revision=revision+1,updated_at=? WHERE program_id=? AND id=? AND revision=? AND status='active'").bind(mode(&req.mode)).bind(t).bind(&req.program_id).bind(&req.session_id).bind(req.expected_revision).execute(&mut *tx).await.map_err(db)?;
        if update.rows_affected() != 1 {
            return Err(invalid("Practice session changed; reload and retry."));
        }
        Self::insert_assistance(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            &req.session_id,
            "mode_changed",
            mode(&req.mode),
            artifact_revision,
            &serde_json::json!({"from":old_mode,"to":mode(&req.mode)}),
            t,
        )
        .await?;
        practice_operation(
            &req.operation_id,
            &req.program_id,
            &req.session_id,
            "change_mode",
            payload_hash,
        )
        .record(&mut *tx, &())
        .await?;
        tx.commit().await.map_err(db)?;
        self.workspace(&req.program_id).await
    }
}
