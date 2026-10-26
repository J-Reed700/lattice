//! Role simulations: starting sessions, appending turns and finishing them.
use super::{
    decode, invalid, json, practical_operation, uuid, InternalActivitySnapshot,
    LearningPracticalRepository, PracticalWorkspaceSnapshot, SimulationTurnWrite,
};
use crate::features::learning::persistence::{db, hash, now};
use crate::features::learning::practical_dto::*;
use lattice_core::shared::error::{AppError, Result};
use sqlx::Row;

impl LearningPracticalRepository {
    pub async fn replay_activity_operation(
        &self,
        program_id: &str,
        activity_id: &str,
        operation_id: &str,
        payload_hash: &str,
    ) -> Result<Option<PracticalWorkspaceSnapshot>> {
        let operation = practical_operation(
            "practical_activity",
            operation_id,
            Some(program_id),
            activity_id,
            "generate",
            payload_hash,
            "Operation ID was reused with different practical activity data.",
        );
        if operation.seen(&self.pool).await? {
            return Ok(Some(self.workspace(program_id).await?));
        }
        Ok(None)
    }

    pub async fn replay_simulation_operation(
        &self,
        program_id: &str,
        session_id: &str,
        kind: &'static str,
        operation_id: &str,
        payload_hash: &str,
    ) -> Result<Option<LearningSimulationSessionDto>> {
        let operation = practical_operation(
            "simulation",
            operation_id,
            Some(program_id),
            session_id,
            kind,
            payload_hash,
            "Operation ID was reused with different simulation data.",
        );
        if operation.seen(&self.pool).await? {
            return Ok(Some(self.simulation(session_id).await?));
        }
        Ok(None)
    }

    pub async fn simulation_generation_context(
        &self,
        program_id: &str,
        activity_id: &str,
    ) -> Result<(
        crate::features::learning::practical_generation::SimulationActivityContext,
        Vec<String>,
    )> {
        uuid(program_id, "program")?;
        uuid(activity_id, "activity")?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        let activity = Self::internal_activity(&mut tx, program_id, activity_id).await?;
        tx.commit().await.map_err(db)?;
        Ok((
            crate::features::learning::practical_generation::SimulationActivityContext {
                title: activity.title,
                brief: activity.brief,
                rubric: activity.rubric,
                interview_plan: activity.interview_plan,
                interview_answers: activity.interview_answers,
            },
            activity.source_version_ids,
        ))
    }

    /// An attempt always continues against the exact activity snapshot it began with.
    pub async fn simulation_session_generation_context(
        &self,
        program_id: &str,
        session_id: &str,
    ) -> Result<(
        crate::features::learning::practical_generation::SimulationActivityContext,
        Vec<String>,
    )> {
        uuid(program_id, "program")?;
        uuid(session_id, "simulation")?;
        let snapshot: String = sqlx::query_scalar(
            "SELECT activity_snapshot_json FROM learning_simulation_sessions WHERE program_id=? AND id=?",
        )
        .bind(program_id).bind(session_id).fetch_optional(&self.pool).await.map_err(db)?
        .ok_or_else(|| AppError::NotFound("Simulation not found".into()))?;
        let activity: InternalActivitySnapshot = decode(&snapshot)?;
        Ok((
            crate::features::learning::practical_generation::SimulationActivityContext {
                title: activity.title,
                brief: activity.brief,
                rubric: activity.rubric,
                interview_plan: activity.interview_plan,
                interview_answers: activity.interview_answers,
            },
            activity.source_version_ids,
        ))
    }

