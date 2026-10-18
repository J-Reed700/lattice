//! Recording assistance: opened sources, tutor turns and revealed solutions.
use super::*;

impl LearningPracticeRepository {
    pub async fn open_source(
        &self,
        req: &OpenLearningPracticeSourceRequestDto,
        payload_hash: &str,
    ) -> Result<LearningPracticeWorkspaceDto> {
        Self::validate_ids(&req.program_id, &req.session_id, Some(&req.operation_id))?;
        if self
            .preflight_replay(
                &req.operation_id,
                &req.program_id,
                &req.session_id,
                "open_source",
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
            "open_source",
            payload_hash,
        )
        .seen(&mut *tx)
        .await?
        {
            tx.commit().await.map_err(db)?;
            return self.workspace(&req.program_id).await;
        }
        let (s, ar) = self
            .current_state_in_tx(&mut tx, &req.program_id, &req.session_id)
            .await?;
        if s.status != LearningPracticeSessionStatus::Active {
            return Err(invalid("Submitted practice attempts are immutable."));
        }
        if s.revision != req.expected_revision {
            return Err(invalid("Practice session changed; reload and retry."));
        }
        if s.mode == LearningPracticeMode::Demonstrate {
            return Err(invalid("Source access is unavailable in Demonstrate mode."));
        }
        if !s.source_version_ids.iter().any(|id| id == &req.version_id) {
            return Err(invalid(
                "This source version is not frozen into the attempt.",
            ));
        }
        let found: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM learning_source_versions WHERE program_id=? AND source_id=? AND id=?",
        )
        .bind(&req.program_id)
        .bind(&req.source_id)
        .bind(&req.version_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        if found.is_none() {
            return Err(AppError::NotFound("Frozen source version not found".into()));
        }
        let t = now();
        let update=sqlx::query("UPDATE learning_practice_sessions SET revision=revision+1,updated_at=? WHERE program_id=? AND id=? AND revision=? AND status='active'").bind(t).bind(&req.program_id).bind(&req.session_id).bind(req.expected_revision).execute(&mut *tx).await.map_err(db)?;
        if update.rows_affected() != 1 {
            return Err(invalid("Practice session changed; reload and retry."));
        }
        Self::insert_assistance(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            &req.session_id,
            "source_opened",
            mode(&s.mode),
            ar,
            &serde_json::json!({"sourceId":req.source_id,"versionId":req.version_id}),
            t,
        )
        .await?;
        practice_operation(
            &req.operation_id,
            &req.program_id,
            &req.session_id,
            "open_source",
            payload_hash,
        )
        .record(&mut *tx, &())
        .await?;
        tx.commit().await.map_err(db)?;
        self.workspace(&req.program_id).await
    }

