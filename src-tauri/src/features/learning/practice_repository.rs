//! Durable attempt/session storage for the Grounded Practice Workbench.
use super::dto::*;
use crate::shared::error::{AppError, Result};
use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

const MAX_ARTIFACT_CHARS: usize = 24_000;
const MAX_TUTOR_PROMPT_CHARS: usize = 2_000;

fn db(error: sqlx::Error) -> AppError {
    AppError::Database(error.to_string())
}
fn invalid(message: impl Into<String>) -> AppError {
    AppError::InvalidInput(message.into())
}
fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
fn json<T: Serialize + ?Sized>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|e| AppError::Serialization(e.to_string()))
}
fn decode<T: DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_str(value).map_err(|e| AppError::Serialization(e.to_string()))
}
fn uuid(value: &str, label: &str) -> Result<()> {
    uuid::Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| invalid(format!("Invalid {label} ID")))
}
fn mode(value: &LearningPracticeMode) -> &'static str {
    match value {
        LearningPracticeMode::Explore => "explore",
        LearningPracticeMode::Practice => "practice",
        LearningPracticeMode::Demonstrate => "demonstrate",
    }
}
fn parse_mode(value: &str) -> Result<LearningPracticeMode> {
    match value {
        "explore" => Ok(LearningPracticeMode::Explore),
        "practice" => Ok(LearningPracticeMode::Practice),
        "demonstrate" => Ok(LearningPracticeMode::Demonstrate),
        _ => Err(AppError::Database("Invalid practice mode".into())),
    }
}
fn hint(value: &LearningPracticeHintLevel) -> &'static str {
    match value {
        LearningPracticeHintLevel::OrientingQuestion => "orienting_question",
        LearningPracticeHintLevel::ConceptOrSource => "concept_or_source",
        LearningPracticeHintLevel::PartialStrategy => "partial_strategy",
        LearningPracticeHintLevel::WorkedExplanation => "worked_explanation",
    }
}
fn parse_hint(value: &str) -> Result<LearningPracticeHintLevel> {
    match value {
        "orienting_question" => Ok(LearningPracticeHintLevel::OrientingQuestion),
        "concept_or_source" => Ok(LearningPracticeHintLevel::ConceptOrSource),
        "partial_strategy" => Ok(LearningPracticeHintLevel::PartialStrategy),
        "worked_explanation" => Ok(LearningPracticeHintLevel::WorkedExplanation),
        _ => Err(AppError::Database("Invalid tutor hint level".into())),
    }
}
fn tutor_kind(value: &LearningTutorRequestKind) -> &'static str {
    match value {
        LearningTutorRequestKind::Hint => "hint",
        LearningTutorRequestKind::Question => "question",
        LearningTutorRequestKind::Critique => "critique",
    }
}
fn parse_tutor_kind(value: &str) -> Result<LearningTutorRequestKind> {
    match value {
        "hint" => Ok(LearningTutorRequestKind::Hint),
        "question" => Ok(LearningTutorRequestKind::Question),
        "critique" => Ok(LearningTutorRequestKind::Critique),
        _ => Err(AppError::Database("Invalid tutor request kind".into())),
    }
}
fn parse_assistance_kind(value: &str) -> Result<LearningPracticeAssistanceKind> {
    match value {
        "source_opened" => Ok(LearningPracticeAssistanceKind::SourceOpened),
        "tutor_response" => Ok(LearningPracticeAssistanceKind::TutorResponse),
        "hint" => Ok(LearningPracticeAssistanceKind::Hint),
        "solution_revealed" => Ok(LearningPracticeAssistanceKind::SolutionRevealed),
        "mode_changed" => Ok(LearningPracticeAssistanceKind::ModeChanged),
        _ => Err(AppError::Database(
            "Invalid practice assistance kind".into(),
        )),
    }
}
fn grade_status(value: &LearningPracticeGradeStatus) -> &'static str {
    match value {
        LearningPracticeGradeStatus::Provisional => "provisional",
        LearningPracticeGradeStatus::Uncertain => "uncertain",
    }
}
fn parse_grade_status(value: &str) -> Result<LearningPracticeGradeStatus> {
    match value {
        "provisional" => Ok(LearningPracticeGradeStatus::Provisional),
        "uncertain" => Ok(LearningPracticeGradeStatus::Uncertain),
        _ => Err(AppError::Database("Invalid practice grade status".into())),
    }
}
fn proposal_kind(value: &LearningPracticeProposalKind) -> &'static str {
    match value {
        LearningPracticeProposalKind::Misconception => "misconception",
        LearningPracticeProposalKind::FollowUp => "follow_up",
    }
}
fn parse_proposal_kind(value: &str) -> Result<LearningPracticeProposalKind> {
    match value {
        "misconception" => Ok(LearningPracticeProposalKind::Misconception),
        "follow_up" => Ok(LearningPracticeProposalKind::FollowUp),
        _ => Err(AppError::Database("Invalid proposal kind".into())),
    }
}
fn proposal_status(value: &str) -> Result<LearningPracticeProposalStatus> {
    match value {
        "pending" => Ok(LearningPracticeProposalStatus::Pending),
        "accepted" => Ok(LearningPracticeProposalStatus::Accepted),
        "rejected" => Ok(LearningPracticeProposalStatus::Rejected),
        _ => Err(AppError::Database("Invalid proposal status".into())),
    }
}

