//! Starting forms, drawing their candidate items, and saving responses while a form is open.
use super::{
    assessment_operation, decode, format_name, invalid, item_ordinal, json, parse_form_status,
    parse_format, parse_grade, parse_purpose, parse_timing, purpose_name, uuid, validate_response,
    LearningAssessmentRepository, StoredCandidate, MAX_FORM_ITEMS,
};
use crate::features::learning::assessment_engine as engine;
use crate::features::learning::dto::*;
use crate::features::learning::persistence::{db, now};
use crate::shared::error::{AppError, Result};
use sqlx::{Row, Sqlite, Transaction};
use std::collections::HashSet;

impl LearningAssessmentRepository {
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

    pub(super) async fn candidate_rows_for_form(
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
}
