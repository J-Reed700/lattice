//! Durable quizzes: the item bank, immutable forms, two-phase submissions,
//! grade contests, and the evidence they record.
use crate::features::learning::{assessment_engine as engine, dto::*};
use crate::features::learning::{
    operations::Operation,
    persistence::{db, now},
};
use lattice_core::shared::error::{AppError, Result};
use serde::{de::DeserializeOwned, Serialize};
use sqlx::{sqlite::SqliteRow, Row, Sqlite, SqliteConnection, SqlitePool, Transaction};
use std::collections::HashMap;

mod bank;
mod contests;
mod forms;
mod overview;
mod recall;
mod submission;

pub(crate) use bank::insert_bank_items_in;
pub use bank::BankItem;
pub use contests::{ContestInput, Regrade};
pub use submission::ItemGrade;

const MAX_RESPONSE_CHARS: usize = 24_000;
const STALE_EVIDENCE_MS: i64 = 90 * 24 * 60 * 60 * 1000;

fn invalid(s: impl Into<String>) -> AppError {
    AppError::InvalidInput(s.into())
}
fn uuid(s: &str, label: &str) -> Result<()> {
    uuid::Uuid::parse_str(s)
        .map(|_| ())
        .map_err(|_| invalid(format!("Invalid {label} ID")))
}
fn json<T: Serialize + ?Sized>(v: &T) -> Result<String> {
    serde_json::to_string(v).map_err(|e| AppError::Serialization(e.to_string()))
}
fn decode<T: DeserializeOwned>(v: &str) -> Result<T> {
    serde_json::from_str(v).map_err(|e| AppError::Serialization(e.to_string()))
}
fn parse_purpose(v: &str) -> Result<engine::LearningAssessmentPurpose> {
    match v {
        "practice" => Ok(engine::LearningAssessmentPurpose::Practice),
        "checkpoint" => Ok(engine::LearningAssessmentPurpose::Checkpoint),
        "module_test" => Ok(engine::LearningAssessmentPurpose::ModuleTest),
        "cumulative" => Ok(engine::LearningAssessmentPurpose::Cumulative),
        "transfer" => Ok(engine::LearningAssessmentPurpose::Transfer),
        _ => Err(AppError::Database("Invalid assessment purpose".into())),
    }
}
fn parse_timing(v: &str) -> Result<engine::LearningFeedbackTiming> {
    match v {
        "immediate" => Ok(engine::LearningFeedbackTiming::Immediate),
        "after_batch" => Ok(engine::LearningFeedbackTiming::AfterBatch),
        "after_submission" => Ok(engine::LearningFeedbackTiming::AfterSubmission),
        _ => Err(AppError::Database(
            "Invalid assessment feedback timing".into(),
        )),
    }
}
fn parse_grade(v: &str) -> Result<LearningAssessmentGradeStatus> {
    match v {
        "deterministic" => Ok(LearningAssessmentGradeStatus::Deterministic),
        "provisional" => Ok(LearningAssessmentGradeStatus::Provisional),
        "uncertain" => Ok(LearningAssessmentGradeStatus::Uncertain),
        "needs_review" => Ok(LearningAssessmentGradeStatus::NeedsReview),
        "pending" => Ok(LearningAssessmentGradeStatus::Pending),
        _ => Err(AppError::Database("Invalid assessment grade status".into())),
    }
}
fn grade_name(s: &LearningAssessmentGradeStatus) -> &'static str {
    match s {
        LearningAssessmentGradeStatus::Deterministic => "deterministic",
        LearningAssessmentGradeStatus::Provisional => "provisional",
        LearningAssessmentGradeStatus::Uncertain => "uncertain",
        LearningAssessmentGradeStatus::NeedsReview => "needs_review",
        LearningAssessmentGradeStatus::Pending => "pending",
    }
}
/// The submission's status: the least settled of its items'.
fn stronger_grade(
    a: LearningAssessmentGradeStatus,
    b: LearningAssessmentGradeStatus,
) -> LearningAssessmentGradeStatus {
    fn rank(s: &LearningAssessmentGradeStatus) -> u8 {
        match s {
            LearningAssessmentGradeStatus::Deterministic => 0,
            LearningAssessmentGradeStatus::Provisional => 1,
            LearningAssessmentGradeStatus::Uncertain => 2,
            LearningAssessmentGradeStatus::NeedsReview => 3,
            LearningAssessmentGradeStatus::Pending => 4,
        }
    }
    if rank(&a) >= rank(&b) {
        a
    } else {
        b
    }
}
fn parse_mode(v: &str) -> Result<LearningQuizMode> {
    match v {
        "graded" => Ok(LearningQuizMode::Graded),
        "practice" => Ok(LearningQuizMode::Practice),
        _ => Err(AppError::Database("Invalid assessment form mode".into())),
    }
}
fn mode_name(mode: LearningQuizMode) -> &'static str {
    match mode {
        LearningQuizMode::Graded => "graded",
        LearningQuizMode::Practice => "practice",
    }
}
fn scope_kind_name(kind: LearningQuizScopeKind) -> &'static str {
    match kind {
        LearningQuizScopeKind::Lesson => "lesson",
        LearningQuizScopeKind::Module => "module",
        LearningQuizScopeKind::Course => "course",
    }
}
/// A stored `scope_kind`/`scope_id` pair; None for an unscoped form.
fn parse_scope(kind: Option<String>, id: Option<String>) -> Result<Option<LearningQuizScopeDto>> {
    let kind = match kind.as_deref() {
        None => return Ok(None),
        Some("lesson") => LearningQuizScopeKind::Lesson,
        Some("module") => LearningQuizScopeKind::Module,
        Some("course") => LearningQuizScopeKind::Course,
        Some(_) => return Err(AppError::Database("Invalid quiz scope".into())),
    };
    Ok(Some(LearningQuizScopeDto { kind, id }))
}
fn parse_form_status(v: &str) -> Result<LearningAssessmentFormStatus> {
    match v {
        "active" => Ok(LearningAssessmentFormStatus::Active),
        "submitted" => Ok(LearningAssessmentFormStatus::Submitted),
        "interrupted" => Ok(LearningAssessmentFormStatus::Interrupted),
        _ => Err(AppError::Database("Invalid assessment form status".into())),
    }
}
fn parse_format(v: &str) -> Result<engine::LearningItemFormat> {
    crate::features::learning::quiz_blueprints::parse_format(v)
}
fn format_name(v: engine::LearningItemFormat) -> &'static str {
    crate::features::learning::quiz_blueprints::format_name(v)
}
/// Section 4.6: what an item's evidence shows.
fn dimension_for(
    format: engine::LearningItemFormat,
    difficulty: u8,
    purpose: engine::LearningAssessmentPurpose,
) -> &'static str {
    use engine::LearningItemFormat as F;
    if purpose == engine::LearningAssessmentPurpose::Transfer {
        return "transfer";
    }
    match format {
        F::Explanation => "explanation",
        F::Artifact => "application",
        F::ShortAnswer => "recall",
        _ if difficulty >= 4 => "application",
        _ => "recall",
    }
}
fn parse_dimension(s: &str) -> Result<LearningEvidenceDimension> {
    match s {
        "recall" => Ok(LearningEvidenceDimension::Recall),
        "explanation" => Ok(LearningEvidenceDimension::Explanation),
        "application" => Ok(LearningEvidenceDimension::Application),
        "transfer" => Ok(LearningEvidenceDimension::Transfer),
        _ => Err(AppError::Database("Invalid evidence dimension".into())),
    }
}
fn parse_evidence_kind(s: &str) -> Result<LearningEvidenceSourceKind> {
    match s {
        "practice" => Ok(LearningEvidenceSourceKind::Practice),
        "assessment" => Ok(LearningEvidenceSourceKind::Assessment),
        "recall" => Ok(LearningEvidenceSourceKind::Recall),
        "practical" => Ok(LearningEvidenceSourceKind::Practical),
        "simulation" => Ok(LearningEvidenceSourceKind::Simulation),
        "manual" => Ok(LearningEvidenceSourceKind::Manual),
        _ => Err(AppError::Database("Invalid evidence source kind".into())),
    }
}
fn parse_evidence_result(s: &str) -> Result<LearningEvidenceResult> {
    match s {
        "observed" => Ok(LearningEvidenceResult::Observed),
        "not_observed" => Ok(LearningEvidenceResult::NotObserved),
        "uncertain" => Ok(LearningEvidenceResult::Uncertain),
        "not_assessed" => Ok(LearningEvidenceResult::NotAssessed),
        _ => Err(AppError::Database("Invalid evidence result".into())),
    }
}
fn parse_reason(s: &str) -> Result<LearningFollowUpReasonCode> {
    match s {
        "missed_outcome" => Ok(LearningFollowUpReasonCode::MissedOutcome),
        "assisted_success" => Ok(LearningFollowUpReasonCode::AssistedSuccess),
        "low_transfer" => Ok(LearningFollowUpReasonCode::LowTransfer),
        "stale_evidence" => Ok(LearningFollowUpReasonCode::StaleEvidence),
        "uncertain_grade" => Ok(LearningFollowUpReasonCode::UncertainGrade),
        _ => Err(AppError::Database("Invalid follow-up reason".into())),
    }
}
fn parse_action(s: &str) -> Result<LearningFollowUpActionKind> {
    match s {
        "lesson" => Ok(LearningFollowUpActionKind::Lesson),
        "practice" => Ok(LearningFollowUpActionKind::Practice),
        "assessment" => Ok(LearningFollowUpActionKind::Assessment),
        "recall" => Ok(LearningFollowUpActionKind::Recall),
        "practical" => Ok(LearningFollowUpActionKind::Practical),
        _ => Err(AppError::Database("Invalid follow-up action".into())),
    }
}
fn parse_follow_status(s: &str) -> Result<LearningFollowUpStatus> {
    match s {
        "pending" => Ok(LearningFollowUpStatus::Pending),
        "accepted" => Ok(LearningFollowUpStatus::Accepted),
        "dismissed" => Ok(LearningFollowUpStatus::Dismissed),
        "completed" => Ok(LearningFollowUpStatus::Completed),
        _ => Err(AppError::Database("Invalid follow-up status".into())),
    }
}
fn assessment_operation<'a>(
    operation_id: &'a str,
    program_id: &'a str,
    subject_id: Option<&'a str>,
    kind: &'a str,
    payload_hash: &'a str,
    conflict: &'static str,
) -> Operation<'a> {
    Operation {
        id: operation_id,
        scope: "assessment",
        kind,
        program_id: Some(program_id),
        subject_id,
        payload_hash,
        conflict,
    }
}

