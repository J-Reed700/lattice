//! Submitting attempts for grading and deciding tutor proposals.
use super::*;

impl LearningPracticeRepository {
    pub async fn submit(
        &self,
        req: &SubmitLearningPracticeAttemptRequestDto,
        write: SubmissionWrite,
        rubric: &[LearningPracticeCriterionDto],
    ) -> Result<LearningPracticeWorkspaceDto> {
        Self::validate_ids(&req.program_id, &req.session_id, Some(&req.operation_id))?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &req.program_id).await?;
        if practice_operation(
            &req.operation_id,
            &req.program_id,
            &req.session_id,
            "submit",
            &write.payload_hash,
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
            return Err(invalid("Practice attempt has already been submitted."));
        }
        if s.revision != req.expected_revision {
            return Err(invalid("Practice session changed; reload and retry."));
        }
        let artifact=sqlx::query("SELECT text FROM learning_practice_artifact_revisions WHERE session_id=? AND revision=?").bind(&req.session_id).bind(ar).fetch_one(&mut *tx).await.map_err(db)?;
        let artifact_text: String = artifact.get("text");
        let assistance_rows = sqlx::query(
            "SELECT * FROM learning_practice_assistance WHERE session_id=? ORDER BY created_at,id",
        )
        .bind(&req.session_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        let assistance = assistance_rows
            .into_iter()
            .map(|r| {
                Ok(LearningPracticeAssistanceEventDto {
                    id: r.get("id"),
                    kind: parse_assistance_kind(r.get("kind"))?,
                    mode: parse_mode(r.get("mode"))?,
                    artifact_revision: r.get("artifact_revision"),
                    details: decode(r.get("details_json"))?,
                    created_at: r.get("created_at"),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let t = now();
        let rubric_json = json(rubric)?;
        let criteria_json = json(&write.criteria)?;
        let evidence_json = json(&write.evidence)?;
        let assistance_json = json(&assistance)?;
        sqlx::query("INSERT INTO learning_practice_submissions(session_id,program_id,operation_id,payload_hash,grade_status,artifact_revision,artifact_text,rubric_json,criteria_json,evidence_json,assistance_json,mode,grader_model,submitted_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(&req.session_id).bind(&req.program_id).bind(&req.operation_id).bind(&write.payload_hash).bind(grade_status(&write.grade_status)).bind(ar).bind(&artifact_text).bind(rubric_json).bind(criteria_json).bind(evidence_json).bind(assistance_json).bind(mode(&s.mode)).bind(&write.grader_model).bind(t).execute(&mut *tx).await.map_err(db)?;
        for event in &write.evidence {
            sqlx::query("INSERT INTO learning_practice_evidence_events(id,program_id,session_id,dimension,observed,observation,evidence_quote,assistance_kinds_json,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(uuid::Uuid::new_v4().to_string()).bind(&req.program_id).bind(&req.session_id).bind(dimension(&event.dimension)).bind(event.observed as i64).bind(&event.observation).bind(&event.evidence_quote).bind(json(&event.assistance_kinds)?).bind(t).execute(&mut *tx).await.map_err(db)?;
        }
        let upd=sqlx::query("UPDATE learning_practice_sessions SET status='submitted',revision=revision+1,submitted_at=?,updated_at=? WHERE program_id=? AND id=? AND revision=? AND status='active'").bind(t).bind(t).bind(&req.program_id).bind(&req.session_id).bind(req.expected_revision).execute(&mut *tx).await.map_err(db)?;
        if upd.rows_affected() != 1 {
            return Err(invalid("Practice session changed; reload and retry."));
        }
        practice_operation(
            &req.operation_id,
            &req.program_id,
            &req.session_id,
            "submit",
            &write.payload_hash,
        )
        .record(&mut *tx, &())
        .await?;
        tx.commit().await.map_err(db)?;
        self.workspace(&req.program_id).await
    }

    pub async fn decide_proposal(
        &self,
        req: &DecideLearningPracticeProposalRequestDto,
        accept: bool,
        payload_hash: &str,
    ) -> Result<LearningPracticeWorkspaceDto> {
        Self::validate_ids(&req.program_id, &req.session_id, Some(&req.operation_id))?;
        uuid(&req.proposal_id, "proposal")?;
        let kind = if accept {
            "accept_proposal"
        } else {
            "reject_proposal"
        };
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &req.program_id).await?;
        if practice_operation(
            &req.operation_id,
            &req.program_id,
            &req.session_id,
            kind,
            payload_hash,
        )
        .seen(&mut *tx)
        .await?
        {
            tx.commit().await.map_err(db)?;
            return self.workspace(&req.program_id).await;
        }
        let (s, _) = self
            .current_state_in_tx(&mut tx, &req.program_id, &req.session_id)
            .await?;
        if s.status != LearningPracticeSessionStatus::Active {
            return Err(invalid("Submitted practice attempts are immutable."));
        }
        if s.revision != req.expected_revision {
            return Err(invalid("Practice session changed; reload and retry."));
        }
        let t = now();
        let status = if accept { "accepted" } else { "rejected" };
        let res=sqlx::query("UPDATE learning_practice_proposals SET status=?,decided_at=? WHERE id=? AND session_id=? AND status='pending'").bind(status).bind(t).bind(&req.proposal_id).bind(&req.session_id).execute(&mut *tx).await.map_err(db)?;
        if res.rows_affected() != 1 {
            return Err(invalid("Pending proposal not found."));
        }
        let upd=sqlx::query("UPDATE learning_practice_sessions SET revision=revision+1,updated_at=? WHERE id=? AND program_id=? AND revision=? AND status='active'").bind(t).bind(&req.session_id).bind(&req.program_id).bind(req.expected_revision).execute(&mut *tx).await.map_err(db)?;
        if upd.rows_affected() != 1 {
            return Err(invalid("Practice session changed; reload and retry."));
        }
        practice_operation(
            &req.operation_id,
            &req.program_id,
            &req.session_id,
            kind,
            payload_hash,
        )
        .record(&mut *tx, &req.proposal_id)
        .await?;
        tx.commit().await.map_err(db)?;
        self.workspace(&req.program_id).await
    }
}
