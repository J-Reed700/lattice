//! Durable versioned assessment blueprints, immutable forms, submissions, and evidence.
use crate::features::learning::{assessment_engine as engine, dto::*};
use crate::features::learning::{
    operations::Operation,
    persistence::{db, now},
};
use lattice_core::shared::error::{AppError, Result};
use serde::{de::DeserializeOwned, Serialize};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use std::collections::HashSet;

mod blueprints;
mod forms;
mod submission;
mod workspace;

const MAX_FORM_ITEMS: usize = 64;
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
fn format_name(v: engine::LearningItemFormat) -> &'static str {
    match v {
        engine::LearningItemFormat::SingleChoice => "single_choice",
        engine::LearningItemFormat::MultiSelect => "multi_select",
        engine::LearningItemFormat::TrueFalse => "true_false",
        engine::LearningItemFormat::ShortAnswer => "short_answer",
        engine::LearningItemFormat::Cloze => "cloze",
        engine::LearningItemFormat::Matching => "matching",
        engine::LearningItemFormat::Ordering => "ordering",
        engine::LearningItemFormat::Explanation => "explanation",
        engine::LearningItemFormat::Artifact => "artifact",
    }
}
fn parse_format(v: &str) -> Result<engine::LearningItemFormat> {
    match v {
        "single_choice" => Ok(engine::LearningItemFormat::SingleChoice),
        "multi_select" => Ok(engine::LearningItemFormat::MultiSelect),
        "true_false" => Ok(engine::LearningItemFormat::TrueFalse),
        "short_answer" => Ok(engine::LearningItemFormat::ShortAnswer),
        "cloze" => Ok(engine::LearningItemFormat::Cloze),
        "matching" => Ok(engine::LearningItemFormat::Matching),
        "explanation" => Ok(engine::LearningItemFormat::Explanation),
        "ordering" => Ok(engine::LearningItemFormat::Ordering),
        "artifact" => Ok(engine::LearningItemFormat::Artifact),
        _ => Err(AppError::Database("Invalid assessment item format".into())),
    }
}
fn purpose_name(v: engine::LearningAssessmentPurpose) -> &'static str {
    match v {
        engine::LearningAssessmentPurpose::Practice => "practice",
        engine::LearningAssessmentPurpose::Checkpoint => "checkpoint",
        engine::LearningAssessmentPurpose::ModuleTest => "module_test",
        engine::LearningAssessmentPurpose::Cumulative => "cumulative",
        engine::LearningAssessmentPurpose::Transfer => "transfer",
    }
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
fn timing_name(v: engine::LearningFeedbackTiming) -> &'static str {
    match v {
        engine::LearningFeedbackTiming::Immediate => "immediate",
        engine::LearningFeedbackTiming::AfterBatch => "after_batch",
        engine::LearningFeedbackTiming::AfterSubmission => "after_submission",
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
fn parse_mode(v: &str) -> Result<LearningQuizMode> {
    match v {
        "graded" => Ok(LearningQuizMode::Graded),
        "practice" => Ok(LearningQuizMode::Practice),
        _ => Err(AppError::Database("Invalid assessment form mode".into())),
    }
}
/// A stored `scope_kind`/`scope_id` pair; None for an unscoped legacy form.
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
fn parse_blueprint_status(v: &str) -> Result<LearningBlueprintStatus> {
    match v {
        "draft" => Ok(LearningBlueprintStatus::Draft),
        "accepted" => Ok(LearningBlueprintStatus::Accepted),
        "retired" => Ok(LearningBlueprintStatus::Retired),
        _ => Err(AppError::Database("Invalid blueprint status".into())),
    }
}
fn rubric_valid(items: &[LearningRubricCriterion]) -> Result<()> {
    if items.is_empty() || items.len() > 16 {
        return Err(invalid("Assessment rubric needs 1–16 criteria."));
    }
    let mut ids = HashSet::new();
    for c in items {
        uuid(&c.id, "rubric criterion")?;
        if c.title.trim().is_empty()
            || c.title.chars().count() > 160
            || c.description.trim().is_empty()
            || c.description.chars().count() > 1200
            || c.max_points == 0
            || c.max_points > 100
            || !ids.insert(&c.id)
        {
            return Err(invalid(
                "Assessment rubric criteria must be unique and bounded.",
            ));
        }
    }
    Ok(())
}

#[derive(Clone)]
pub struct LearningAssessmentRepository {
    pool: SqlitePool,
}

#[derive(Debug, Clone)]
struct StoredCandidate {
    candidate: engine::LearningAssessmentCandidate,
    artifact_kind: Option<String>,
}

impl LearningAssessmentRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    async fn active_program(tx: &mut Transaction<'_, Sqlite>, program_id: &str) -> Result<()> {
        let status: Option<String> =
            sqlx::query_scalar("SELECT status FROM learning_programs WHERE id=?")
                .bind(program_id)
                .fetch_optional(&mut **tx)
                .await
                .map_err(db)?;
        if status.as_deref() != Some("active") {
            return Err(invalid(
                "Assessment changes require an active learning program.",
            ));
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
    async fn ensure_outcomes(tx: &mut Transaction<'_, Sqlite>, program_id: &str) -> Result<()> {
        for module in sqlx::query("SELECT id,title,outcomes_json FROM learning_modules WHERE program_id=? ORDER BY ordinal").bind(program_id).fetch_all(&mut **tx).await.map_err(db)? {
            let module_id:String=module.get("id");let module_title:String=module.get("title");let outcomes:Vec<String>=decode(module.get("outcomes_json"))?;
            for (ordinal,text) in outcomes.into_iter().enumerate(){let title=text.trim();if title.is_empty(){continue;}let existing:Option<String>=sqlx::query_scalar("SELECT id FROM learning_outcome_definitions WHERE program_id=? AND module_id=? AND ordinal=?").bind(program_id).bind(&module_id).bind(ordinal as i64).fetch_optional(&mut **tx).await.map_err(db)?;if existing.is_none(){sqlx::query("INSERT INTO learning_outcome_definitions(id,program_id,module_id,lesson_id,title,description,ordinal,created_at) VALUES(?,?,?,NULL,?,?,?,?)").bind(uuid::Uuid::new_v4().to_string()).bind(program_id).bind(&module_id).bind(title).bind(format!("Outcome for {module_title}: {title}" )).bind(ordinal as i64).bind(now()).execute(&mut **tx).await.map_err(db)?;}}
        }
        Ok(())
    }
    async fn verify_sources(
        tx: &mut Transaction<'_, Sqlite>,
        program_id: &str,
        ids: &[String],
    ) -> Result<()> {
        if ids.len() > 32 {
            return Err(invalid(
                "Assessments support at most 32 frozen source versions.",
            ));
        }
        let mut seen = HashSet::new();
        for id in ids {
            uuid(id, "source version")?;
            if !seen.insert(id) {
                return Err(invalid("Source version IDs must be unique."));
            }
            let exists: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM learning_source_versions WHERE program_id=? AND id=?",
            )
            .bind(program_id)
            .bind(id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(db)?;
            if exists.is_none() {
                return Err(invalid(
                    "Assessment source version is not owned by this program.",
                ));
            }
        }
        Ok(())
    }
}

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
fn grade_name(s: &LearningAssessmentGradeStatus) -> &'static str {
    match s {
        LearningAssessmentGradeStatus::Deterministic => "deterministic",
        LearningAssessmentGradeStatus::Provisional => "provisional",
        LearningAssessmentGradeStatus::Uncertain => "uncertain",
        LearningAssessmentGradeStatus::NeedsReview => "needs_review",
        LearningAssessmentGradeStatus::Pending => "pending",
    }
}
fn dimension_for(
    format: engine::LearningItemFormat,
    purpose: engine::LearningAssessmentPurpose,
) -> &'static str {
    if purpose == engine::LearningAssessmentPurpose::Transfer {
        "transfer"
    } else {
        match format {
            engine::LearningItemFormat::SingleChoice
            | engine::LearningItemFormat::MultiSelect
            | engine::LearningItemFormat::TrueFalse
            | engine::LearningItemFormat::ShortAnswer
            | engine::LearningItemFormat::Cloze
            | engine::LearningItemFormat::Matching
            | engine::LearningItemFormat::Ordering => "recall",
            engine::LearningItemFormat::Explanation => "explanation",
            engine::LearningItemFormat::Artifact => "application",
        }
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
    form_id: Option<&'a str>,
    kind: &'a str,
    payload_hash: &'a str,
    conflict: &'static str,
) -> Operation<'a> {
    Operation {
        id: operation_id,
        scope: "assessment",
        kind,
        program_id: Some(program_id),
        subject_id: form_id,
        payload_hash,
        conflict,
    }
}
async fn item_ordinal(
    tx: &mut Transaction<'_, Sqlite>,
    form_id: &str,
    candidate_id: &str,
) -> Result<i64> {
    sqlx::query_scalar(
        "SELECT ordinal FROM learning_assessment_form_items WHERE form_id=? AND candidate_id=?",
    )
    .bind(form_id)
    .bind(candidate_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db)?
    .ok_or_else(|| invalid("Response item does not belong to this form."))
}
fn validate_response(
    r: &engine::LearningAssessmentResponse,
    format: engine::LearningItemFormat,
    options: &[String],
) -> Result<()> {
    if r.text
        .as_ref()
        .is_some_and(|s| s.chars().count() > MAX_RESPONSE_CHARS)
        || r.ordered_values.len() > 32
        || r.artifact_json
            .as_ref()
            .is_some_and(|v| v.to_string().len() > MAX_RESPONSE_CHARS)
    {
        return Err(invalid("Assessment response exceeds its size limit."));
    }
    match format {
        engine::LearningItemFormat::SingleChoice => {
            if r.selected_index.is_none_or(|i| i >= options.len())
                || r.text.is_some()
                || !r.ordered_values.is_empty()
                || r.artifact_json.is_some()
            {
                return Err(invalid("Choose one available option for this item."));
            }
        }
        engine::LearningItemFormat::ShortAnswer | engine::LearningItemFormat::Explanation => {
            if r.text.as_deref().is_none_or(|s| s.trim().is_empty())
                || r.selected_index.is_some()
                || !r.ordered_values.is_empty()
                || r.artifact_json.is_some()
            {
                return Err(invalid("Provide a written response for this item."));
            }
        }
        engine::LearningItemFormat::Ordering => {
            if r.ordered_values.len() != options.len()
                || r.ordered_values.iter().collect::<HashSet<_>>().len() != options.len()
                || r.ordered_values.iter().collect::<HashSet<_>>()
                    != options.iter().collect::<HashSet<_>>()
                || r.selected_index.is_some()
                || r.text.is_some()
                || r.artifact_json.is_some()
            {
                return Err(invalid("Order every available value exactly once."));
            }
        }
        engine::LearningItemFormat::Artifact => {
            if r.selected_index.is_some()
                || !r.ordered_values.is_empty()
                || r.artifact_json.as_ref().is_some_and(|v| !v.is_object())
                || (r.artifact_json.is_none()
                    && r.text.as_deref().is_none_or(|s| s.trim().is_empty()))
            {
                return Err(invalid("Save an artifact or written response."));
            }
        }
        engine::LearningItemFormat::MultiSelect
        | engine::LearningItemFormat::TrueFalse
        | engine::LearningItemFormat::Cloze
        | engine::LearningItemFormat::Matching => {
            return Err(AppError::InvalidState("not built".into()));
        }
    }
    Ok(())
}
async fn insert_followup(
    tx: &mut Transaction<'_, Sqlite>,
    program_id: &str,
    outcome_id: Option<&str>,
    reason: &str,
    explanation: &str,
    action: &str,
    action_ref: Option<&str>,
    evidence_ids: &[String],
) -> Result<()> {
    let existing:Option<String>=sqlx::query_scalar("SELECT id FROM learning_follow_up_recommendations WHERE program_id=? AND outcome_id IS ? AND reason_code=? AND status='pending' ORDER BY created_at DESC LIMIT 1").bind(program_id).bind(outcome_id).bind(reason).fetch_optional(&mut **tx).await.map_err(db)?;
    if existing.is_some() {
        return Ok(());
    }
    sqlx::query("INSERT INTO learning_follow_up_recommendations(id,program_id,outcome_id,reason_code,explanation,action_kind,action_ref,status,evidence_event_ids_json,created_at) VALUES(?,?,?,?,?,?,?,'pending',?,?)").bind(uuid::Uuid::new_v4().to_string()).bind(program_id).bind(outcome_id).bind(reason).bind(explanation).bind(action).bind(action_ref).bind(json(evidence_ids)?).bind(now()).execute(&mut **tx).await.map_err(db)?;
    Ok(())
}