/// A bank item read back with what the bank keeps beside it.
#[derive(Debug, Clone)]
pub(crate) struct StoredCandidate {
    pub candidate: engine::LearningAssessmentCandidate,
    pub artifact_kind: Option<String>,
    pub source_quote: String,
}

/// Columns a candidate row must carry: `c.*` with the key's `key_json`,
/// `explanation` and `distractor_rationales_json`.
const CANDIDATE_COLUMNS: &str = "c.*,k.key_json,k.explanation,k.distractor_rationales_json";

fn candidate_from_row(row: &SqliteRow, outcome_ids: Vec<String>) -> Result<StoredCandidate> {
    let primary: String = row.get("outcome_id");
    let mut outcomes = vec![primary];
    for outcome in outcome_ids {
        if !outcomes.contains(&outcome) {
            outcomes.push(outcome);
        }
    }
    Ok(StoredCandidate {
        candidate: engine::LearningAssessmentCandidate {
            id: row.get("id"),
            outcome_ids: outcomes,
            format: parse_format(row.get("format"))?,
            difficulty: row.get::<i64, _>("difficulty").clamp(1, 5) as u8,
            prompt: row.get("prompt"),
            options: decode(row.get("options_json"))?,
            left: decode(row.get("left_items_json"))?,
            source_version_ids: decode(row.get("source_version_ids_json"))?,
            rubric: decode(row.get("rubric_json"))?,
            key: decode(row.get("key_json"))?,
            explanation: row.get("explanation"),
            distractor_rationales: decode(row.get("distractor_rationales_json"))?,
        },
        artifact_kind: row.get("artifact_kind"),
        source_quote: row.get("source_quote"),
    })
}

