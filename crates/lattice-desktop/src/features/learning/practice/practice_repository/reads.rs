//! Reading the practice workspace and sessions back into DTOs, and replay checks.
use super::*;

impl LearningPracticeRepository {
    pub async fn workspace(&self, program_id: &str) -> Result<LearningPracticeWorkspaceDto> {
        uuid(program_id, "program")?;
        let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM learning_programs WHERE id=?")
            .bind(program_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        if exists.is_none() {
            return Err(AppError::NotFound("Learning program not found".into()));
        }
        let rows=sqlx::query("SELECT s.*, a.revision AS artifact_revision, sub.grade_status FROM learning_practice_sessions s LEFT JOIN learning_practice_artifact_revisions a ON a.session_id=s.id AND a.revision=(SELECT max(revision) FROM learning_practice_artifact_revisions WHERE session_id=s.id) LEFT JOIN learning_practice_submissions sub ON sub.session_id=s.id WHERE s.program_id=? ORDER BY s.updated_at DESC,s.id")
            .bind(program_id).fetch_all(&self.pool).await.map_err(db)?;
        let sessions = rows
            .into_iter()
            .map(|r| {
                Ok(LearningPracticeSessionSummaryDto {
                    task_kind: parse_task_kind(r.get("task_kind"))?,
                    revises_session_id: r.get("revises_session_id"),
                    id: r.get("id"),
                    lesson_id: r.get("lesson_id"),
                    lesson_title: r.get("lesson_title"),
                    status: if r.get::<String, _>("status") == "submitted" {
                        LearningPracticeSessionStatus::Submitted
                    } else {
                        LearningPracticeSessionStatus::Active
                    },
                    mode: parse_mode(r.get("mode"))?,
                    revision: r.get("revision"),
                    artifact_revision: r.get::<Option<i64>, _>("artifact_revision").unwrap_or(0),
                    source_version_ids: decode(r.get("source_version_ids_json"))?,
                    created_at: r.get("created_at"),
                    updated_at: r.get("updated_at"),
                    submitted_at: r.get("submitted_at"),
                    grade_status: r
                        .get::<Option<String>, _>("grade_status")
                        .map(|x| parse_grade_status(&x))
                        .transpose()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(LearningPracticeWorkspaceDto {
            program_id: program_id.to_owned(),
            sessions,
        })
    }

    pub async fn get_session(&self, session_id: &str) -> Result<LearningPracticeSessionDto> {
        uuid(session_id, "session")?;
        let row=sqlx::query("SELECT s.*, a.revision AS artifact_revision,a.text AS artifact_text,a.sha256 AS artifact_sha,a.created_at AS artifact_updated,sub.grade_status,sub.artifact_revision AS result_artifact_revision,sub.artifact_text AS result_artifact_text,sub.rubric_json AS result_rubric,sub.criteria_json,sub.evidence_json,sub.assistance_json AS result_assistance,sub.mode AS result_mode,sub.grader_model,sub.submitted_at AS result_submitted_at FROM learning_practice_sessions s LEFT JOIN learning_practice_artifact_revisions a ON a.session_id=s.id AND a.revision=(SELECT max(revision) FROM learning_practice_artifact_revisions WHERE session_id=s.id) LEFT JOIN learning_practice_submissions sub ON sub.session_id=s.id WHERE s.id=?")
            .bind(session_id).fetch_optional(&self.pool).await.map_err(db)?.ok_or_else(||AppError::NotFound("Practice session not found".into()))?;
        let status_text: String = row.get("status");
        let summary = LearningPracticeSessionSummaryDto {
            task_kind: parse_task_kind(row.get("task_kind"))?,
            revises_session_id: row.get("revises_session_id"),
            id: row.get("id"),
            lesson_id: row.get("lesson_id"),
            lesson_title: row.get("lesson_title"),
            status: if status_text == "submitted" {
                LearningPracticeSessionStatus::Submitted
            } else {
                LearningPracticeSessionStatus::Active
            },
            mode: parse_mode(row.get("mode"))?,
            revision: row.get("revision"),
            artifact_revision: row.get::<Option<i64>, _>("artifact_revision").unwrap_or(0),
            source_version_ids: decode(row.get("source_version_ids_json"))?,
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            submitted_at: row.get("submitted_at"),
            grade_status: row
                .get::<Option<String>, _>("grade_status")
                .map(|x| parse_grade_status(&x))
                .transpose()?,
        };
        let rubric: Vec<LearningPracticeCriterionDto> = decode(row.get("rubric_json"))?;
        let artifact = LearningPracticeArtifactDto {
            revision: row.get::<Option<i64>, _>("artifact_revision").unwrap_or(0),
            text: row
                .get::<Option<String>, _>("artifact_text")
                .unwrap_or_default(),
            sha256: row
                .get::<Option<String>, _>("artifact_sha")
                .unwrap_or_else(|| hash_text("")),
            updated_at: row
                .get::<Option<i64>, _>("artifact_updated")
                .unwrap_or(summary.created_at),
        };
        let assistance = self.assistance(session_id).await?;
        let tutor_turns = self.turns(session_id).await?;
        let proposals = self.proposals(session_id).await?;
        let revealed_solution: Option<String> = row.get("revealed_solution");
        let revealed_solution_citations = decode(row.get("revealed_solution_citations_json"))?;
        let result = if let Some(grade) = row.get::<Option<String>, _>("grade_status") {
            Some(LearningPracticeResultDto {
                grade_status: parse_grade_status(&grade)?,
                artifact_revision: row.get("result_artifact_revision"),
                artifact_text: row.get("result_artifact_text"),
                rubric: decode(row.get("result_rubric"))?,
                criteria: decode(row.get("criteria_json"))?,
                evidence: decode(row.get("evidence_json"))?,
                mode_at_submission: parse_mode(row.get("result_mode"))?,
                assistance: decode(row.get("result_assistance"))?,
                grader_model: row.get("grader_model"),
                submitted_at: row.get("result_submitted_at"),
            })
        } else {
            None
        };
        Ok(LearningPracticeSessionDto {
            summary,
            task_prompt: row.get("task_prompt"),
            lesson_objective: row.get("lesson_objective"),
            rubric,
            artifact,
            assistance,
            tutor_turns,
            proposals,
            revealed_solution,
            revealed_solution_citations,
            result,
        })
    }

    async fn assistance(
        &self,
        session_id: &str,
    ) -> Result<Vec<LearningPracticeAssistanceEventDto>> {
        let rows = sqlx::query(
            "SELECT * FROM learning_practice_assistance WHERE session_id=? ORDER BY created_at,id",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.into_iter()
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
            .collect()
    }
    async fn turns(&self, session_id: &str) -> Result<Vec<LearningPracticeTutorTurnDto>> {
        let rows = sqlx::query(
            "SELECT * FROM learning_practice_tutor_turns WHERE session_id=? ORDER BY created_at,id",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.into_iter()
            .map(|r| {
                Ok(LearningPracticeTutorTurnDto {
                    id: r.get("id"),
                    prompt: r.get("prompt"),
                    response: r.get("response"),
                    request_kind: parse_tutor_kind(r.get("request_kind"))?,
                    hint_level: r
                        .get::<Option<String>, _>("hint_level")
                        .map(|x| parse_hint(&x))
                        .transpose()?,
                    citations: decode(r.get("citations_json"))?,
                    proposal_ids: decode(r.get("proposal_ids_json"))?,
                    model_name: r.get("model_name"),
                    created_at: r.get("created_at"),
                })
            })
            .collect()
    }
    async fn proposals(&self, session_id: &str) -> Result<Vec<LearningPracticeProposalDto>> {
        let rows = sqlx::query(
            "SELECT * FROM learning_practice_proposals WHERE session_id=? ORDER BY created_at,id",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.into_iter()
            .map(|r| {
                Ok(LearningPracticeProposalDto {
                    id: r.get("id"),
                    kind: parse_proposal_kind(r.get("kind"))?,
                    text: r.get("text"),
                    evidence_quote: r.get("evidence_quote"),
                    tutor_turn_id: r.get("tutor_turn_id"),
                    status: proposal_status(r.get("status"))?,
                    created_at: r.get("created_at"),
                    decided_at: r.get("decided_at"),
                })
            })
            .collect()
    }

    pub async fn session_header(
        &self,
        program_id: &str,
        session_id: &str,
    ) -> Result<(
        LearningPracticeSessionSummaryDto,
        String,
        Vec<LearningPracticeCriterionDto>,
        String,
        i64,
    )> {
        Self::validate_ids(program_id, session_id, None)?;
        let ws = self.get_session(session_id).await?;
        let owner: Option<String> =
            sqlx::query_scalar("SELECT program_id FROM learning_practice_sessions WHERE id=?")
                .bind(session_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(db)?;
        if owner.as_deref() != Some(program_id) {
            return Err(AppError::NotFound("Practice session not found".into()));
        }
        Ok((
            ws.summary,
            ws.task_prompt,
            ws.rubric,
            ws.artifact.text,
            ws.artifact.revision,
        ))
    }

    pub async fn validate_live_session(
        &self,
        program_id: &str,
        session_id: &str,
        expected_revision: i64,
        allow_demonstrate: bool,
    ) -> Result<LearningPracticeSessionDto> {
        let session = self.get_session(session_id).await?;
        let owner: Option<String> =
            sqlx::query_scalar("SELECT program_id FROM learning_practice_sessions WHERE id=?")
                .bind(session_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(db)?;
        if owner.as_deref() != Some(program_id) {
            return Err(AppError::NotFound("Practice session not found".into()));
        }
        let program_status: Option<String> =
            sqlx::query_scalar("SELECT status FROM learning_programs WHERE id=?")
                .bind(program_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(db)?;
        if program_status.as_deref() != Some("active") {
            return Err(invalid(
                "Practice actions require an active learning program.",
            ));
        }
        if session.summary.status != LearningPracticeSessionStatus::Active {
            return Err(invalid("Submitted practice attempts are immutable."));
        }
        if session.summary.revision != expected_revision {
            return Err(invalid("Practice session changed; reload and retry."));
        }
        if !allow_demonstrate && session.summary.mode == LearningPracticeMode::Demonstrate {
            return Err(invalid("This aid is unavailable in Demonstrate mode."));
        }
        Ok(session)
    }

    /// Load exact frozen source versions. `ids` are immutable version IDs copied
    /// into the session at start, never current active pointers.
    pub async fn frozen_sources(
        &self,
        program_id: &str,
        ids: &[String],
    ) -> Result<Vec<LearningSourceVersionDto>> {
        let mut out = Vec::new();
        for id in ids.iter().take(32) {
            let row=sqlx::query("SELECT source_id,id,version_number,title,publisher,resolved_url,excerpt,content_sha256,word_count,truncated,extraction_version,acquired_at,full_text FROM learning_source_versions WHERE program_id=? AND id=?")
                .bind(program_id).bind(id).fetch_optional(&self.pool).await.map_err(db)?.ok_or_else(||AppError::NotFound("A source version frozen into this attempt is unavailable.".into()))?;
            out.push(LearningSourceVersionDto {
                source_id: row.get("source_id"),
                version: LearningSourceVersionSummaryDto {
                    id: row.get("id"),
                    version_number: row.get("version_number"),
                    title: row.get("title"),
                    publisher: row.get("publisher"),
                    resolved_url: row.get("resolved_url"),
                    excerpt: row.get("excerpt"),
                    content_sha256: row.get("content_sha256"),
                    word_count: row.get::<i64, _>("word_count").max(0) as usize,
                    truncated: row.get::<i64, _>("truncated") != 0,
                    extraction_version: row.get("extraction_version"),
                    acquired_at: row.get("acquired_at"),
                },
                full_text: row.get("full_text"),
                usage: vec![],
            });
        }
        Ok(out)
    }

    pub async fn preflight_replay(
        &self,
        operation_id: &str,
        program_id: &str,
        session_id: &str,
        kind: &str,
        payload_hash: &str,
    ) -> Result<bool> {
        uuid(operation_id, "operation")?;
        practice_operation(operation_id, program_id, session_id, kind, payload_hash)
            .seen(&self.pool)
            .await
    }
}