    pub async fn start_simulation(
        &self,
        request: &StartLearningSimulationRequestDto,
        payload_hash: &str,
    ) -> Result<LearningSimulationSessionDto> {
        for (value, label) in [
            (&request.operation_id, "operation"),
            (&request.session_id, "simulation"),
            (&request.program_id, "program"),
            (&request.activity_id, "activity"),
        ] {
            uuid(value, label)?;
        }
        if let Some(session) = request.practice_session_id.as_deref() {
            uuid(session, "practice session")?;
        }
        for (value, label) in [
            (&request.learner_role, "Learner role"),
            (&request.counterpart_role, "Counterpart role"),
        ] {
            if value.trim().is_empty() || value.chars().count() > 100 {
                return Err(invalid(format!("{label} must contain 1–100 characters.")));
            }
        }
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &request.program_id).await?;
        let operation = practical_operation(
            "simulation",
            &request.operation_id,
            Some(&request.program_id),
            &request.session_id,
            "start",
            payload_hash,
            "Operation ID was reused with different simulation data.",
        );
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return self.simulation(&request.session_id).await;
        }
        Self::active_program(&mut tx, &request.program_id).await?;
        let activity =
            Self::internal_activity(&mut tx, &request.program_id, &request.activity_id).await?;
        if activity.revision != request.expected_activity_revision {
            return Err(invalid("Practical activity changed; reload and retry."));
        }
        if let Some(practice_id) = request.practice_session_id.as_deref() {
            let owned: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM learning_practice_sessions WHERE program_id=? AND id=?",
            )
            .bind(&request.program_id)
            .bind(practice_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
            if owned.is_none() {
                return Err(invalid("Practice session does not belong to this program."));
            }
        }
        let timestamp = now();
        sqlx::query("INSERT INTO learning_simulation_sessions(id,program_id,activity_id,activity_revision,activity_snapshot_json,practice_session_id,operation_id,payload_hash,learner_role,counterpart_role,status,revision,created_at,updated_at,submitted_at) VALUES(?,?,?,?,?,?,?,?,?,?,'active',0,?,?,NULL)")
            .bind(&request.session_id).bind(&request.program_id).bind(&request.activity_id)
            .bind(activity.revision).bind(json(&activity)?).bind(&request.practice_session_id)
            .bind(&request.operation_id).bind(payload_hash).bind(request.learner_role.trim())
            .bind(request.counterpart_role.trim()).bind(timestamp).bind(timestamp)
            .execute(&mut *tx).await.map_err(db)?;
        // The interviewer opens with the first planned question. Starting and
        // replaying remain atomic and do not fabricate a learner response.
        if let Some(question) = activity
            .interview_plan
            .as_ref()
            .and_then(|plan| plan.questions.first())
        {
            sqlx::query("INSERT INTO learning_simulation_turns(id,program_id,session_id,operation_id,payload_hash,ordinal,speaker,content,citations_json,model_name,created_at) VALUES(?,?,?,?,?,0,'counterpart',?,'[]',?,?)")
                .bind(uuid::Uuid::new_v4().to_string()).bind(&request.program_id).bind(&request.session_id)
                .bind(uuid::Uuid::new_v4().to_string()).bind(payload_hash)
                .bind(&question.question).bind(&activity.generator_model).bind(timestamp)
                .execute(&mut *tx).await.map_err(db)?;
        }
        operation.record(&mut *tx, &request.session_id).await?;
        tx.commit().await.map_err(db)?;
        self.simulation(&request.session_id).await
    }

    pub async fn append_simulation_turn(
        &self,
        request: &SendLearningSimulationTurnRequestDto,
        reply: &SimulationTurnWrite,
        payload_hash: &str,
    ) -> Result<LearningSimulationSessionDto> {
        uuid(&request.operation_id, "operation")?;
        uuid(&request.program_id, "program")?;
        uuid(&request.session_id, "simulation")?;
        if request.content.trim().is_empty() || request.content.chars().count() > 8_000 {
            return Err(invalid("Simulation turns must contain 1–8000 characters."));
        }
        if reply.content.trim().is_empty() || reply.content.chars().count() > 6_000 {
            return Err(invalid("Simulation reply must contain 1–6000 characters."));
        }
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &request.program_id).await?;
        let operation = practical_operation(
            "simulation",
            &request.operation_id,
            Some(&request.program_id),
            &request.session_id,
            "turn",
            payload_hash,
            "Operation ID was reused with different turn data.",
        );
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return self.simulation(&request.session_id).await;
        }
        let session = sqlx::query(
            "SELECT status,revision FROM learning_simulation_sessions WHERE id=? AND program_id=?",
        )
        .bind(&request.session_id)
        .bind(&request.program_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Simulation not found".into()))?;
        if session.get::<String, _>("status") != "active" {
            return Err(invalid("Only active simulations accept turns."));
        }
        if session.get::<i64, _>("revision") != request.expected_revision {
            return Err(invalid("Simulation changed; reload and retry."));
        }
        let ordinal: i64 = sqlx::query_scalar(
            "SELECT coalesce(max(ordinal),-1)+1 FROM learning_simulation_turns WHERE session_id=?",
        )
        .bind(&request.session_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        if ordinal >= 160 {
            return Err(invalid("This simulation has reached its turn limit."));
        }
        let timestamp = now();
        let learner_turn_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO learning_simulation_turns(id,program_id,session_id,operation_id,payload_hash,ordinal,speaker,content,citations_json,model_name,created_at) VALUES(?,?,?,?,?,?,'learner',?,'[]',NULL,?)")
            .bind(&learner_turn_id).bind(&request.program_id).bind(&request.session_id)
            .bind(&request.operation_id).bind(payload_hash).bind(ordinal)
            .bind(request.content.trim()).bind(timestamp).execute(&mut *tx).await.map_err(db)?;
        let reply_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO learning_simulation_turns(id,program_id,session_id,operation_id,payload_hash,ordinal,speaker,content,citations_json,model_name,created_at) VALUES(?,?,?,?,?,?,'counterpart',?,?,?,?)")
            .bind(&reply_id).bind(&request.program_id).bind(&request.session_id)
            .bind(uuid::Uuid::new_v4().to_string()).bind(hash(reply)?).bind(ordinal+1)
            .bind(reply.content.trim()).bind(json(&reply.citations)?).bind(&reply.model_name)
            .bind(timestamp).execute(&mut *tx).await.map_err(db)?;
        let updated = sqlx::query("UPDATE learning_simulation_sessions SET revision=revision+1,updated_at=? WHERE id=? AND program_id=? AND revision=? AND status='active'")
            .bind(timestamp).bind(&request.session_id).bind(&request.program_id)
            .bind(request.expected_revision).execute(&mut *tx).await.map_err(db)?;
        if updated.rows_affected() != 1 {
            return Err(invalid("Simulation changed; reload and retry."));
        }
        operation.record(&mut *tx, &reply_id).await?;
        tx.commit().await.map_err(db)?;
        self.simulation(&request.session_id).await
    }

    pub async fn finish_simulation(
        &self,
        request: &FinishLearningSimulationRequestDto,
        feedback: &SimulationTurnWrite,
        payload_hash: &str,
    ) -> Result<LearningSimulationSessionDto> {
        uuid(&request.operation_id, "operation")?;
        uuid(&request.program_id, "program")?;
        uuid(&request.session_id, "simulation")?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &request.program_id).await?;
        let operation = practical_operation(
            "simulation",
            &request.operation_id,
            Some(&request.program_id),
            &request.session_id,
            "finish",
            payload_hash,
            "Operation ID was reused with different finish data.",
        );
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return self.simulation(&request.session_id).await;
        }
        let session =
            sqlx::query("SELECT * FROM learning_simulation_sessions WHERE id=? AND program_id=?")
                .bind(&request.session_id)
                .bind(&request.program_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                .ok_or_else(|| AppError::NotFound("Simulation not found".into()))?;
        if session.get::<String, _>("status") != "active" {
            return Err(invalid("Only active simulations can be submitted."));
        }
        if session.get::<i64, _>("revision") != request.expected_revision {
            return Err(invalid("Simulation changed; reload and retry."));
        }
        let learner_turns = sqlx::query("SELECT content FROM learning_simulation_turns WHERE session_id=? AND speaker='learner' ORDER BY ordinal")
            .bind(&request.session_id).fetch_all(&mut *tx).await.map_err(db)?;
        if learner_turns.is_empty() {
            return Err(invalid(
                "Respond at least once before submitting a simulation.",
            ));
        }
        let activity: InternalActivitySnapshot = decode(session.get("activity_snapshot_json"))?;
        let ordinal: i64 = sqlx::query_scalar(
            "SELECT coalesce(max(ordinal),-1)+1 FROM learning_simulation_turns WHERE session_id=?",
        )
        .bind(&request.session_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        let timestamp = now();
        let feedback_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO learning_simulation_turns(id,program_id,session_id,operation_id,payload_hash,ordinal,speaker,content,citations_json,model_name,created_at) VALUES(?,?,?,?,?,?,'coach',?,?,?,?)")
            .bind(&feedback_id).bind(&request.program_id).bind(&request.session_id)
            .bind(uuid::Uuid::new_v4().to_string()).bind(hash(feedback)?).bind(ordinal)
            .bind(feedback.content.trim()).bind(json(&feedback.citations)?).bind(&feedback.model_name)
            .bind(timestamp).execute(&mut *tx).await.map_err(db)?;
        let updated = sqlx::query("UPDATE learning_simulation_sessions SET status='submitted',revision=revision+1,updated_at=?,submitted_at=? WHERE id=? AND program_id=? AND revision=? AND status='active'")
            .bind(timestamp).bind(timestamp).bind(&request.session_id).bind(&request.program_id)
            .bind(request.expected_revision).execute(&mut *tx).await.map_err(db)?;
        if updated.rows_affected() != 1 {
            return Err(invalid("Simulation changed; reload and retry."));
        }
        let quote = learner_turns.first().map(|row| {
            row.get::<String, _>("content")
                .chars()
                .take(500)
                .collect::<String>()
        });
        for outcome_id in &activity.outcome_ids {
            sqlx::query("INSERT INTO learning_evidence_events(id,program_id,outcome_id,source_kind,source_id,dimension,result,score,observation,evidence_quote,assistance_json,observed_at) VALUES(?,?,?,'simulation',?,'transfer','uncertain',NULL,?,?,?,?)")
                .bind(uuid::Uuid::new_v4().to_string()).bind(&request.program_id).bind(outcome_id)
                .bind(&request.session_id).bind(&feedback.content).bind(&quote)
                .bind(json(&serde_json::json!({"mode":activity.practice_mode,"allowedAids":activity.allowed_aids}))?)
                .bind(timestamp).execute(&mut *tx).await.map_err(db)?;
        }
        operation.record(&mut *tx, &feedback_id).await?;
        tx.commit().await.map_err(db)?;
        self.simulation(&request.session_id).await
    }
}