/// Every outcome each candidate of a program addresses.
async fn candidate_outcomes_in(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<HashMap<String, Vec<String>>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for row in sqlx::query("SELECT co.candidate_id,co.outcome_id FROM learning_assessment_candidate_outcomes co JOIN learning_assessment_candidates c ON c.id=co.candidate_id WHERE c.program_id=? ORDER BY co.outcome_id")
        .bind(program_id)
        .fetch_all(&mut *connection)
        .await
        .map_err(db)?
    {
        map.entry(row.get("candidate_id"))
            .or_default()
            .push(row.get("outcome_id"));
    }
    Ok(map)
}

/// One candidate of the program, whatever its review status (a form keeps
/// the items it was drawn with).
async fn candidate_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    candidate_id: &str,
) -> Result<StoredCandidate> {
    let row = sqlx::query(&format!("SELECT {CANDIDATE_COLUMNS} FROM learning_assessment_candidates c JOIN learning_assessment_answer_keys k ON k.candidate_id=c.id WHERE c.id=? AND c.program_id=?"))
        .bind(candidate_id)
        .bind(program_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::InvalidState("This quiz question is no longer available.".into()))?;
    let outcomes = sqlx::query_scalar::<_, String>(
        "SELECT outcome_id FROM learning_assessment_candidate_outcomes WHERE candidate_id=? ORDER BY outcome_id",
    )
    .bind(candidate_id)
    .fetch_all(&mut *connection)
    .await
    .map_err(db)?;
    candidate_from_row(&row, outcomes)
}