#[derive(Clone)]
pub struct LearningPracticeRepository {
    pool: SqlitePool,
}

#[derive(Debug, Clone)]
pub struct TutorTurnWrite {
    pub id: String,
    pub operation_id: String,
    pub payload_hash: String,
    pub prompt: String,
    pub request_kind: LearningTutorRequestKind,
    pub response: String,
    pub hint_level: Option<LearningPracticeHintLevel>,
    pub citations: Vec<LearningPracticeCitationDto>,
    pub proposals: Vec<LearningPracticeProposalDto>,
    pub model_name: String,
}

#[derive(Debug, Clone)]
pub struct SubmissionWrite {
    pub payload_hash: String,
    pub operation_id: String,
    pub grade_status: LearningPracticeGradeStatus,
    pub criteria: Vec<LearningPracticeCriterionResultDto>,
    pub evidence: Vec<LearningPracticeEvidenceEventDto>,
    pub grader_model: String,
}

impl LearningPracticeRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    fn validate_ids(program_id: &str, session_id: &str, operation_id: Option<&str>) -> Result<()> {
        uuid(program_id, "program")?;
        uuid(session_id, "session")?;
        if let Some(id) = operation_id {
            uuid(id, "operation")?;
        }
        Ok(())
    }

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
                .unwrap_or_else(|| digest("")),
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
        let row=sqlx::query("SELECT program_id,session_id,kind,payload_hash FROM learning_practice_operations WHERE operation_id=?").bind(operation_id).fetch_optional(&self.pool).await.map_err(db)?;
        let Some(r) = row else { return Ok(false) };
        if r.get::<String, _>("program_id") == program_id
            && r.get::<String, _>("session_id") == session_id
            && r.get::<String, _>("kind") == kind
            && r.get::<String, _>("payload_hash") == payload_hash
        {
            Ok(true)
        } else {
            Err(invalid(
                "Operation ID was already used with different practice data.",
            ))
        }
    }

    async fn writer_lock(tx: &mut Transaction<'_, Sqlite>, program_id: &str) -> Result<()> {
        sqlx::query("UPDATE learning_programs SET revision=revision WHERE id=?")
            .bind(program_id)
            .execute(&mut **tx)
            .await
            .map_err(db)?;
        Ok(())
    }
    async fn verify_active(tx: &mut Transaction<'_, Sqlite>, program_id: &str) -> Result<()> {
        let status: Option<String> =
            sqlx::query_scalar("SELECT status FROM learning_programs WHERE id=?")
                .bind(program_id)
                .fetch_optional(&mut **tx)
                .await
                .map_err(db)?;
        match status.as_deref() {
            Some("active") => Ok(()),
            Some(_) => Err(invalid("Accept this program before starting practice.")),
            None => Err(AppError::NotFound("Learning program not found".into())),
        }
    }
    async fn record_operation(
        tx: &mut Transaction<'_, Sqlite>,
        operation_id: &str,
        program_id: &str,
        session_id: &str,
        kind: &str,
        payload_hash: &str,
        response_id: Option<&str>,
    ) -> Result<()> {
        sqlx::query("INSERT INTO learning_practice_operations(operation_id,program_id,session_id,kind,payload_hash,response_id,created_at) VALUES(?,?,?,?,?,?,?)").bind(operation_id).bind(program_id).bind(session_id).bind(kind).bind(payload_hash).bind(response_id).bind(now()).execute(&mut **tx).await.map_err(db)?;
        Ok(())
    }

    pub async fn start(
        &self,
        req: &StartLearningPracticeSessionRequestDto,
        payload_hash: &str,
    ) -> Result<LearningPracticeWorkspaceDto> {
        Self::validate_ids(&req.program_id, &req.session_id, Some(&req.operation_id))?;
        uuid(&req.lesson_id, "lesson")?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &req.program_id).await?;
        if let Some(row)=sqlx::query("SELECT program_id,session_id,kind,payload_hash FROM learning_practice_operations WHERE operation_id=?").bind(&req.operation_id).fetch_optional(&mut *tx).await.map_err(db)? {
            if row.get::<String,_>("program_id")==req.program_id && row.get::<String,_>("session_id")==req.session_id && row.get::<String,_>("kind")=="start" && row.get::<String,_>("payload_hash")==payload_hash { tx.commit().await.map_err(db)?; return self.workspace(&req.program_id).await; }
            return Err(invalid("Operation ID was already used with different practice data."));
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
        for row in sqlx::query("SELECT source_ids_json FROM learning_blocks WHERE lesson_id=? UNION ALL SELECT source_ids_json FROM learning_questions WHERE lesson_id=?").bind(&req.lesson_id).bind(&req.lesson_id).fetch_all(&mut *tx).await.map_err(db)? {
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
        sqlx::query("INSERT INTO learning_practice_artifact_revisions(program_id,session_id,revision,text,sha256,operation_id,created_at) VALUES(?,?,0,?,?,?,?)").bind(&req.program_id).bind(&req.session_id).bind(&initial_text).bind(digest(&initial_text)).bind(&req.operation_id).bind(timestamp).execute(&mut *tx).await.map_err(db)?;
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
        Self::record_operation(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            &req.session_id,
            "start",
            payload_hash,
            None,
        )
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
        if let Some(r)=sqlx::query("SELECT payload_hash,program_id,session_id,kind FROM learning_practice_operations WHERE operation_id=?").bind(&req.operation_id).fetch_optional(&mut *tx).await.map_err(db)? {
            if r.get::<String,_>("payload_hash")==payload_hash && r.get::<String,_>("program_id")==req.program_id && r.get::<String,_>("session_id")==req.session_id && r.get::<String,_>("kind")=="save_artifact" {tx.commit().await.map_err(db)?;return self.workspace(&req.program_id).await;} return Err(invalid("Operation ID was already used with different practice data."));
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
        sqlx::query("INSERT INTO learning_practice_artifact_revisions(program_id,session_id,revision,text,sha256,operation_id,created_at) VALUES(?,?,?,?,?,?,?)").bind(&req.program_id).bind(&req.session_id).bind(next).bind(&req.text).bind(digest(&req.text)).bind(&req.operation_id).bind(t).execute(&mut *tx).await.map_err(db)?;
        Self::record_operation(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            &req.session_id,
            "save_artifact",
            payload_hash,
            None,
        )
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
        if let Some(r)=sqlx::query("SELECT payload_hash,program_id,session_id,kind FROM learning_practice_operations WHERE operation_id=?").bind(&req.operation_id).fetch_optional(&mut *tx).await.map_err(db)?{if r.get::<String,_>("payload_hash")==payload_hash&&r.get::<String,_>("program_id")==req.program_id&&r.get::<String,_>("session_id")==req.session_id&&r.get::<String,_>("kind")=="change_mode"{tx.commit().await.map_err(db)?;return self.workspace(&req.program_id).await;}return Err(invalid("Operation ID was already used with different practice data."));}
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
        Self::record_operation(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            &req.session_id,
            "change_mode",
            payload_hash,
            None,
        )
        .await?;
        tx.commit().await.map_err(db)?;
        self.workspace(&req.program_id).await
    }

    async fn current_state_in_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        program_id: &str,
        session_id: &str,
    ) -> Result<(LearningPracticeSessionSummaryDto, i64)> {
        let program_status: Option<String> =
            sqlx::query_scalar("SELECT status FROM learning_programs WHERE id=?")
                .bind(program_id)
                .fetch_optional(&mut **tx)
                .await
                .map_err(db)?;
        if program_status.as_deref() != Some("active") {
            return Err(invalid(
                "Practice actions require an active learning program.",
            ));
        }
        let row=sqlx::query("SELECT s.*, (SELECT max(revision) FROM learning_practice_artifact_revisions a WHERE a.session_id=s.id) artifact_revision, sub.grade_status FROM learning_practice_sessions s LEFT JOIN learning_practice_submissions sub ON sub.session_id=s.id WHERE s.program_id=? AND s.id=?").bind(program_id).bind(session_id).fetch_optional(&mut **tx).await.map_err(db)?.ok_or_else(||AppError::NotFound("Practice session not found".into()))?;
        let summary = LearningPracticeSessionSummaryDto {
            task_kind: parse_task_kind(row.get("task_kind"))?,
            revises_session_id: row.get("revises_session_id"),
            id: row.get("id"),
            lesson_id: row.get("lesson_id"),
            lesson_title: row.get("lesson_title"),
            status: if row.get::<String, _>("status") == "submitted" {
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
                .map(|s| parse_grade_status(&s))
                .transpose()?,
        };
        let revision = summary.artifact_revision;
        Ok((summary, revision))
    }

    async fn insert_assistance(
        tx: &mut Transaction<'_, Sqlite>,
        id: &str,
        program_id: &str,
        session_id: &str,
        kind: &str,
        mode: &str,
        artifact_revision: i64,
        details: &serde_json::Value,
        timestamp: i64,
    ) -> Result<()> {
        sqlx::query("INSERT INTO learning_practice_assistance(id,program_id,session_id,operation_id,kind,mode,artifact_revision,details_json,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(uuid::Uuid::new_v4().to_string()).bind(program_id).bind(session_id).bind(id).bind(kind).bind(mode).bind(artifact_revision).bind(json(details)?).bind(timestamp).execute(&mut **tx).await.map_err(db)?;
        Ok(())
    }

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
        if let Some(r)=sqlx::query("SELECT payload_hash,program_id,session_id,kind FROM learning_practice_operations WHERE operation_id=?").bind(&req.operation_id).fetch_optional(&mut *tx).await.map_err(db)?{if r.get::<String,_>("payload_hash")==payload_hash&&r.get::<String,_>("program_id")==req.program_id&&r.get::<String,_>("session_id")==req.session_id&&r.get::<String,_>("kind")=="open_source"{tx.commit().await.map_err(db)?;return self.workspace(&req.program_id).await;}return Err(invalid("Operation ID was already used with different practice data."));}
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
        Self::record_operation(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            &req.session_id,
            "open_source",
            payload_hash,
            None,
        )
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
        if let Some(r)=sqlx::query("SELECT program_id,session_id,kind,payload_hash FROM learning_practice_operations WHERE operation_id=?").bind(&turn.operation_id).fetch_optional(&mut *tx).await.map_err(db)?{
            if r.get::<String,_>("program_id")==program_id&&r.get::<String,_>("session_id")==session_id&&r.get::<String,_>("kind")=="tutor"&&r.get::<String,_>("payload_hash")==turn.payload_hash{tx.commit().await.map_err(db)?;return self.workspace(program_id).await;}return Err(invalid("Operation ID was already used with different practice data."));
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
        Self::record_operation(
            &mut tx,
            &turn.operation_id,
            program_id,
            session_id,
            "tutor",
            &turn.payload_hash,
            Some(&turn.id),
        )
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
        if let Some(r)=sqlx::query("SELECT payload_hash,program_id,session_id,kind FROM learning_practice_operations WHERE operation_id=?").bind(&req.operation_id).fetch_optional(&mut *tx).await.map_err(db)?{if r.get::<String,_>("payload_hash")==payload_hash&&r.get::<String,_>("program_id")==req.program_id&&r.get::<String,_>("session_id")==req.session_id&&r.get::<String,_>("kind")=="reveal_solution"{tx.commit().await.map_err(db)?;return self.workspace(&req.program_id).await;}return Err(invalid("Operation ID was already used with different practice data."));}
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
        Self::record_operation(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            &req.session_id,
            "reveal_solution",
            payload_hash,
            None,
        )
        .await?;
        tx.commit().await.map_err(db)?;
        self.workspace(&req.program_id).await
    }

    pub async fn submit(
        &self,
        req: &SubmitLearningPracticeAttemptRequestDto,
        write: SubmissionWrite,
        rubric: &[LearningPracticeCriterionDto],
    ) -> Result<LearningPracticeWorkspaceDto> {
        Self::validate_ids(&req.program_id, &req.session_id, Some(&req.operation_id))?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &req.program_id).await?;
        if let Some(r)=sqlx::query("SELECT payload_hash,program_id,session_id,kind FROM learning_practice_operations WHERE operation_id=?").bind(&req.operation_id).fetch_optional(&mut *tx).await.map_err(db)?{if r.get::<String,_>("payload_hash")==write.payload_hash&&r.get::<String,_>("program_id")==req.program_id&&r.get::<String,_>("session_id")==req.session_id&&r.get::<String,_>("kind")=="submit"{tx.commit().await.map_err(db)?;return self.workspace(&req.program_id).await;}return Err(invalid("Operation ID was already used with different practice data."));}
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
        Self::record_operation(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            &req.session_id,
            "submit",
            &write.payload_hash,
            None,
        )
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
        if let Some(r)=sqlx::query("SELECT payload_hash,program_id,session_id,kind FROM learning_practice_operations WHERE operation_id=?").bind(&req.operation_id).fetch_optional(&mut *tx).await.map_err(db)?{if r.get::<String,_>("payload_hash")==payload_hash&&r.get::<String,_>("program_id")==req.program_id&&r.get::<String,_>("session_id")==req.session_id&&r.get::<String,_>("kind")==kind{tx.commit().await.map_err(db)?;return self.workspace(&req.program_id).await;}return Err(invalid("Operation ID was already used with different practice data."));}
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
        Self::record_operation(
            &mut tx,
            &req.operation_id,
            &req.program_id,
            &req.session_id,
            kind,
            payload_hash,
            Some(&req.proposal_id),
        )
        .await?;
        tx.commit().await.map_err(db)?;
        self.workspace(&req.program_id).await
    }
}

fn dimension(value: &LearningPracticeEvidenceDimension) -> &'static str {
    match value {
        LearningPracticeEvidenceDimension::Recall => "recall",
        LearningPracticeEvidenceDimension::Explanation => "explanation",
        LearningPracticeEvidenceDimension::Application => "application",
        LearningPracticeEvidenceDimension::Transfer => "transfer",
    }
}

