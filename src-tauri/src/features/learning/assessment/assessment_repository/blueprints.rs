//! Preflighting and creating assessment blueprints against the program's outcomes and sources.
use super::{
    assessment_operation, format_name, invalid, json, purpose_name, rubric_valid, timing_name,
    uuid, LearningAssessmentRepository,
};
use crate::features::learning::assessment_engine as engine;
use crate::features::learning::dto::*;
use crate::features::learning::persistence::{self, db, now};
use crate::shared::error::Result;
use std::collections::HashSet;

impl LearningAssessmentRepository {
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
}
