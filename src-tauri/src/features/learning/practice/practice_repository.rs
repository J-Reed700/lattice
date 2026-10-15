//! Durable attempt/session storage for the Grounded Practice Workbench.
use crate::features::learning::dto::*;
use crate::features::learning::{
    operations::Operation,
    persistence::{db, hash_text, now},
};
use lattice_core::shared::error::{AppError, Result};
use serde::{de::DeserializeOwned, Serialize};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

mod assistance;
mod reads;
mod sessions;
mod submissions;

const MAX_ARTIFACT_CHARS: usize = 24_000;
const MAX_TUTOR_PROMPT_CHARS: usize = 2_000;

fn invalid(message: impl Into<String>) -> AppError {
    AppError::InvalidInput(message.into())
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

fn practice_operation<'a>(
    operation_id: &'a str,
    program_id: &'a str,
    session_id: &'a str,
    kind: &'a str,
    payload_hash: &'a str,
) -> Operation<'a> {
    Operation {
        id: operation_id,
        scope: "practice",
        kind,
        program_id: Some(program_id),
        subject_id: Some(session_id),
        payload_hash,
        conflict: "Operation ID was already used with different practice data.",
    }
}