pub fn default_rubric() -> Vec<LearningPracticeCriterionDto> {
    [
        (
            "recall",
            "Recall",
            "Accurately retrieves the relevant idea or facts.",
        ),
        (
            "explanation",
            "Explanation",
            "Explains the reasoning in the learner's own words.",
        ),
        (
            "application",
            "Application",
            "Applies the idea to the stated task.",
        ),
        (
            "transfer",
            "Transfer",
            "Connects the idea to a changed or new context.",
        ),
    ]
    .into_iter()
    .map(|(id, title, description)| LearningPracticeCriterionDto {
        id: id.into(),
        dimension: match id {
            "recall" => LearningPracticeEvidenceDimension::Recall,
            "explanation" => LearningPracticeEvidenceDimension::Explanation,
            "application" => LearningPracticeEvidenceDimension::Application,
            _ => LearningPracticeEvidenceDimension::Transfer,
        },
        title: title.into(),
        description: description.into(),
        max_points: 3,
    })
    .collect()
}

pub fn validate_tutor_bounds(prompt: &str, response: &str) -> Result<()> {
    if prompt.trim().is_empty() || prompt.chars().count() > MAX_TUTOR_PROMPT_CHARS {
        return Err(invalid("Tutor prompt must be 1–2,000 characters."));
    }
    if response.trim().is_empty() || response.chars().count() > 6_000 {
        return Err(invalid("Tutor response must be 1–6,000 characters."));
    }
    Ok(())
}

fn task_kind(kind: &LearningPracticeTaskKind) -> &'static str {
    match kind {
        LearningPracticeTaskKind::Guided => "guided",
        LearningPracticeTaskKind::Independent => "independent",
    }
}
fn parse_task_kind(value: &str) -> Result<LearningPracticeTaskKind> {
    match value {
        "guided" => Ok(LearningPracticeTaskKind::Guided),
        "independent" => Ok(LearningPracticeTaskKind::Independent),
        _ => Err(AppError::Database("Invalid practice task kind".into())),
    }
}
