//! Durable versioned assessment blueprints, immutable forms, submissions, and evidence.
use crate::features::learning::{assessment_engine as engine, dto::*};
use crate::features::learning::{
    operations::Operation,
    persistence::{self, db, now},
};
use crate::shared::error::{AppError, Result};
use serde::{de::DeserializeOwned, Serialize};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use std::collections::{HashMap, HashSet};

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
        engine::LearningItemFormat::MultipleChoice => "multiple_choice",
        engine::LearningItemFormat::ShortAnswer => "short_answer",
        engine::LearningItemFormat::Explanation => "explanation",
        engine::LearningItemFormat::Ordering => "ordering",
        engine::LearningItemFormat::Artifact => "artifact",
    }
}
fn parse_format(v: &str) -> Result<engine::LearningItemFormat> {
    match v {
        "multiple_choice" => Ok(engine::LearningItemFormat::MultipleChoice),
        "short_answer" => Ok(engine::LearningItemFormat::ShortAnswer),
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
        _ => Err(AppError::Database("Invalid assessment grade status".into())),
    }
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

    pub async fn workspace(&self, program_id: &str) -> Result<LearningAssessmentWorkspaceDto> {
        uuid(program_id, "program")?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM learning_programs WHERE id=?")
            .bind(program_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
        if exists.is_none() {
            return Err(AppError::NotFound("Learning program not found".into()));
        }
        Self::writer_lock(&mut tx, program_id).await?;
        Self::ensure_outcomes(&mut tx, program_id).await?;
        tx.commit().await.map_err(db)?;
        let outcomes = self.outcomes(program_id).await?;
        let blueprints = self.blueprints(program_id).await?;
        let forms = self.form_summaries(program_id).await?;
        let evidence = self.evidence(program_id, None).await?;
        let follow_ups = self.follow_ups(program_id).await?;
        Ok(LearningAssessmentWorkspaceDto {
            program_id: program_id.into(),
            outcomes,
            blueprints,
            forms,
            evidence,
            follow_ups,
        })
    }

    async fn outcomes(&self, program_id: &str) -> Result<Vec<LearningOutcomeDefinitionDto>> {
        let rows=sqlx::query("SELECT * FROM learning_outcome_definitions WHERE program_id=? ORDER BY module_id,ordinal,id").bind(program_id).fetch_all(&self.pool).await.map_err(db)?;
        rows.into_iter()
            .map(|r| {
                Ok(LearningOutcomeDefinitionDto {
                    id: r.get("id"),
                    module_id: r.get("module_id"),
                    lesson_id: r.get("lesson_id"),
                    title: r.get("title"),
                    description: r.get("description"),
                    ordinal: r.get("ordinal"),
                    created_at: r.get("created_at"),
                })
            })
            .collect()
    }

    async fn blueprints(&self, program_id: &str) -> Result<Vec<LearningAssessmentBlueprintDto>> {
        let rows=sqlx::query("SELECT * FROM learning_assessment_blueprints WHERE program_id=? ORDER BY created_at DESC,id,revision DESC").bind(program_id).fetch_all(&self.pool).await.map_err(db)?;
        let mut output = Vec::new();
        for r in rows {
            let id: String = r.get("id");
            let revision: i64 = r.get("revision");
            let slotrows=sqlx::query("SELECT outcome_id,format,difficulty,difficulty_max FROM learning_assessment_blueprint_slots WHERE blueprint_id=? AND blueprint_revision=? ORDER BY ordinal").bind(&id).bind(revision).fetch_all(&self.pool).await.map_err(db)?;
            let mut group: HashMap<(String, engine::LearningItemFormat), Vec<(u8, u8)>> =
                HashMap::new();
            for s in slotrows {
                let k = (s.get("outcome_id"), parse_format(s.get("format"))?);
                group.entry(k).or_default().push((
                    s.get::<i64, _>("difficulty").clamp(1, 5) as u8,
                    s.get::<i64, _>("difficulty_max").clamp(1, 5) as u8,
                ));
            }
            let mut requirements = Vec::new();
            for ((outcome_id, format), diffs) in group {
                let min = diffs.iter().map(|x| x.0).min().unwrap_or(1);
                let max = diffs.iter().map(|x| x.1).max().unwrap_or(min);
                requirements.push(engine::LearningBlueprintRequirement {
                    outcome_id,
                    format,
                    count: diffs.len(),
                    difficulty_min: min,
                    difficulty_max: max,
                });
            }
            requirements.sort_by(|a, b| {
                a.outcome_id
                    .cmp(&b.outcome_id)
                    .then_with(|| format_name(a.format).cmp(format_name(b.format)))
            });
            output.push(LearningAssessmentBlueprintDto {
                id,
                revision,
                predecessor_revision: r.get("predecessor_revision"),
                purpose: parse_purpose(r.get("purpose"))?,
                title: r.get("title"),
                instructions: r.get("instructions"),
                expected_minutes: r.get::<i64, _>("expected_minutes").clamp(1, 480) as u32,
                allowed_aids: decode(r.get("allowed_aids_json"))?,
                passing_score: r.get("passing_score"),
                feedback_timing: parse_timing(r.get("feedback_timing"))?,
                rubric: decode(r.get("rubric_json"))?,
                source_version_ids: decode(r.get("source_version_ids_json"))?,
                requirements,
                status: parse_blueprint_status(r.get("status"))?,
                change_reason: r.get("change_reason"),
                created_at: r.get("created_at"),
            });
        }
        Ok(output)
    }

    async fn form_summaries(
        &self,
        program_id: &str,
    ) -> Result<Vec<LearningAssessmentFormSummaryDto>> {
        let rows=sqlx::query("SELECT f.*,s.score,s.grade_status FROM learning_assessment_forms f LEFT JOIN learning_assessment_submissions s ON s.form_id=f.id WHERE f.program_id=? ORDER BY f.created_at DESC,f.id").bind(program_id).fetch_all(&self.pool).await.map_err(db)?;
        rows.into_iter()
            .map(|r| {
                Ok(LearningAssessmentFormSummaryDto {
                    id: r.get("id"),
                    blueprint_id: r.get("blueprint_id"),
                    blueprint_revision: r.get("blueprint_revision"),
                    title: r.get("title"),
                    purpose: parse_purpose(r.get("purpose"))?,
                    status: parse_form_status(r.get("status"))?,
                    revision: r.get("revision"),
                    retake_of_form_id: r.get("retake_of_form_id"),
                    score: r.get("score"),
                    grade_status: r
                        .get::<Option<String>, _>("grade_status")
                        .map(|s| parse_grade(&s))
                        .transpose()?,
                    created_at: r.get("created_at"),
                    submitted_at: r.get("submitted_at"),
                })
            })
            .collect()
    }

    async fn evidence(
        &self,
        program_id: &str,
        outcome_id: Option<&str>,
    ) -> Result<Vec<LearningEvidenceEventDto>> {
        let rows = if let Some(outcome) = outcome_id {
            sqlx::query("SELECT * FROM learning_evidence_events WHERE program_id=? AND outcome_id=? ORDER BY observed_at DESC,id LIMIT 500").bind(program_id).bind(outcome).fetch_all(&self.pool).await.map_err(db)?
        } else {
            sqlx::query("SELECT * FROM learning_evidence_events WHERE program_id=? ORDER BY observed_at DESC,id LIMIT 1000").bind(program_id).fetch_all(&self.pool).await.map_err(db)?
        };
        rows.into_iter()
            .map(|r| {
                Ok(LearningEvidenceEventDto {
                    id: r.get("id"),
                    outcome_id: r.get("outcome_id"),
                    source_kind: parse_evidence_kind(r.get("source_kind"))?,
                    source_id: r.get("source_id"),
                    dimension: parse_dimension(r.get("dimension"))?,
                    result: parse_evidence_result(r.get("result"))?,
                    score: r.get("score"),
                    observation: r.get("observation"),
                    evidence_quote: r.get("evidence_quote"),
                    assistance: decode(r.get("assistance_json"))?,
                    observed_at: r.get("observed_at"),
                })
            })
            .collect()
    }

    async fn follow_ups(&self, program_id: &str) -> Result<Vec<LearningFollowUpRecommendationDto>> {
        let rows=sqlx::query("SELECT * FROM learning_follow_up_recommendations WHERE program_id=? ORDER BY created_at DESC,id LIMIT 500").bind(program_id).fetch_all(&self.pool).await.map_err(db)?;
        rows.into_iter()
            .map(|r| {
                Ok(LearningFollowUpRecommendationDto {
                    id: r.get("id"),
                    outcome_id: r.get("outcome_id"),
                    reason_code: parse_reason(r.get("reason_code"))?,
                    explanation: r.get("explanation"),
                    action_kind: parse_action(r.get("action_kind"))?,
                    action_ref: r.get("action_ref"),
                    status: parse_follow_status(r.get("status"))?,
                    evidence_event_ids: decode(r.get("evidence_event_ids_json"))?,
                    created_at: r.get("created_at"),
                    decided_at: r.get("decided_at"),
                })
            })
            .collect()
    }

    pub async fn preflight_blueprint(
        &self,
        req: &CreateLearningAssessmentBlueprintRequestDto,
        hash: &str,
    ) -> Result<Option<LearningAssessmentWorkspaceDto>> {
        uuid(&req.program_id, "program")?;
        uuid(&req.operation_id, "operation")?;
        uuid(&req.blueprint_id, "blueprint")?;
        if assessment_operation(
            &req.operation_id,
            &req.program_id,
            None,
            "create_blueprint",
            hash,
            "Operation ID was already used for different assessment data.",
        )
        .seen(&self.pool)
        .await?
        {
            return Ok(Some(self.workspace(&req.program_id).await?));
        }
        if req.revision < 1 || req.revision > 10_000 {
            return Err(invalid("Invalid blueprint revision."));
        }
        if req.title.trim().is_empty()
            || req.title.chars().count() > 160
            || req.instructions.trim().is_empty()
            || req.instructions.chars().count() > 4_000
            || req.change_reason.trim().is_empty()
            || req.change_reason.chars().count() > 800
            || !req.passing_score.is_finite()
            || !(0.0..=1.0).contains(&req.passing_score)
        {
            return Err(invalid(
                "Assessment blueprint metadata is outside its allowed bounds.",
            ));
        }
        rubric_valid(&req.rubric)?;
        let engine_blueprint = engine::LearningAssessmentBlueprint {
            id: req.blueprint_id.clone(),
            purpose: req.purpose,
            title: req.title.trim().into(),
            instructions: req.instructions.trim().into(),
            expected_minutes: req.expected_minutes,
            allowed_aids: req.allowed_aids.clone(),
            pass_points: (req.passing_score * 100.0).round() as u32,
            feedback_timing: req.feedback_timing,
            requirements: req.requirements.clone(),
        };
        engine::validate_blueprint(&engine_blueprint)?;
        if req.source_version_ids.len() > 32 {
            return Err(invalid("Select no more than 32 frozen source versions."));
        }
        if req.source_version_ids.is_empty() {
            let has_sources: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM learning_sources WHERE program_id=?)",
            )
            .bind(&req.program_id)
            .fetch_one(&self.pool)
            .await
            .map_err(db)?;
            if has_sources {
                return Err(invalid(
                    "Select frozen source versions for this source-backed assessment.",
                ));
            }
        }
        let status: Option<String> =
            sqlx::query_scalar("SELECT status FROM learning_programs WHERE id=?")
                .bind(&req.program_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(db)?;
        if status.as_deref() != Some("active") {
            return Err(invalid(
                "Assessment changes require an active learning program.",
            ));
        }
        // Outcome rows are derived from the accepted program/module data.
        // Seed them before checking ownership so invalid requests still fail
        // before any model call while using the same canonical outcome IDs.
        self.workspace(&req.program_id).await?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::active_program(&mut tx, &req.program_id).await?;
        let revision_exists: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM learning_assessment_blueprints WHERE program_id=? AND id=? AND revision=?",
        )
        .bind(&req.program_id)
        .bind(&req.blueprint_id)
        .bind(req.revision)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        if revision_exists.is_some() {
            return Err(invalid("Blueprint revision already exists."));
        }
        Self::verify_sources(&mut tx, &req.program_id, &req.source_version_ids).await?;
        for requirement in &req.requirements {
            let owned: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM learning_outcome_definitions WHERE id=? AND program_id=?",
            )
            .bind(&requirement.outcome_id)
            .bind(&req.program_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
            if owned.is_none() {
                return Err(invalid(
                    "Blueprint outcome does not belong to this program.",
                ));
            }
        }
        if req.revision == 1 {
            if req.predecessor_revision.is_some() {
                return Err(invalid(
                    "First blueprint version cannot have a predecessor.",
                ));
            }
        } else {
            if req.predecessor_revision != Some(req.revision - 1) {
                return Err(invalid(
                    "Blueprint revisions must advance from the prior revision.",
                ));
            }
            let previous: Option<String> = sqlx::query_scalar(
                "SELECT status FROM learning_assessment_blueprints WHERE program_id=? AND id=? AND revision=?",
            )
            .bind(&req.program_id)
            .bind(&req.blueprint_id)
            .bind(req.revision - 1)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
            if previous.as_deref() != Some("accepted") {
                return Err(invalid("Previous accepted blueprint revision not found."));
            }
        }
        tx.commit().await.map_err(db)?;
        Ok(None)
    }

    pub async fn create_blueprint(
        &self,
        req: &CreateLearningAssessmentBlueprintRequestDto,
        candidates: &[LearningAssessmentCandidateWriteDto],
        author_model: &str,
        hash: &str,
    ) -> Result<LearningAssessmentWorkspaceDto> {
        for (value, label) in [
            (&req.program_id, "program"),
            (&req.blueprint_id, "blueprint"),
            (&req.operation_id, "operation"),
        ] {
            uuid(value, label)?;
        }
        if req.revision < 1 || req.revision > 10_000 {
            return Err(invalid("Invalid blueprint revision."));
        }
        if req.title.trim().is_empty()
            || req.title.chars().count() > 160
            || req.instructions.trim().is_empty()
            || req.instructions.chars().count() > 4_000
            || req.change_reason.trim().is_empty()
            || req.change_reason.chars().count() > 800
            || !req.passing_score.is_finite()
            || !(0.0..=1.0).contains(&req.passing_score)
        {
            return Err(invalid(
                "Assessment blueprint metadata is outside its allowed bounds.",
            ));
        }
        rubric_valid(&req.rubric)?;
        if candidates.is_empty() || candidates.len() > 256 {
            return Err(invalid(
                "Blueprint must include 1–256 validated item candidates.",
            ));
        }
        let engine_blueprint = engine::LearningAssessmentBlueprint {
            id: req.blueprint_id.clone(),
            purpose: req.purpose,
            title: req.title.trim().into(),
            instructions: req.instructions.trim().into(),
            expected_minutes: req.expected_minutes,
            allowed_aids: req.allowed_aids.clone(),
            pass_points: (req.passing_score * 100.0).round() as u32,
            feedback_timing: req.feedback_timing,
            requirements: req.requirements.clone(),
        };
        engine::validate_blueprint(&engine_blueprint)?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &req.program_id).await?;
        let operation = assessment_operation(
            &req.operation_id,
            &req.program_id,
            None,
            "create_blueprint",
            hash,
            "Operation ID was already used for different assessment data.",
        );
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return self.workspace(&req.program_id).await;
        }
        Self::active_program(&mut tx, &req.program_id).await?;
        Self::ensure_outcomes(&mut tx, &req.program_id).await?;
        if req.source_version_ids.is_empty() {
            let has_sources: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM learning_sources WHERE program_id=?)",
            )
            .bind(&req.program_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
            if has_sources {
                return Err(invalid(
                    "Select frozen source versions for this source-backed assessment.",
                ));
            }
        }
        Self::verify_sources(&mut tx, &req.program_id, &req.source_version_ids).await?;
        if req.revision == 1 {
            if req.predecessor_revision.is_some() {
                return Err(invalid(
                    "First blueprint version cannot have a predecessor.",
                ));
            }
        } else {
            if req.predecessor_revision != Some(req.revision - 1) {
                return Err(invalid(
                    "Blueprint revisions must advance from the prior revision.",
                ));
            }
            let previous:Option<String>=sqlx::query_scalar("SELECT status FROM learning_assessment_blueprints WHERE program_id=? AND id=? AND revision=?").bind(&req.program_id).bind(&req.blueprint_id).bind(req.revision-1).fetch_optional(&mut *tx).await.map_err(db)?;
            if previous.as_deref() != Some("accepted") {
                return Err(invalid("Previous accepted blueprint revision not found."));
            }
        }
        for req_slot in &req.requirements {
            let owned: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM learning_outcome_definitions WHERE id=? AND program_id=?",
            )
            .bind(&req_slot.outcome_id)
            .bind(&req.program_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
            if owned.is_none() {
                return Err(invalid(
                    "Blueprint outcome does not belong to this program.",
                ));
            }
        }
        let timestamp = now();
        sqlx::query("INSERT INTO learning_assessment_blueprints(id,program_id,revision,predecessor_revision,purpose,title,instructions,expected_minutes,allowed_aids_json,passing_score,rubric_json,feedback_timing,source_version_ids_json,status,change_reason,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,'accepted',?,?)").bind(&req.blueprint_id).bind(&req.program_id).bind(req.revision).bind(req.predecessor_revision).bind(purpose_name(req.purpose)).bind(req.title.trim()).bind(req.instructions.trim()).bind(i64::from(req.expected_minutes)).bind(json(&req.allowed_aids)?).bind(req.passing_score).bind(json(&req.rubric)?).bind(timing_name(req.feedback_timing)).bind(json(&req.source_version_ids)?).bind(req.change_reason.trim()).bind(timestamp).execute(&mut *tx).await.map_err(db)?;
        let mut slot_ordinal = 0_i64;
        for requirement in &req.requirements {
            for _ in 0..requirement.count {
                sqlx::query("INSERT INTO learning_assessment_blueprint_slots(blueprint_id,blueprint_revision,ordinal,outcome_id,format,difficulty,difficulty_max,points) VALUES(?,?,?,?,?,?,?,1)").bind(&req.blueprint_id).bind(req.revision).bind(slot_ordinal).bind(&requirement.outcome_id).bind(format_name(requirement.format)).bind(i64::from(requirement.difficulty_min)).bind(i64::from(requirement.difficulty_max)).execute(&mut *tx).await.map_err(db)?;
                slot_ordinal += 1;
            }
        }
        let mut candidate_ids = HashSet::new();
        for input in candidates {
            if (!req.source_version_ids.is_empty() && input.source_version_ids.is_empty())
                || input
                    .source_version_ids
                    .iter()
                    .any(|id| !req.source_version_ids.contains(id))
            {
                return Err(invalid(
                    "Assessment candidate citations must match its frozen blueprint sources.",
                ));
            }
            if !candidate_ids.insert(input.id.as_str()) {
                return Err(invalid("Blueprint repeats a candidate ID."));
            }
            let key: engine::LearningAssessmentKey =
                serde_json::from_value(input.answer_key.clone())
                    .map_err(|_| invalid("Assessment candidate key is invalid."))?;
            let candidate = engine::LearningAssessmentCandidate {
                id: input.id.clone(),
                outcome_ids: input.outcome_ids.clone(),
                format: input.format,
                difficulty: input.difficulty,
                prompt: input.prompt.trim().into(),
                options: input.options.clone(),
                source_version_ids: input.source_version_ids.clone(),
                rubric: input.rubric.clone(),
                key,
            };
            engine::validate_candidate(&candidate)?;
            for outcome in &input.outcome_ids {
                let own: Option<i64> = sqlx::query_scalar(
                    "SELECT 1 FROM learning_outcome_definitions WHERE program_id=? AND id=?",
                )
                .bind(&req.program_id)
                .bind(outcome)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
                if own.is_none() {
                    return Err(invalid(
                        "Candidate outcome does not belong to this program.",
                    ));
                }
            }
            Self::verify_sources(&mut tx, &req.program_id, &input.source_version_ids).await?;
            if input.answer_explanation.trim().chars().count() < 8
                || input.answer_explanation.chars().count() > 1200
            {
                return Err(invalid(
                    "Assessment rationale must contain 8–1200 characters.",
                ));
            }
            let content_hash = persistence::hash(&(&candidate, input.artifact_kind.as_deref()))?;
            let primary = input
                .outcome_ids
                .first()
                .ok_or_else(|| invalid("Candidate requires an outcome."))?;
            sqlx::query("INSERT INTO learning_assessment_candidates(id,program_id,outcome_id,format,difficulty,prompt,options_json,artifact_kind,rubric_json,source_version_ids_json,authored_by,author_model,content_sha256,created_at) VALUES(?,?,?,?,?,?,?,?,?,?, 'model',?, ?,?)").bind(&input.id).bind(&req.program_id).bind(primary).bind(format_name(input.format)).bind(i64::from(input.difficulty)).bind(input.prompt.trim()).bind(json(&input.options)?).bind(&input.artifact_kind).bind(json(&input.rubric)?).bind(json(&input.source_version_ids)?).bind(author_model).bind(content_hash).bind(timestamp).execute(&mut *tx).await.map_err(db)?;
            for outcome in &input.outcome_ids {
                sqlx::query("INSERT INTO learning_assessment_candidate_outcomes(candidate_id,outcome_id) VALUES(?,?)").bind(&input.id).bind(outcome).execute(&mut *tx).await.map_err(db)?;
            }
            sqlx::query("INSERT INTO learning_assessment_answer_keys(candidate_id,selected_index,accepted_answers_json,ordered_values_json,explanation) VALUES(?,?,?,?,?)").bind(&input.id).bind(match &candidate.key{engine::LearningAssessmentKey::Choice(i)=>Some(*i as i64),_=>None}).bind(match &candidate.key{engine::LearningAssessmentKey::TextVariants(a)=>json(a)?,_=>"[]".into()}).bind(match &candidate.key{engine::LearningAssessmentKey::Ordering(a)=>json(a)?,_=>"[]".into()}).bind(input.answer_explanation.trim()).execute(&mut *tx).await.map_err(db)?;
            sqlx::query("INSERT INTO learning_assessment_blueprint_candidates(program_id,blueprint_id,blueprint_revision,candidate_id) VALUES(?,?,?,?)")
                .bind(&req.program_id)
                .bind(&req.blueprint_id)
                .bind(req.revision)
                .bind(&input.id)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        if req.revision > 1 {
            sqlx::query("UPDATE learning_assessment_blueprints SET status='retired' WHERE program_id=? AND id=? AND revision=? AND status='accepted'").bind(&req.program_id).bind(&req.blueprint_id).bind(req.revision-1).execute(&mut *tx).await.map_err(db)?;
        }
        operation.record(&mut *tx, &req.blueprint_id).await?;
        tx.commit().await.map_err(db)?;
        self.workspace(&req.program_id).await
    }

    pub async fn start_form(
        &self,
        req: &StartLearningAssessmentFormRequestDto,
        hash: &str,
    ) -> Result<LearningAssessmentFormDto> {
        uuid(&req.program_id, "program")?;
        uuid(&req.form_id, "form")?;
        uuid(&req.blueprint_id, "blueprint")?;
        uuid(&req.operation_id, "operation")?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &req.program_id).await?;
        let operation = assessment_operation(
            &req.operation_id,
            &req.program_id,
            Some(&req.form_id),
            "create_form",
            hash,
            "Operation ID was already used for a different assessment form.",
        );
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return self.get_form(&req.form_id).await;
        }
        Self::active_program(&mut tx, &req.program_id).await?;
        if let Some(parent) = req.retake_of_form_id.as_deref() {
            uuid(parent, "retake form")?;
            let belongs: Option<String> =
                sqlx::query_scalar("SELECT program_id FROM learning_assessment_forms WHERE id=?")
                    .bind(parent)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(db)?;
            if belongs.as_deref() != Some(req.program_id.as_str()) {
                return Err(AppError::NotFound("Retake source form not found".into()));
            }
        }
        let bp=sqlx::query("SELECT * FROM learning_assessment_blueprints WHERE program_id=? AND id=? AND revision=? AND status='accepted'").bind(&req.program_id).bind(&req.blueprint_id).bind(req.blueprint_revision).fetch_optional(&mut *tx).await.map_err(db)?.ok_or_else(||AppError::NotFound("Accepted assessment blueprint version not found".into()))?;
        let source_ids: Vec<String> = decode(bp.get("source_version_ids_json"))?;
        Self::verify_sources(&mut tx, &req.program_id, &source_ids).await?;
        let slotrows=sqlx::query("SELECT outcome_id,format,difficulty,difficulty_max FROM learning_assessment_blueprint_slots WHERE blueprint_id=? AND blueprint_revision=? ORDER BY ordinal").bind(&req.blueprint_id).bind(req.blueprint_revision).fetch_all(&mut *tx).await.map_err(db)?;
        let mut requirements: Vec<engine::LearningBlueprintRequirement> = Vec::new();
        for row in slotrows {
            let outcome: String = row.get("outcome_id");
            let format = parse_format(row.get("format"))?;
            let diff = row.get::<i64, _>("difficulty").clamp(1, 5) as u8;
            let diff_max = row.get::<i64, _>("difficulty_max").clamp(1, 5) as u8;
            if let Some(req) = requirements
                .iter_mut()
                .find(|r| r.outcome_id == outcome && r.format == format)
            {
                req.count += 1;
                req.difficulty_min = req.difficulty_min.min(diff);
                req.difficulty_max = req.difficulty_max.max(diff_max);
            } else {
                requirements.push(engine::LearningBlueprintRequirement {
                    outcome_id: outcome,
                    format,
                    count: 1,
                    difficulty_min: diff,
                    difficulty_max: diff_max,
                });
            }
        }
        let allowed_aids: Vec<String> = decode(bp.get("allowed_aids_json"))?;
        let engine_bp = engine::LearningAssessmentBlueprint {
            id: req.blueprint_id.clone(),
            purpose: parse_purpose(bp.get("purpose"))?,
            title: bp.get("title"),
            instructions: bp.get("instructions"),
            expected_minutes: bp.get::<i64, _>("expected_minutes").clamp(1, 480) as u32,
            allowed_aids: allowed_aids.clone(),
            pass_points: 1,
            feedback_timing: parse_timing(bp.get("feedback_timing"))?,
            requirements,
        };
        let candidates = self
            .candidates_for_blueprint(
                &mut tx,
                &req.program_id,
                &req.blueprint_id,
                req.blueprint_revision,
                &engine_bp,
            )
            .await?;
        let exposed_rows=sqlx::query("SELECT i.candidate_id FROM learning_assessment_form_items i JOIN learning_assessment_forms f ON f.id=i.form_id WHERE f.program_id=? AND f.status IN ('submitted','interrupted')").bind(&req.program_id).fetch_all(&mut *tx).await.map_err(db)?;
        let exposed = exposed_rows
            .into_iter()
            .map(|r| r.get::<String, _>("candidate_id"))
            .collect::<HashSet<_>>();
        let form = engine::build_assessment_form(
            &engine_bp,
            &candidates
                .iter()
                .map(|c| c.candidate.clone())
                .collect::<Vec<_>>(),
            &exposed,
            req.form_id.clone(),
            now(),
        )?;
        if form.items.len() > MAX_FORM_ITEMS {
            return Err(invalid("Assessment form exceeds the item limit."));
        }
        let timestamp = now();
        let passing_score: f64 = bp.get("passing_score");
        let rubric: String = bp.get("rubric_json");
        let feedback_timing: String = bp.get("feedback_timing");
        let model_name: Option<String> =
            sqlx::query_scalar("SELECT model_name FROM learning_programs WHERE id=?")
                .bind(&req.program_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        let mut frozen_sources = source_ids.clone();
        for item in &form.items {
            if let Some(candidate) = candidates.iter().find(|c| c.candidate.id == item.id) {
                frozen_sources.extend(candidate.candidate.source_version_ids.clone());
            }
        }
        frozen_sources.sort();
        frozen_sources.dedup();
        Self::verify_sources(&mut tx, &req.program_id, &frozen_sources).await?;
        sqlx::query("INSERT INTO learning_assessment_forms(id,program_id,blueprint_id,blueprint_revision,retake_of_form_id,status,revision,title,instructions,expected_minutes,allowed_aids_json,purpose,passing_score,rubric_json,feedback_timing,source_version_ids_json,model_name,created_at,updated_at,submitted_at) VALUES(?,?,?,?,?,'active',0,?,?,?,?,?,?,?,?,?,?,?,?,NULL)")
            .bind(&req.form_id).bind(&req.program_id).bind(&req.blueprint_id).bind(req.blueprint_revision).bind(&req.retake_of_form_id).bind(form.title).bind(form.instructions).bind(i64::from(engine_bp.expected_minutes)).bind(json(&allowed_aids)?).bind(purpose_name(form.purpose)).bind(passing_score).bind(rubric).bind(&feedback_timing).bind(json(&frozen_sources)?).bind(model_name).bind(timestamp).bind(timestamp).execute(&mut *tx).await.map_err(db)?;
        for (ordinal, item) in form.items.iter().enumerate() {
            let candidate = candidates
                .iter()
                .find(|c| c.candidate.id == item.id)
                .ok_or_else(|| {
                    AppError::InvalidState("Selected assessment candidate disappeared.".into())
                })?;
            let points=sqlx::query_scalar::<_,f64>("SELECT points FROM learning_assessment_blueprint_slots WHERE blueprint_id=? AND blueprint_revision=? AND outcome_id=? AND format=? ORDER BY ordinal LIMIT 1").bind(&req.blueprint_id).bind(req.blueprint_revision).bind(item.outcome_ids.first().ok_or_else(||invalid("Item missing outcome"))?).bind(format_name(item.format)).fetch_optional(&mut *tx).await.map_err(db)?.unwrap_or(1.0);
            let primary = item
                .outcome_ids
                .first()
                .ok_or_else(|| invalid("Assessment item has no outcome."))?;
            sqlx::query("INSERT INTO learning_assessment_form_items(form_id,ordinal,candidate_id,outcome_id,format,difficulty,prompt,options_json,artifact_kind,rubric_json,points,previously_exposed) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(&req.form_id).bind(ordinal as i64).bind(&item.id).bind(primary).bind(format_name(item.format)).bind(i64::from(candidate.candidate.difficulty)).bind(&item.prompt).bind(json(&item.options)?).bind(&candidate.artifact_kind).bind(json(&item.rubric)?).bind(points).bind(item.previously_exposed as i64).execute(&mut *tx).await.map_err(db)?;
            for outcome in &item.outcome_ids {
                sqlx::query("INSERT INTO learning_assessment_form_item_outcomes(form_id,item_ordinal,outcome_id) VALUES(?,?,?)").bind(&req.form_id).bind(ordinal as i64).bind(outcome).execute(&mut *tx).await.map_err(db)?;
            }
        }
        operation.record(&mut *tx, &req.form_id).await?;
        tx.commit().await.map_err(db)?;
        self.get_form(&req.form_id).await
    }

    async fn candidates_for_blueprint(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        program_id: &str,
        blueprint_id: &str,
        blueprint_revision: i64,
        bp: &engine::LearningAssessmentBlueprint,
    ) -> Result<Vec<StoredCandidate>> {
        let mut ids = HashSet::new();
        for r in &bp.requirements {
            ids.insert(r.outcome_id.as_str());
        }
        let mut candidates = Vec::new();
        for outcome in ids {
            let rows=sqlx::query("SELECT c.*,k.selected_index,k.accepted_answers_json,k.ordered_values_json,k.explanation FROM learning_assessment_blueprint_candidates bc JOIN learning_assessment_candidates c ON c.program_id=bc.program_id AND c.id=bc.candidate_id JOIN learning_assessment_candidate_outcomes o ON o.candidate_id=c.id JOIN learning_assessment_answer_keys k ON k.candidate_id=c.id WHERE bc.program_id=? AND bc.blueprint_id=? AND bc.blueprint_revision=? AND o.outcome_id=?")
                .bind(program_id).bind(blueprint_id).bind(blueprint_revision).bind(outcome).fetch_all(&mut **tx).await.map_err(db)?;
            for row in rows {
                let id: String = row.get("id");
                if candidates
                    .iter()
                    .any(|c: &StoredCandidate| c.candidate.id == id)
                {
                    continue;
                }
                let outcomes=sqlx::query_scalar::<_,String>("SELECT outcome_id FROM learning_assessment_candidate_outcomes WHERE candidate_id=? UNION SELECT outcome_id FROM learning_assessment_candidates WHERE id=?").bind(&id).bind(&id).fetch_all(&mut **tx).await.map_err(db)?;
                let format = parse_format(row.get("format"))?;
                let selected: Option<i64> = row.get("selected_index");
                let accepted: Vec<String> = decode(row.get("accepted_answers_json"))?;
                let ordered: Vec<String> = decode(row.get("ordered_values_json"))?;
                let key = match format {
                    engine::LearningItemFormat::MultipleChoice => {
                        engine::LearningAssessmentKey::Choice(selected.ok_or_else(|| {
                            AppError::Database("Choice candidate has no answer key".into())
                        })? as usize)
                    }
                    engine::LearningItemFormat::ShortAnswer => {
                        engine::LearningAssessmentKey::TextVariants(accepted)
                    }
                    engine::LearningItemFormat::Ordering => {
                        engine::LearningAssessmentKey::Ordering(ordered)
                    }
                    _ => engine::LearningAssessmentKey::Rubric,
                };
                let candidate = engine::LearningAssessmentCandidate {
                    id,
                    outcome_ids: outcomes,
                    format,
                    difficulty: row.get::<i64, _>("difficulty").clamp(1, 5) as u8,
                    prompt: row.get("prompt"),
                    options: decode(row.get("options_json"))?,
                    source_version_ids: decode(row.get("source_version_ids_json"))?,
                    rubric: decode(row.get("rubric_json"))?,
                    key,
                };
                candidates.push(StoredCandidate {
                    candidate,
                    artifact_kind: row.get("artifact_kind"),
                });
            }
        }
        Ok(candidates)
    }

    async fn candidate_rows_for_form(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        form_id: &str,
        program_id: &str,
    ) -> Result<Vec<StoredCandidate>> {
        let ids=sqlx::query_scalar::<_,String>("SELECT candidate_id FROM learning_assessment_form_items WHERE form_id=? ORDER BY ordinal").bind(form_id).fetch_all(&mut **tx).await.map_err(db)?;
        let mut candidates = Vec::new();
        for id in ids {
            let row=sqlx::query("SELECT c.*,k.selected_index,k.accepted_answers_json,k.ordered_values_json FROM learning_assessment_candidates c JOIN learning_assessment_answer_keys k ON k.candidate_id=c.id WHERE c.id=? AND c.program_id=?").bind(&id).bind(program_id).fetch_optional(&mut **tx).await.map_err(db)?.ok_or_else(||AppError::InvalidState("Assessment candidate key is unavailable.".into()))?;
            let outcomes=sqlx::query_scalar::<_,String>("SELECT outcome_id FROM learning_assessment_form_item_outcomes WHERE form_id=? AND item_ordinal=(SELECT ordinal FROM learning_assessment_form_items WHERE form_id=? AND candidate_id=?) ORDER BY outcome_id").bind(form_id).bind(form_id).bind(&id).fetch_all(&mut **tx).await.map_err(db)?;
            let format = parse_format(row.get("format"))?;
            let key = match format {
                engine::LearningItemFormat::MultipleChoice => {
                    engine::LearningAssessmentKey::Choice(
                        row.get::<Option<i64>, _>("selected_index").ok_or_else(|| {
                            AppError::Database("Choice candidate has no answer key".into())
                        })? as usize,
                    )
                }
                engine::LearningItemFormat::ShortAnswer => {
                    engine::LearningAssessmentKey::TextVariants(decode(
                        row.get("accepted_answers_json"),
                    )?)
                }
                engine::LearningItemFormat::Ordering => {
                    engine::LearningAssessmentKey::Ordering(decode(row.get("ordered_values_json"))?)
                }
                _ => engine::LearningAssessmentKey::Rubric,
            };
            let candidate = engine::LearningAssessmentCandidate {
                id: row.get("id"),
                outcome_ids: outcomes,
                format,
                difficulty: row.get::<i64, _>("difficulty").clamp(1, 5) as u8,
                prompt: row.get("prompt"),
                options: decode(row.get("options_json"))?,
                source_version_ids: decode(row.get("source_version_ids_json"))?,
                rubric: decode(row.get("rubric_json"))?,
                key,
            };
            candidates.push(StoredCandidate {
                candidate,
                artifact_kind: row.get("artifact_kind"),
            });
        }
        Ok(candidates)
    }

    pub async fn get_form(&self, form_id: &str) -> Result<LearningAssessmentFormDto> {
        uuid(form_id, "form")?;
        let h = sqlx::query("SELECT * FROM learning_assessment_forms WHERE id=?")
            .bind(form_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Assessment form not found".into()))?;
        let status = parse_form_status(h.get("status"))?;
        let rows=sqlx::query("SELECT i.*,o.outcome_id,rr.revision response_revision,rr.selected_index,rr.text_response,rr.ordered_values_json,rr.artifact_json FROM learning_assessment_form_items i JOIN learning_assessment_form_item_outcomes o ON o.form_id=i.form_id AND o.item_ordinal=i.ordinal LEFT JOIN learning_assessment_response_revisions rr ON rr.form_id=i.form_id AND rr.item_ordinal=i.ordinal AND rr.revision=(SELECT max(r2.revision) FROM learning_assessment_response_revisions r2 WHERE r2.form_id=i.form_id AND r2.item_ordinal=i.ordinal) WHERE i.form_id=? ORDER BY i.ordinal,o.outcome_id").bind(form_id).fetch_all(&self.pool).await.map_err(db)?;
        let mut items: Vec<LearningAssessmentFormItemDto> = Vec::new();
        for r in rows {
            let id: String = r.get("candidate_id");
            if let Some(item) = items.iter_mut().find(|i| i.id == id) {
                let outcome: String = r.get("outcome_id");
                if !item.outcome_ids.contains(&outcome) {
                    item.outcome_ids.push(outcome);
                }
                continue;
            }
            let ordinal: i64 = r.get("ordinal");
            let _ = ordinal;
            items.push(LearningAssessmentFormItemDto {
                id,
                outcome_ids: vec![r.get("outcome_id")],
                format: parse_format(r.get("format"))?,
                difficulty: r.get::<i64, _>("difficulty").clamp(1, 5) as u8,
                prompt: r.get("prompt"),
                options: decode(r.get("options_json"))?,
                artifact_kind: r.get("artifact_kind"),
                rubric: decode(r.get("rubric_json"))?,
                points: r.get("points"),
                source_version_ids: vec![],
                previously_exposed: r.get::<i64, _>("previously_exposed") != 0,
                selected_index: r
                    .get::<Option<i64>, _>("selected_index")
                    .map(|v| v as usize),
                text_response: r.get("text_response"),
                ordered_values: decode(
                    r.get::<Option<String>, _>("ordered_values_json")
                        .as_deref()
                        .unwrap_or("[]"),
                )?,
                artifact_json: r
                    .get::<Option<String>, _>("artifact_json")
                    .map(|s| decode(&s))
                    .transpose()?,
                response_revision: r.get::<Option<i64>, _>("response_revision").unwrap_or(0),
            });
        }
        for item in &mut items {
            let source_ids:String=sqlx::query_scalar("SELECT source_version_ids_json FROM learning_assessment_candidates WHERE program_id=? AND id=?").bind(h.get::<String,_>("program_id")).bind(&item.id).fetch_one(&self.pool).await.map_err(db)?;
            item.source_version_ids = decode(&source_ids)?;
        }
        let submission = self.load_submission(form_id).await?;
        Ok(LearningAssessmentFormDto {
            id: h.get("id"),
            program_id: h.get("program_id"),
            blueprint_id: h.get("blueprint_id"),
            blueprint_revision: h.get("blueprint_revision"),
            retake_of_form_id: h.get("retake_of_form_id"),
            status,
            revision: h.get("revision"),
            title: h.get("title"),
            instructions: h.get("instructions"),
            expected_minutes: h.get::<i64, _>("expected_minutes").clamp(1, 480) as u32,
            allowed_aids: decode(h.get("allowed_aids_json"))?,
            purpose: parse_purpose(h.get("purpose"))?,
            passing_score: h.get("passing_score"),
            feedback_timing: parse_timing(h.get("feedback_timing"))?,
            rubric: decode(h.get("rubric_json"))?,
            source_version_ids: decode(h.get("source_version_ids_json"))?,
            model_name: h.get("model_name"),
            items,
            created_at: h.get("created_at"),
            updated_at: h.get("updated_at"),
            submitted_at: h.get("submitted_at"),
            submission,
        })
    }

    async fn load_submission(
        &self,
        form_id: &str,
    ) -> Result<Option<LearningAssessmentSubmissionDto>> {
        let Some(r) = sqlx::query("SELECT * FROM learning_assessment_submissions WHERE form_id=?")
            .bind(form_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
        else {
            return Ok(None);
        };
        let rows = sqlx::query(
            "SELECT * FROM learning_assessment_item_results WHERE form_id=? ORDER BY item_ordinal",
        )
        .bind(form_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let mut results = Vec::new();
        for row in rows {
            let ordinal: i64 = row.get("item_ordinal");
            let outcomes=sqlx::query_scalar::<_,String>("SELECT outcome_id FROM learning_assessment_form_item_outcomes WHERE form_id=? AND item_ordinal=? ORDER BY outcome_id").bind(form_id).bind(ordinal).fetch_all(&self.pool).await.map_err(db)?;
            results.push(LearningAssessmentItemResultDto{item_id:sqlx::query_scalar("SELECT candidate_id FROM learning_assessment_form_items WHERE form_id=? AND ordinal=?").bind(form_id).bind(ordinal).fetch_one(&self.pool).await.map_err(db)?,outcome_ids:outcomes,score:row.get("score"),correct:row.get("correct"),grade_status:parse_grade(row.get("grade_status"))?,feedback:row.get("feedback"),criterion_results:decode(row.get("criterion_results_json"))?,artifact_quotes:decode(row.get("artifact_quotes_json"))?});
        }
        Ok(Some(LearningAssessmentSubmissionDto {
            score: r.get("score"),
            passed: r.get::<i64, _>("passed") != 0,
            grade_status: parse_grade(r.get("grade_status"))?,
            grader_model: r.get("grader_model"),
            grader_disagreement: decode(r.get("grader_disagreement_json"))?,
            feedback: r.get("feedback"),
            item_results: results,
            submitted_at: r.get("submitted_at"),
        }))
    }

    pub async fn save_response(
        &self,
        req: &SaveLearningAssessmentResponseRequestDto,
        hash: &str,
    ) -> Result<LearningAssessmentFormDto> {
        uuid(&req.operation_id, "operation")?;
        uuid(&req.program_id, "program")?;
        uuid(&req.form_id, "form")?;
        if req.assistance.len() > 24
            || req
                .assistance
                .iter()
                .any(|s| s.trim().is_empty() || s.len() > 120)
        {
            return Err(invalid("Assessment assistance conditions are bounded."));
        }
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &req.program_id).await?;
        let operation = assessment_operation(
            &req.operation_id,
            &req.program_id,
            Some(&req.form_id),
            "save_response",
            hash,
            "Operation ID was reused with different assessment data.",
        );
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return self.get_form(&req.form_id).await;
        }
        Self::active_program(&mut tx, &req.program_id).await?;
        let h = sqlx::query(
            "SELECT revision,status FROM learning_assessment_forms WHERE id=? AND program_id=?",
        )
        .bind(&req.form_id)
        .bind(&req.program_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Assessment form not found".into()))?;
        if h.get::<String, _>("status") != "active" {
            return Err(invalid("Only active assessment forms can be edited."));
        }
        if h.get::<i64, _>("revision") != req.expected_revision {
            return Err(invalid("Assessment form changed; reload and retry."));
        }
        if req.assistance.len() > 24
            || req
                .assistance
                .iter()
                .any(|a| a.trim().is_empty() || a.chars().count() > 160)
        {
            return Err(invalid(
                "Assessment assistance notes are outside their allowed bounds.",
            ));
        }
        let item=sqlx::query("SELECT format,options_json FROM learning_assessment_form_items WHERE form_id=? AND candidate_id=?").bind(&req.form_id).bind(&req.response.item_id).fetch_optional(&mut *tx).await.map_err(db)?.ok_or_else(||invalid("Response item does not belong to this form."))?;
        validate_response(
            &req.response,
            parse_format(item.get("format"))?,
            &decode::<Vec<String>>(item.get("options_json"))?,
        )?;
        let last:Option<i64>=sqlx::query_scalar("SELECT max(revision) FROM learning_assessment_response_revisions WHERE form_id=? AND item_ordinal=(SELECT ordinal FROM learning_assessment_form_items WHERE form_id=? AND candidate_id=?)").bind(&req.form_id).bind(&req.form_id).bind(&req.response.item_id).fetch_one(&mut *tx).await.map_err(db)?;
        let next = last.unwrap_or(0) + 1;
        let t = now();
        let ordinal: i64 = item_ordinal(&mut tx, &req.form_id, &req.response.item_id).await?;
        let updated = sqlx::query("UPDATE learning_assessment_forms SET revision=revision+1,updated_at=? WHERE id=? AND program_id=? AND revision=? AND status='active'").bind(t).bind(&req.form_id).bind(&req.program_id).bind(req.expected_revision).execute(&mut *tx).await.map_err(db)?;
        if updated.rows_affected() != 1 {
            return Err(invalid("Assessment form changed; reload and retry."));
        }
        sqlx::query("INSERT INTO learning_assessment_response_revisions(form_id,item_ordinal,revision,operation_id,payload_hash,selected_index,text_response,ordered_values_json,artifact_json,assistance_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(&req.form_id).bind(ordinal).bind(next).bind(&req.operation_id).bind(hash).bind(req.response.selected_index.map(|i|i as i64)).bind(&req.response.text).bind(json(&req.response.ordered_values)?).bind(req.response.artifact_json.as_ref().map(serde_json::to_string).transpose().map_err(|e|AppError::Serialization(e.to_string()))?).bind(json(&req.assistance)?).bind(t).execute(&mut *tx).await.map_err(db)?;
        operation.record(&mut *tx, &req.response.item_id).await?;
        tx.commit().await.map_err(db)?;
        self.get_form(&req.form_id).await
    }

    pub async fn interrupt(
        &self,
        req: &MutateLearningAssessmentFormRequestDto,
        hash: &str,
    ) -> Result<LearningAssessmentFormDto> {
        self.mutate_form(req, hash, "interrupt", |_| Ok(())).await
    }

    async fn mutate_form<F>(
        &self,
        req: &MutateLearningAssessmentFormRequestDto,
        hash: &str,
        kind: &str,
        _validate: F,
    ) -> Result<LearningAssessmentFormDto>
    where
        F: FnOnce(&str) -> Result<()>,
    {
        uuid(&req.operation_id, "operation")?;
        uuid(&req.program_id, "program")?;
        uuid(&req.form_id, "form")?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &req.program_id).await?;
        let operation = assessment_operation(
            &req.operation_id,
            &req.program_id,
            Some(&req.form_id),
            kind,
            hash,
            "Operation ID was reused with different assessment data.",
        );
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return self.get_form(&req.form_id).await;
        }
        Self::active_program(&mut tx, &req.program_id).await?;
        let row = sqlx::query(
            "SELECT revision,status FROM learning_assessment_forms WHERE program_id=? AND id=?",
        )
        .bind(&req.program_id)
        .bind(&req.form_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Assessment form not found".into()))?;
        if row.get::<String, _>("status") != "active" {
            return Err(invalid("Only active assessment forms can be interrupted."));
        }
        if row.get::<i64, _>("revision") != req.expected_revision {
            return Err(invalid("Assessment form changed; reload and retry."));
        }
        let t = now();
        let updated = sqlx::query("UPDATE learning_assessment_forms SET status='interrupted',revision=revision+1,updated_at=? WHERE id=? AND program_id=? AND revision=? AND status='active'").bind(t).bind(&req.form_id).bind(&req.program_id).bind(req.expected_revision).execute(&mut *tx).await.map_err(db)?;
        if updated.rows_affected() != 1 {
            return Err(invalid("Assessment form changed; reload and retry."));
        }
        operation.record(&mut *tx, &()).await?;
        tx.commit().await.map_err(db)?;
        self.get_form(&req.form_id).await
    }

    pub async fn submit(
        &self,
        req: &MutateLearningAssessmentFormRequestDto,
        hash: &str,
        open_results: HashMap<
            String,
            (
                LearningAssessmentGradeStatus,
                Vec<LearningAssessmentCriterionResultDto>,
            ),
        >,
        grader_model: Option<String>,
    ) -> Result<LearningAssessmentFormDto> {
        uuid(&req.operation_id, "operation")?;
        uuid(&req.program_id, "program")?;
        uuid(&req.form_id, "form")?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &req.program_id).await?;
        let operation = assessment_operation(
            &req.operation_id,
            &req.program_id,
            Some(&req.form_id),
            "submit",
            hash,
            "Operation ID was reused with different assessment data.",
        );
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return self.get_form(&req.form_id).await;
        }
        Self::active_program(&mut tx, &req.program_id).await?;
        let h = sqlx::query("SELECT * FROM learning_assessment_forms WHERE id=? AND program_id=?")
            .bind(&req.form_id)
            .bind(&req.program_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Assessment form not found".into()))?;
        if h.get::<String, _>("status") != "active" {
            return Err(invalid("Only active forms can be submitted."));
        }
        if h.get::<i64, _>("revision") != req.expected_revision {
            return Err(invalid("Assessment form changed; reload and retry."));
        }
        let rows=sqlx::query("SELECT i.*,r.selected_index,r.text_response,r.ordered_values_json,r.artifact_json,r.assistance_json FROM learning_assessment_form_items i LEFT JOIN learning_assessment_response_revisions r ON r.form_id=i.form_id AND r.item_ordinal=i.ordinal AND r.revision=(SELECT max(x.revision) FROM learning_assessment_response_revisions x WHERE x.form_id=i.form_id AND x.item_ordinal=i.ordinal) WHERE i.form_id=? ORDER BY i.ordinal").bind(&req.form_id).fetch_all(&mut *tx).await.map_err(db)?;
        for row in &rows {
            let answered = row.get::<Option<i64>, _>("selected_index").is_some()
                || row
                    .get::<Option<String>, _>("text_response")
                    .as_deref()
                    .is_some_and(|x| !x.trim().is_empty())
                || row.get::<Option<String>, _>("artifact_json").is_some()
                || row
                    .get::<Option<String>, _>("ordered_values_json")
                    .as_deref()
                    .is_some_and(|x| x != "[]");
            if !answered {
                return Err(invalid(
                    "Save a response for every assessment item before submitting.",
                ));
            }
        }
        let candidates = self
            .candidate_rows_for_form(&mut tx, &req.form_id, &req.program_id)
            .await?;
        let answers = rows
            .iter()
            .filter_map(|r| {
                let id: String = r.get("candidate_id");
                let sel = r
                    .get::<Option<i64>, _>("selected_index")
                    .map(|x| x as usize);
                let text: Option<String> = r.get("text_response");
                let order: Vec<String> = decode(
                    r.get::<Option<String>, _>("ordered_values_json")
                        .as_deref()
                        .unwrap_or("[]"),
                )
                .unwrap_or_default();
                let artifact: Option<String> = r.get("artifact_json");
                if sel.is_none() && text.is_none() && order.is_empty() && artifact.is_none() {
                    None
                } else {
                    Some(engine::LearningAssessmentResponse {
                        item_id: id,
                        selected_index: sel,
                        text,
                        ordered_values: order,
                        artifact_json: artifact.and_then(|s| serde_json::from_str(&s).ok()),
                    })
                }
            })
            .collect::<Vec<_>>();
        let keyed_candidates = candidates
            .iter()
            .filter(|c| {
                matches!(
                    c.candidate.key,
                    engine::LearningAssessmentKey::Choice(_)
                        | engine::LearningAssessmentKey::TextVariants(_)
                        | engine::LearningAssessmentKey::Ordering(_)
                )
            })
            .map(|c| c.candidate.clone())
            .collect::<Vec<_>>();
        let keyed_answers = answers
            .iter()
            .filter(|a| keyed_candidates.iter().any(|c| c.id == a.item_id))
            .cloned()
            .collect::<Vec<_>>();
        let deterministic =
            engine::grade_deterministic_responses(&keyed_candidates, &keyed_answers)?;
        let det: HashMap<String, bool> = deterministic
            .into_iter()
            .map(|r| (r.item_id, r.correct))
            .collect();
        let mut item_results = Vec::new();
        let mut score_n = 0.0;
        let mut score_d = 0.0;
        let mut aggregate_status = LearningAssessmentGradeStatus::Deterministic;
        let mut event_ids = Vec::<String>::new();
        let mut stale_outcomes = HashSet::new();
        for row in &rows {
            let id: String = row.get("candidate_id");
            let format = parse_format(row.get("format"))?;
            let points: f64 = row.get("points");
            let candidate = candidates
                .iter()
                .find(|c| c.candidate.id == id)
                .ok_or_else(|| AppError::InvalidState("Form candidate disappeared.".into()))?;
            let response_present = row.get::<Option<i64>, _>("selected_index").is_some()
                || row.get::<Option<String>, _>("text_response").is_some()
                || row.get::<Option<String>, _>("artifact_json").is_some()
                || row
                    .get::<Option<String>, _>("ordered_values_json")
                    .is_some_and(|s| s != "[]");
            let (item_score, correct, status, feedback, criteria, quotes) = if matches!(
                format,
                engine::LearningItemFormat::MultipleChoice
                    | engine::LearningItemFormat::ShortAnswer
                    | engine::LearningItemFormat::Ordering
            ) {
                let correct = det.get(&id).copied().unwrap_or(false);
                (
                    if correct { 1.0 } else { 0.0 },
                    Some(correct),
                    LearningAssessmentGradeStatus::Deterministic,
                    if correct {
                        sqlx::query_scalar::<_,String>("SELECT explanation FROM learning_assessment_answer_keys WHERE candidate_id=?").bind(&id).fetch_one(&mut *tx).await.map_err(db)?
                    } else if response_present {
                        "Review the item rationale and compare it with the saved source.".into()
                    } else {
                        "Not answered.".into()
                    },
                    vec![],
                    vec![],
                )
            } else {
                let (status, results) = open_results
                    .get(&id)
                    .cloned()
                    .unwrap_or((LearningAssessmentGradeStatus::Uncertain, vec![]));
                aggregate_status = stronger_grade(aggregate_status, status.clone());
                let max_total = results.iter().map(|r| r.max_points).sum::<u32>();
                let scored = results.iter().filter_map(|r| r.score).sum::<u32>();
                let ratio = if max_total == 0 {
                    0.0
                } else {
                    f64::from(scored) / f64::from(max_total)
                };
                let quotes = results
                    .iter()
                    .filter_map(|r| r.artifact_quote.clone())
                    .collect::<Vec<_>>();
                (
                    ratio,
                    None,
                    status,
                    results
                        .iter()
                        .map(|r| r.observation.clone())
                        .collect::<Vec<_>>()
                        .join(" "),
                    results,
                    quotes,
                )
            };
            score_n += item_score * points;
            score_d += points;
            let outcomes:Vec<String>=sqlx::query_scalar("SELECT outcome_id FROM learning_assessment_form_item_outcomes WHERE form_id=? AND item_ordinal=? ORDER BY outcome_id").bind(&req.form_id).bind(row.get::<i64,_>("ordinal")).fetch_all(&mut *tx).await.map_err(db)?;
            for outcome in &outcomes {
                let dimension = dimension_for(format, parse_purpose(h.get("purpose"))?);
                let ev_id = uuid::Uuid::new_v4().to_string();
                let evidence_result = if status == LearningAssessmentGradeStatus::Uncertain
                    || status == LearningAssessmentGradeStatus::NeedsReview
                {
                    "uncertain"
                } else if item_score > 0.0 {
                    "observed"
                } else {
                    "not_observed"
                };
                let assistance_json: Option<String> = row.get("assistance_json");
                let assistance = assistance_json.as_deref().unwrap_or("[]");
                let evidence_assistance = serde_json::json!({"allowedAids":decode::<Vec<String>>(h.get("allowed_aids_json")).unwrap_or_default(),"used":decode::<Vec<String>>(assistance).unwrap_or_default()});
                let evidence_score = if matches!(
                    status,
                    LearningAssessmentGradeStatus::Uncertain
                        | LearningAssessmentGradeStatus::NeedsReview
                ) {
                    None
                } else {
                    Some(item_score)
                };
                sqlx::query("INSERT INTO learning_evidence_events(id,program_id,outcome_id,source_kind,source_id,dimension,result,score,observation,evidence_quote,assistance_json,observed_at) VALUES(?,?,?,'assessment',?,?,?,?,?,?,?,?)").bind(&ev_id).bind(&req.program_id).bind(outcome).bind(&req.form_id).bind(dimension).bind(evidence_result).bind(evidence_score).bind(&feedback).bind(quotes.first()).bind(json(&evidence_assistance)?).bind(now()).execute(&mut *tx).await.map_err(db)?;
                event_ids.push(ev_id.clone());
                if evidence_result == "not_observed" {
                    let latest:Option<i64>=sqlx::query_scalar("SELECT max(observed_at) FROM learning_evidence_events WHERE program_id=? AND outcome_id=? AND source_id<>?").bind(&req.program_id).bind(outcome).bind(&req.form_id).fetch_one(&mut *tx).await.map_err(db)?;
                    if latest.is_some_and(|date| now() - date > STALE_EVIDENCE_MS) {
                        stale_outcomes.insert(outcome.clone());
                    }
                }
                if item_score > 0.0
                    && !decode::<Vec<String>>(assistance)
                        .unwrap_or_default()
                        .is_empty()
                {
                    insert_followup(&mut tx,&req.program_id,Some(outcome),"assisted_success","Successful evidence included recorded assistance; consider a short independent recheck.","assessment",Some(&req.form_id),std::slice::from_ref(&ev_id)).await?;
                }
                if status == LearningAssessmentGradeStatus::Uncertain
                    || status == LearningAssessmentGradeStatus::NeedsReview
                {
                    insert_followup(&mut tx,&req.program_id,Some(outcome),"uncertain_grade","This response could not be graded reliably; review it or collect another clear example.","assessment",Some(&req.form_id),std::slice::from_ref(&ev_id)).await?;
                } else if item_score <= 0.0 && dimension == "transfer" {
                    insert_followup(&mut tx,&req.program_id,Some(outcome),"low_transfer","The changed-context response did not yet show the target outcome; try a new transfer example.","practice",Some(outcome),std::slice::from_ref(&ev_id)).await?;
                } else if item_score <= 0.0 {
                    insert_followup(&mut tx,&req.program_id,Some(outcome),"missed_outcome","This response did not yet show the target outcome; revisit the material and try another example.","lesson",None,std::slice::from_ref(&ev_id)).await?;
                }
            }
            item_results.push(LearningAssessmentItemResultDto {
                item_id: id,
                outcome_ids: outcomes,
                score: item_score,
                correct,
                grade_status: status,
                feedback,
                criterion_results: criteria,
                artifact_quotes: quotes,
            });
            let _ = candidate;
        }
        for outcome in stale_outcomes {
            insert_followup(
                &mut tx,
                &req.program_id,
                Some(&outcome),
                "stale_evidence",
                "Earlier evidence for this outcome is old enough to benefit from a fresh check.",
                "assessment",
                Some(&req.form_id),
                &event_ids,
            )
            .await?;
        }
        let total = if score_d == 0.0 {
            0.0
        } else {
            score_n / score_d
        };
        let passing: f64 = h.get("passing_score");
        let passed = total >= passing
            && !matches!(
                aggregate_status,
                LearningAssessmentGradeStatus::Uncertain
                    | LearningAssessmentGradeStatus::NeedsReview
            );
        let timestamp = now();
        let feedback=match aggregate_status{LearningAssessmentGradeStatus::Uncertain|LearningAssessmentGradeStatus::NeedsReview=>"Some responses could not be graded reliably. This result remains uncertain and should be reviewed.",LearningAssessmentGradeStatus::Provisional=>"Open responses received provisional rubric feedback. This is evidence for review, not a mastery claim.",LearningAssessmentGradeStatus::Deterministic=>"Keyed responses were graded deterministically against this form."};
        let grade_disagreement: Vec<String> = if matches!(
            aggregate_status,
            LearningAssessmentGradeStatus::Provisional
                | LearningAssessmentGradeStatus::Uncertain
                | LearningAssessmentGradeStatus::NeedsReview
        ) {
            vec!["Open-response grading is model-generated and has not been independently calibrated.".into()]
        } else {
            vec![]
        };
        sqlx::query("INSERT INTO learning_assessment_submissions(form_id,program_id,operation_id,payload_hash,score,passed,grade_status,grader_model,grader_disagreement_json,feedback,submitted_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(&req.form_id).bind(&req.program_id).bind(&req.operation_id).bind(hash).bind(total).bind(passed as i64).bind(grade_name(&aggregate_status)).bind(grader_model).bind(json(&grade_disagreement)?).bind(feedback).bind(timestamp).execute(&mut *tx).await.map_err(db)?;
        // Item results reference the submission row, so publish the immutable
        // submission first and its per-item result rows second, in this same
        // transaction.
        for result in &item_results {
            let ordinal: i64 = sqlx::query_scalar(
                "SELECT ordinal FROM learning_assessment_form_items WHERE form_id=? AND candidate_id=?",
            )
            .bind(&req.form_id)
            .bind(&result.item_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
            sqlx::query("INSERT INTO learning_assessment_item_results(form_id,item_ordinal,score,correct,grade_status,feedback,criterion_results_json,artifact_quotes_json) VALUES(?,?,?,?,?,?,?,?)")
                .bind(&req.form_id)
                .bind(ordinal)
                .bind(result.score)
                .bind(result.correct.map(i64::from))
                .bind(grade_name(&result.grade_status))
                .bind(&result.feedback)
                .bind(json(&result.criterion_results)?)
                .bind(json(&result.artifact_quotes)?)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        let updated = sqlx::query("UPDATE learning_assessment_forms SET status='submitted',revision=revision+1,submitted_at=?,updated_at=? WHERE id=? AND program_id=? AND revision=? AND status='active'").bind(timestamp).bind(timestamp).bind(&req.form_id).bind(&req.program_id).bind(req.expected_revision).execute(&mut *tx).await.map_err(db)?;
        if updated.rows_affected() != 1 {
            return Err(invalid("Assessment form changed; reload and retry."));
        }
        operation.record(&mut *tx, &()).await?;
        tx.commit().await.map_err(db)?;
        self.get_form(&req.form_id).await
    }

    pub async fn preflight_submit(
        &self,
        req: &MutateLearningAssessmentFormRequestDto,
        hash: &str,
    ) -> Result<Option<LearningAssessmentFormDto>> {
        uuid(&req.operation_id, "operation")?;
        uuid(&req.program_id, "program")?;
        uuid(&req.form_id, "form")?;
        if assessment_operation(
            &req.operation_id,
            &req.program_id,
            Some(&req.form_id),
            "submit",
            hash,
            "Operation ID was already used for different assessment data.",
        )
        .seen(&self.pool)
        .await?
        {
            return self.get_form(&req.form_id).await.map(Some);
        }
        Ok(None)
    }

    pub async fn decide_follow_up(
        &self,
        req: &DecideLearningFollowUpRequestDto,
        accept: bool,
        hash: &str,
    ) -> Result<LearningAssessmentWorkspaceDto> {
        uuid(&req.program_id, "program")?;
        uuid(&req.follow_up_id, "follow-up")?;
        uuid(&req.operation_id, "operation")?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &req.program_id).await?;
        let operation = assessment_operation(
            &req.operation_id,
            &req.program_id,
            None,
            "decide_follow_up",
            hash,
            "Operation ID was reused with different follow-up data.",
        );
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return self.workspace(&req.program_id).await;
        }
        Self::active_program(&mut tx, &req.program_id).await?;
        let status = if accept { "accepted" } else { "dismissed" };
        let result=sqlx::query("UPDATE learning_follow_up_recommendations SET status=?,decided_at=? WHERE id=? AND program_id=? AND status='pending'").bind(status).bind(now()).bind(&req.follow_up_id).bind(&req.program_id).execute(&mut *tx).await.map_err(db)?;
        if result.rows_affected() != 1 {
            return Err(AppError::NotFound("Pending follow-up not found".into()));
        }
        operation.record(&mut *tx, &req.follow_up_id).await?;
        tx.commit().await.map_err(db)?;
        self.workspace(&req.program_id).await
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
            engine::LearningItemFormat::MultipleChoice
            | engine::LearningItemFormat::ShortAnswer
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
        engine::LearningItemFormat::MultipleChoice => {
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