    pub async fn commit_tutor_turn(
        &self,
        program_id: &str,
        session_id: &str,
        expected_revision: i64,
        turn: TutorTurnWrite,
    ) -> Result<LearningPracticeWorkspaceDto> {
        Self::validate_ids(program_id, session_id, Some(&turn.operation_id))?;
        uuid(&turn.id, "tutor turn")?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, program_id).await?;
        if practice_operation(
            &turn.operation_id,
            program_id,
            session_id,
            "tutor",
            &turn.payload_hash,
        )
        .seen(&mut *tx)
        .await?
        {
            tx.commit().await.map_err(db)?;
            return self.workspace(program_id).await;
        }
        let (s, ar) = self
            .current_state_in_tx(&mut tx, program_id, session_id)
            .await?;
        if s.status != LearningPracticeSessionStatus::Active {
            return Err(invalid("Submitted practice attempts are immutable."));
        }
        if s.revision != expected_revision {
            return Err(invalid("Practice session changed; reload and retry."));
        }
        if s.mode == LearningPracticeMode::Demonstrate {
            return Err(invalid(
                "Tutor assistance is unavailable in Demonstrate mode.",
            ));
        }
        let t = now();
        let upd=sqlx::query("UPDATE learning_practice_sessions SET revision=revision+1,updated_at=? WHERE program_id=? AND id=? AND revision=? AND status='active'").bind(t).bind(program_id).bind(session_id).bind(expected_revision).execute(&mut *tx).await.map_err(db)?;
        if upd.rows_affected() != 1 {
            return Err(invalid("Practice session changed; reload and retry."));
        }
        let citations = json(&turn.citations)?;
        let proposal_ids: Vec<String> = turn.proposals.iter().map(|p| p.id.clone()).collect();
        sqlx::query("INSERT INTO learning_practice_tutor_turns(id,program_id,session_id,operation_id,prompt,response,request_kind,hint_level,citations_json,proposal_ids_json,model_name,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(&turn.id).bind(program_id).bind(session_id).bind(&turn.operation_id).bind(&turn.prompt).bind(&turn.response).bind(tutor_kind(&turn.request_kind)).bind(turn.hint_level.as_ref().map(hint)).bind(citations).bind(json(&proposal_ids)?).bind(&turn.model_name).bind(t).execute(&mut *tx).await.map_err(db)?;
        for proposal in &turn.proposals {
            sqlx::query("INSERT INTO learning_practice_proposals(id,program_id,session_id,tutor_turn_id,kind,text,evidence_quote,status,created_at) VALUES(?,?,?,?,?,?,?,'pending',?)").bind(&proposal.id).bind(program_id).bind(session_id).bind(&turn.id).bind(proposal_kind(&proposal.kind)).bind(&proposal.text).bind(&proposal.evidence_quote).bind(t).execute(&mut *tx).await.map_err(db)?;
        }
        let details =
            serde_json::json!({"turnId":turn.id,"hintLevel":turn.hint_level.as_ref().map(hint)});
        let ledger_kind = if turn.request_kind == LearningTutorRequestKind::Hint {
            "hint"
        } else {
            "tutor_response"
        };
        Self::insert_assistance(
            &mut tx,
            &turn.operation_id,
            program_id,
            session_id,
            ledger_kind,
            mode(&s.mode),
            ar,
            &details,
            t,
        )
        .await?;
        practice_operation(
            &turn.operation_id,
            program_id,
            session_id,
            "tutor",
            &turn.payload_hash,
        )
        .record(&mut *tx, &turn.id)
        .await?;
        tx.commit().await.map_err(db)?;
        self.workspace(program_id).await
    }

    pub async fn commit_solution(
        &self,
        req: &RevealLearningPracticeSolutionRequestDto,
        payload_hash: &str,
        solution: &str,
        citations: &[LearningPracticeCitationDto],
    ) -> Result<LearningPracticeWorkspaceDto> {
        Self::validate_ids(&req.program_id, &req.session_id, Some(&req.operation_id))?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &req.program_id).await?;
        if practice_operation(
            &req.operation_id,
            &req.program_id,
            &req.session_id,
            "reveal_solution",
            payload_hash,
        )
        .seen(&mut *tx)
        .await?
        {
            tx.commit().await.map_err(db)?;
            return self.workspace(&req.program_id).await;
        }
        let (s, ar) = self
            .current_state_in_tx(&mut tx, &req.program_id, &req.session_id)
            .await?;
        if s.status != LearningPracticeSessionStatus::Active {
            return Err(invalid("Submitted practice attempts are immutable."));
        }
        if s.revision != req.expected_revision {
            return Err(invalid("Practice session changed; reload and retry."));
        }
        if s.mode == LearningPracticeMode::Demonstrate {
            return Err(invalid(
                "Solution reveal is unavailable in Demonstrate mode.",
            ));
        }
        let t = now();
        let upd=sqlx::query("UPDATE learning_practice_sessions SET revealed_solution=?,revealed_solution_citations_json=?,revision=revision+1,updated_at=? WHERE program_id=? AND id=? AND revision=? AND status='active'").bind(solution).bind(json(citations)?).bind(t).bind(&req.program_id).bind(&req.session_id).bind(req.expected_revision).execute(&mut *tx).await.map_err(db)?;
        if upd.rows_affected() != 1 {
            return Err(invalid("Practice session changed; reload and retry."));
        }
        Self::insert_assistance(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            &req.session_id,
            "solution_revealed",
            mode(&s.mode),
            ar,
            &serde_json::json!({"citationCount":citations.len()}),
            t,
        )
        .await?;
        practice_operation(
            &req.operation_id,
            &req.program_id,
            &req.session_id,
            "reveal_solution",
            payload_hash,
        )
        .record(&mut *tx, &())
        .await?;
        tx.commit().await.map_err(db)?;
        self.workspace(&req.program_id).await
    }
}