impl LearningAssessmentRepository {
    /// One bank item with its key, for the grading job.
    pub(crate) async fn stored_candidate_in(
        connection: &mut SqliteConnection,
        program_id: &str,
        candidate_id: &str,
    ) -> Result<StoredCandidate> {
        candidate_in(connection, program_id, candidate_id).await
    }
}

#[derive(Clone)]
pub struct LearningAssessmentRepository {
    pool: SqlitePool,
}

impl LearningAssessmentRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    async fn active_program(tx: &mut Transaction<'_, Sqlite>, program_id: &str) -> Result<()> {
        let status: Option<String> =
            sqlx::query_scalar("SELECT status FROM learning_programs WHERE id=?")
                .bind(program_id)
                .fetch_optional(&mut **tx)
                .await
                .map_err(db)?;
        if status.as_deref() != Some("active") {
            return Err(invalid("Quizzes need an active course."));
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
    /// Module outcomes, one per line of each module's outcome list.
    pub(crate) async fn ensure_outcomes(
        connection: &mut SqliteConnection,
        program_id: &str,
    ) -> Result<()> {
        for module in sqlx::query("SELECT id,title,outcomes_json FROM learning_modules WHERE program_id=? ORDER BY ordinal").bind(program_id).fetch_all(&mut *connection).await.map_err(db)? {
            let module_id: String = module.get("id");
            let module_title: String = module.get("title");
            let outcomes: Vec<String> = decode(module.get("outcomes_json"))?;
            for (ordinal, text) in outcomes.into_iter().enumerate() {
                let title = text.trim();
                if title.is_empty() {
                    continue;
                }
                let existing: Option<String> = sqlx::query_scalar("SELECT id FROM learning_outcome_definitions WHERE program_id=? AND module_id=? AND ordinal=? AND lesson_id IS NULL").bind(program_id).bind(&module_id).bind(ordinal as i64).fetch_optional(&mut *connection).await.map_err(db)?;
                if existing.is_none() {
                    sqlx::query("INSERT INTO learning_outcome_definitions(id,program_id,module_id,lesson_id,title,description,ordinal,created_at) VALUES(?,?,?,NULL,?,?,?,?)").bind(uuid::Uuid::new_v4().to_string()).bind(program_id).bind(&module_id).bind(title).bind(format!("Outcome for {module_title}: {title}")).bind(ordinal as i64).bind(now()).execute(&mut *connection).await.map_err(db)?;
                }
            }
        }
        Ok(())
    }
}

/// A follow-up recommendation, unless one for the same outcome and reason is
/// already waiting.
#[allow(clippy::too_many_arguments)]
async fn insert_followup(
    connection: &mut SqliteConnection,
    program_id: &str,
    outcome_id: Option<&str>,
    reason: &str,
    explanation: &str,
    action: &str,
    action_ref: Option<&str>,
    evidence_ids: &[String],
) -> Result<()> {
    let existing:Option<String>=sqlx::query_scalar("SELECT id FROM learning_follow_up_recommendations WHERE program_id=? AND outcome_id IS ? AND reason_code=? AND status='pending' ORDER BY created_at DESC LIMIT 1").bind(program_id).bind(outcome_id).bind(reason).fetch_optional(&mut *connection).await.map_err(db)?;
    if existing.is_some() {
        return Ok(());
    }
    sqlx::query("INSERT INTO learning_follow_up_recommendations(id,program_id,outcome_id,reason_code,explanation,action_kind,action_ref,status,evidence_event_ids_json,created_at) VALUES(?,?,?,?,?,?,?,'pending',?,?)").bind(uuid::Uuid::new_v4().to_string()).bind(program_id).bind(outcome_id).bind(reason).bind(explanation).bind(action).bind(action_ref).bind(json(evidence_ids)?).bind(now()).execute(&mut *connection).await.map_err(db)?;
    Ok(())
}
