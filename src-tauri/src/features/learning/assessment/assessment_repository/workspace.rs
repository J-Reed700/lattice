//! Reading the assessment workspace: outcomes, blueprints, forms, evidence and follow-ups.
use super::{
    decode, format_name, parse_action, parse_blueprint_status, parse_dimension,
    parse_evidence_kind, parse_evidence_result, parse_follow_status, parse_form_status,
    parse_format, parse_grade, parse_purpose, parse_reason, parse_timing, uuid,
    LearningAssessmentRepository,
};
use crate::features::learning::assessment_engine as engine;
use crate::features::learning::dto::*;
use crate::features::learning::persistence::db;
use lattice_core::shared::error::{AppError, Result};
use sqlx::Row;
use std::collections::HashMap;

impl LearningAssessmentRepository {
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
}
