//! Starting a quiz, reading a form, and saving answers while it is open.
use super::{
    assessment_operation, candidate_in, decode, format_name, invalid, json, mode_name,
    parse_form_status, parse_format, parse_grade, parse_mode, parse_purpose, parse_scope,
    parse_timing, scope_kind_name, uuid, LearningAssessmentRepository, MAX_RESPONSE_CHARS,
};
use crate::features::learning::assessment_engine as engine;
use crate::features::learning::dto::*;
use crate::features::learning::persistence::{db, now};
use crate::features::learning::quiz_blueprints;
use lattice_core::shared::error::{AppError, Result};
use sqlx::{Row, SqliteConnection};

/// What a saved answer looks like for one item.
fn validate_response(
    response: &engine::LearningAssessmentResponse,
    format: engine::LearningItemFormat,
    options: usize,
    left: usize,
) -> Result<()> {
    use engine::LearningItemFormat as F;
    let total_text = response.text.as_deref().map_or(0, |t| t.chars().count())
        + response
            .blank_responses
            .iter()
            .map(|b| b.chars().count())
            .sum::<usize>();
    if total_text > MAX_RESPONSE_CHARS
        || response.ordered_values.len() > 32
        || response.blank_responses.len() > 8
        || response.matches.len() > 8
        || response.selected_indices.len() > 12
        || response
            .artifact_json
            .as_ref()
            .is_some_and(|v| v.to_string().len() > MAX_RESPONSE_CHARS)
    {
        return Err(invalid("This answer is longer than a quiz answer may be."));
    }
    let in_range = |i: &usize| *i < options;
    let ok = match format {
        F::SingleChoice | F::TrueFalse => {
            response.selected_indices.len() <= 1 && response.selected_indices.iter().all(in_range)
        }
        F::MultiSelect => {
            response.selected_indices.iter().all(in_range)
                && response
                    .selected_indices
                    .iter()
                    .collect::<std::collections::HashSet<_>>()
                    .len()
                    == response.selected_indices.len()
        }
        F::ShortAnswer | F::Explanation => {
            response.selected_indices.is_empty() && response.ordered_values.is_empty()
        }
        F::Cloze => response.selected_indices.is_empty(),
        F::Matching => {
            response.matches.len() <= left && response.matches.iter().flatten().all(in_range)
        }
        F::Ordering => response.ordered_values.len() <= options,
        F::Artifact => response
            .artifact_json
            .as_ref()
            .is_none_or(serde_json::Value::is_object),
    };
    if !ok {
        return Err(invalid("This answer does not fit the question."));
    }
    Ok(())
}

impl LearningAssessmentRepository {
    /// Start a quiz from the latest blueprint for its scope, drawing unseen
    /// bank items first and shuffling option order for this form.
    pub async fn start_quiz(
        &self,
        req: &StartLearningQuizRequestDto,
        hash: &str,
    ) -> Result<LearningAssessmentFormDto> {
        uuid(&req.program_id, "program")?;
        uuid(&req.form_id, "form")?;
        uuid(&req.operation_id, "operation")?;
        quiz_blueprints::validate_scope(&req.scope)?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &req.program_id).await?;
        let operation = assessment_operation(
            &req.operation_id,
            &req.program_id,
            Some(&req.form_id),
            "create_form",
            hash,
            "Operation ID was already used for a different quiz.",
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
                return Err(AppError::NotFound(
                    "The quiz being retaken was not found.".into(),
                ));
            }
        }
        Self::ensure_outcomes(&mut tx, &req.program_id).await?;
        let Some((blueprint, desired)) =
            quiz_blueprints::ensure_blueprint_in(&mut tx, &req.program_id, &req.scope).await?
        else {
            return Err(AppError::InvalidState(match req.scope.kind {
                LearningQuizScopeKind::Lesson => {
                    "Prepare the lesson first; its quiz is written as part of preparation.".into()
                }
                _ => "Prepare at least one lesson first; this test asks about prepared lessons."
                    .into(),
            }));
        };
        let requirements = quiz_blueprints::requirements_in(&mut tx, &blueprint).await?;
        let bank = Self::bank_in(&mut tx, &req.program_id).await?;
        let exposed = Self::exposed_in(&mut tx, &req.program_id).await?;
        let engine_blueprint = engine::LearningAssessmentBlueprint {
            id: blueprint.id.clone(),
            purpose: desired.purpose,
            title: desired.title.clone(),
            instructions: desired.instructions.clone(),
            expected_minutes: desired.expected_minutes,
            allowed_aids: vec![],
            feedback_timing: engine::LearningFeedbackTiming::AfterSubmission,
            requirements,
        };
        let form = engine::build_assessment_form(
            &engine_blueprint,
            &bank.iter().map(|c| c.candidate.clone()).collect::<Vec<_>>(),
            &exposed,
            req.form_id.clone(),
            now(),
        )?;
        let timestamp = now();
        let model_name: Option<String> =
            sqlx::query_scalar("SELECT model_name FROM learning_programs WHERE id=?")
                .bind(&req.program_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        let mut frozen_sources = desired.source_version_ids.clone();
        for item in &form.items {
            frozen_sources.extend(item.source_version_ids.clone());
        }
        frozen_sources.sort();
        frozen_sources.dedup();
        sqlx::query("INSERT INTO learning_assessment_forms(id,program_id,blueprint_id,blueprint_revision,retake_of_form_id,status,revision,title,instructions,expected_minutes,allowed_aids_json,purpose,passing_score,rubric_json,feedback_timing,source_version_ids_json,model_name,created_at,updated_at,submitted_at,mode,scope_kind,scope_id) VALUES(?,?,?,?,?,'active',0,?,?,?,'[]',?,?,'[]','after_submission',?,?,?,?,NULL,?,?,?)")
            .bind(&req.form_id)
            .bind(&req.program_id)
            .bind(&blueprint.id)
            .bind(blueprint.revision)
            .bind(&req.retake_of_form_id)
            .bind(&form.title)
            .bind(&form.instructions)
            .bind(i64::from(form.expected_minutes))
            .bind(quiz_blueprints::purpose_name(form.purpose))
            .bind(desired.passing_score)
            .bind(json(&frozen_sources)?)
            .bind(model_name)
            .bind(timestamp)
            .bind(timestamp)
            .bind(mode_name(req.mode))
            .bind(scope_kind_name(req.scope.kind))
            .bind(&req.scope.id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        for (ordinal, item) in form.items.iter().enumerate() {
            let stored = bank
                .iter()
                .find(|c| c.candidate.id == item.id)
                .ok_or_else(|| {
                    AppError::InvalidState("A drawn quiz question disappeared.".into())
                })?;
            // Keyed items are worth a point; written answers their rubric's total.
            let points = if item.format.is_keyed() {
                1.0
            } else {
                f64::from(item.rubric.iter().map(|c| c.max_points).sum::<u32>().max(1))
            };
            let primary = item
                .outcome_ids
                .first()
                .ok_or_else(|| invalid("A quiz question has no outcome."))?;
            sqlx::query("INSERT INTO learning_assessment_form_items(form_id,ordinal,candidate_id,outcome_id,format,difficulty,prompt,options_json,artifact_kind,rubric_json,points,previously_exposed,option_order_json,left_items_json) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(&req.form_id)
                .bind(ordinal as i64)
                .bind(&item.id)
                .bind(primary)
                .bind(format_name(item.format))
                .bind(i64::from(stored.candidate.difficulty))
                .bind(&item.prompt)
                .bind(json(&item.options)?)
                .bind(&stored.artifact_kind)
                .bind(json(&item.rubric)?)
                .bind(points)
                .bind(item.previously_exposed as i64)
                .bind(json(&item.option_order)?)
                .bind(json(&item.left)?)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
            for outcome in &item.outcome_ids {
                sqlx::query("INSERT INTO learning_assessment_form_item_outcomes(form_id,item_ordinal,outcome_id) VALUES(?,?,?)").bind(&req.form_id).bind(ordinal as i64).bind(outcome).execute(&mut *tx).await.map_err(db)?;
            }
        }
        operation.record(&mut *tx, &req.form_id).await?;
        tx.commit().await.map_err(db)?;
        self.get_form(&req.form_id).await
    }

    pub async fn get_form(&self, form_id: &str) -> Result<LearningAssessmentFormDto> {
        let mut connection = self.pool.acquire().await.map_err(db)?;
        Self::form_in(&mut connection, form_id).await
    }

    pub(crate) async fn form_in(
        connection: &mut SqliteConnection,
        form_id: &str,
    ) -> Result<LearningAssessmentFormDto> {
        uuid(form_id, "form")?;
        let h = sqlx::query("SELECT * FROM learning_assessment_forms WHERE id=?")
            .bind(form_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Quiz not found".into()))?;
        let status = parse_form_status(h.get("status"))?;
        let mode = parse_mode(h.get("mode"))?;
        let program_id: String = h.get("program_id");
        let rows = sqlx::query("SELECT i.*,rr.revision response_revision,rr.selected_indices_json,rr.text_response,rr.blank_responses_json,rr.matches_json,rr.ordered_values_json,rr.artifact_json FROM learning_assessment_form_items i LEFT JOIN learning_assessment_response_revisions rr ON rr.form_id=i.form_id AND rr.item_ordinal=i.ordinal AND rr.revision=(SELECT max(r2.revision) FROM learning_assessment_response_revisions r2 WHERE r2.form_id=i.form_id AND r2.item_ordinal=i.ordinal) WHERE i.form_id=? ORDER BY i.ordinal")
            .bind(form_id)
            .fetch_all(&mut *connection)
            .await
            .map_err(db)?;
        let mut items = Vec::with_capacity(rows.len());
        let mut graded_parts = Vec::with_capacity(rows.len());
        for r in rows {
            let ordinal: i64 = r.get("ordinal");
            let id: String = r.get("candidate_id");
            let format = parse_format(r.get("format"))?;
            let outcome_ids = sqlx::query_scalar::<_, String>("SELECT outcome_id FROM learning_assessment_form_item_outcomes WHERE form_id=? AND item_ordinal=? ORDER BY outcome_id")
                .bind(form_id)
                .bind(ordinal)
                .fetch_all(&mut *connection)
                .await
                .map_err(db)?;
            let response = engine::LearningAssessmentResponse {
                item_id: id.clone(),
                selected_indices: decode(
                    r.get::<Option<String>, _>("selected_indices_json")
                        .as_deref()
                        .unwrap_or("[]"),
                )?,
                text: r.get("text_response"),
                blank_responses: decode(
                    r.get::<Option<String>, _>("blank_responses_json")
                        .as_deref()
                        .unwrap_or("[]"),
                )?,
                matches: decode(
                    r.get::<Option<String>, _>("matches_json")
                        .as_deref()
                        .unwrap_or("[]"),
                )?,
                ordered_values: decode(
                    r.get::<Option<String>, _>("ordered_values_json")
                        .as_deref()
                        .unwrap_or("[]"),
                )?,
                artifact_json: r
                    .get::<Option<String>, _>("artifact_json")
                    .map(|s| decode(&s))
                    .transpose()?,
            };
            let option_order: Vec<usize> = decode(r.get("option_order_json"))?;
            let reveal = status == LearningAssessmentFormStatus::Submitted
                || (mode == LearningQuizMode::Practice
                    && format.is_keyed()
                    && response.is_answered());
            let candidate = candidate_in(connection, &program_id, &id).await?;
            let key_reveal = reveal.then(|| {
                let reveal = engine::key_reveal(&candidate.candidate, &option_order);
                LearningKeyRevealDto {
                    correct_indices: reveal.correct_indices,
                    accepted_answer: reveal.accepted_answer,
                    ordered_values: reveal.ordered_values,
                    matches: reveal.matches,
                    blanks: reveal.blanks,
                    explanation: reveal.explanation,
                    distractor_rationales: reveal.distractor_rationales,
                }
            });
            let prompt: String = r.get("prompt");
            let answer_hint = engine::answer_hint(&candidate.candidate)
                .map(|(numeric, unit_hint)| LearningAnswerHintDto { numeric, unit_hint });
            graded_parts.push(format.is_keyed().then(|| {
                engine::part_results(
                    &candidate.candidate,
                    &engine::to_authored(&response, &option_order),
                    &option_order,
                )
            }));
            items.push(LearningAssessmentFormItemDto {
                id,
                outcome_ids,
                format,
                difficulty: r.get::<i64, _>("difficulty").clamp(1, 5) as u8,
                blank_count: engine::cloze_blanks(&prompt).len() as u32,
                prompt,
                options: decode(r.get("options_json"))?,
                left_items: decode(r.get("left_items_json"))?,
                right_items: if format == engine::LearningItemFormat::Matching {
                    decode(r.get("options_json"))?
                } else {
                    vec![]
                },
                artifact_kind: r.get("artifact_kind"),
                rubric: decode(r.get("rubric_json"))?,
                points: r.get("points"),
                source_version_ids: candidate.candidate.source_version_ids.clone(),
                source_quote: candidate.source_quote.clone(),
                previously_exposed: r.get::<i64, _>("previously_exposed") != 0,
                selected_indices: response.selected_indices,
                text_response: response.text,
                blank_responses: response.blank_responses,
                matches: response.matches,
                ordered_values: response.ordered_values,
                artifact_json: response.artifact_json,
                response_revision: r.get::<Option<i64>, _>("response_revision").unwrap_or(0),
                key_reveal,
                answer_hint,
            });
        }
        let mut submission = Self::submission_in(connection, form_id).await?;
        if let Some(submission) = submission.as_mut() {
            for (result, parts) in submission.item_results.iter_mut().zip(graded_parts) {
                result.part_results = parts
                    .unwrap_or_default()
                    .into_iter()
                    .map(|p| LearningPartResult {
                        index: p.index,
                        correct: p.correct,
                        detail: p.detail,
                    })
                    .collect();
            }
        }
        Ok(LearningAssessmentFormDto {
            id: h.get("id"),
            program_id,
            blueprint_id: h.get("blueprint_id"),
            blueprint_revision: h.get("blueprint_revision"),
            retake_of_form_id: h.get("retake_of_form_id"),
            status,
            revision: h.get("revision"),
            title: h.get("title"),
            instructions: h.get("instructions"),
            expected_minutes: h.get::<i64, _>("expected_minutes").max(1) as u32,
            purpose: parse_purpose(h.get("purpose"))?,
            passing_score: h.get("passing_score"),
            feedback_timing: parse_timing(h.get("feedback_timing"))?,
            source_version_ids: decode(h.get("source_version_ids_json"))?,
            model_name: h.get("model_name"),
            items,
            created_at: h.get("created_at"),
            updated_at: h.get("updated_at"),
            submitted_at: h.get("submitted_at"),
            submission,
            scope: parse_scope(h.get("scope_kind"), h.get("scope_id"))?,
            mode,
        })
    }

    async fn submission_in(
        connection: &mut SqliteConnection,
        form_id: &str,
    ) -> Result<Option<LearningAssessmentSubmissionDto>> {
        let Some(r) = sqlx::query("SELECT * FROM learning_assessment_submissions WHERE form_id=?")
            .bind(form_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(db)?
        else {
            return Ok(None);
        };
        let rows = sqlx::query("SELECT r.*,i.candidate_id FROM learning_assessment_item_results r JOIN learning_assessment_form_items i ON i.form_id=r.form_id AND i.ordinal=r.item_ordinal WHERE r.form_id=? ORDER BY r.item_ordinal")
            .bind(form_id)
            .fetch_all(&mut *connection)
            .await
            .map_err(db)?;
        let mut results = Vec::with_capacity(rows.len());
        for row in rows {
            let ordinal: i64 = row.get("item_ordinal");
            let outcome_ids = sqlx::query_scalar::<_, String>("SELECT outcome_id FROM learning_assessment_form_item_outcomes WHERE form_id=? AND item_ordinal=? ORDER BY outcome_id")
                .bind(form_id)
                .bind(ordinal)
                .fetch_all(&mut *connection)
                .await
                .map_err(db)?;
            let rationale_json: String = row.get("rationale_json");
            let rationale = if rationale_json.trim() == "{}" {
                None
            } else {
                Some(decode::<LearningGradeRationaleDto>(&rationale_json)?)
            };
            results.push(LearningAssessmentItemResultDto {
                item_id: row.get("candidate_id"),
                outcome_ids,
                score: row.get("score"),
                correct: row.get::<Option<i64>, _>("correct").map(|c| c != 0),
                grade_status: parse_grade(row.get("grade_status"))?,
                feedback: row.get("feedback"),
                points_earned: row.get("points_earned"),
                points_possible: row.get("points_possible"),
                detail: row.get("detail"),
                learner_override: row.get::<i64, _>("learner_override") != 0,
                rationale,
                part_results: vec![],
                contest: Self::contest_in(connection, form_id, ordinal).await?,
            });
        }
        // The grading job, while one is waiting or running. Without one,
        // each pending answer says why it is still waiting.
        let job = sqlx::query("SELECT id,status,error_message FROM jobs WHERE kind=? AND json_extract(requested_json,'$.formId')=? AND json_extract(requested_json,'$.contestId') IS NULL ORDER BY created_at DESC LIMIT 1")
            .bind(crate::features::learning::quiz_bank_jobs::QUIZ_GRADING)
            .bind(form_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(db)?;
        let live = job.as_ref().is_some_and(|j| {
            matches!(j.get::<String, _>("status").as_str(), "pending" | "running")
        });
        let grading_job_id = job
            .as_ref()
            .filter(|_| live)
            .map(|j| j.get::<String, _>("id"));
        if !live {
            let reason = match job
                .as_ref()
                .map(|j| j.get::<String, _>("status"))
                .as_deref()
            {
                Some("cancelled") => "Grading stopped. Resume grading to finish.".to_string(),
                Some("failed") => format!(
                    "Couldn't grade yet: {}",
                    job.as_ref()
                        .and_then(|j| j.get::<Option<String>, _>("error_message"))
                        .unwrap_or_else(|| "the grader stopped with an error.".into())
                ),
                _ => "Couldn't grade yet. Retry grading.".into(),
            };
            for result in results.iter_mut().filter(|r| {
                r.grade_status == LearningAssessmentGradeStatus::Pending && r.feedback.is_empty()
            }) {
                result.feedback = reason.clone();
            }
        }
        Ok(Some(LearningAssessmentSubmissionDto {
            score: r.get("score"),
            passed: r.get::<Option<i64>, _>("passed").map(|v| v != 0),
            grade_status: parse_grade(r.get("grade_status"))?,
            grader_model: r.get("grader_model"),
            grader_disagreement: decode(r.get("grader_disagreement_json"))?,
            feedback: r.get("feedback"),
            item_results: results,
            submitted_at: r.get("submitted_at"),
            score_without_overrides: r.get("score_without_overrides"),
            pending_items: r.get("pending_items"),
            grading_job_id,
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
                .any(|a| a.trim().is_empty() || a.chars().count() > 160)
        {
            return Err(invalid(
                "Assistance notes are outside their allowed bounds.",
            ));
        }
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &req.program_id).await?;
        let operation = assessment_operation(
            &req.operation_id,
            &req.program_id,
            Some(&req.form_id),
            "save_response",
            hash,
            "Operation ID was reused with a different answer.",
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
        .ok_or_else(|| AppError::NotFound("Quiz not found".into()))?;
        // Answering a paused quiz resumes it.
        if h.get::<String, _>("status") == "submitted" {
            return Err(invalid("A submitted quiz cannot be changed."));
        }
        if h.get::<i64, _>("revision") != req.expected_revision {
            return Err(invalid("This quiz changed; reload and retry."));
        }
        let item = sqlx::query("SELECT ordinal,format,options_json,left_items_json FROM learning_assessment_form_items WHERE form_id=? AND candidate_id=?")
            .bind(&req.form_id)
            .bind(&req.response.item_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or_else(|| invalid("That question is not part of this quiz."))?;
        validate_response(
            &req.response,
            parse_format(item.get("format"))?,
            decode::<Vec<String>>(item.get("options_json"))?.len(),
            decode::<Vec<String>>(item.get("left_items_json"))?.len(),
        )?;
        let ordinal: i64 = item.get("ordinal");
        let last: Option<i64> = sqlx::query_scalar("SELECT max(revision) FROM learning_assessment_response_revisions WHERE form_id=? AND item_ordinal=?")
            .bind(&req.form_id)
            .bind(ordinal)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
        let next = last.unwrap_or(0) + 1;
        let t = now();
        let updated = sqlx::query("UPDATE learning_assessment_forms SET status='active',revision=revision+1,updated_at=? WHERE id=? AND program_id=? AND revision=? AND status IN ('active','interrupted')").bind(t).bind(&req.form_id).bind(&req.program_id).bind(req.expected_revision).execute(&mut *tx).await.map_err(db)?;
        if updated.rows_affected() != 1 {
            return Err(invalid("This quiz changed; reload and retry."));
        }
        sqlx::query("INSERT INTO learning_assessment_response_revisions(form_id,item_ordinal,revision,operation_id,payload_hash,selected_indices_json,text_response,blank_responses_json,matches_json,ordered_values_json,artifact_json,assistance_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&req.form_id)
            .bind(ordinal)
            .bind(next)
            .bind(&req.operation_id)
            .bind(hash)
            .bind(json(&req.response.selected_indices)?)
            .bind(&req.response.text)
            .bind(json(&req.response.blank_responses)?)
            .bind(json(&req.response.matches)?)
            .bind(json(&req.response.ordered_values)?)
            .bind(req.response.artifact_json.as_ref().map(json).transpose()?)
            .bind(json(&req.assistance)?)
            .bind(t)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        operation.record(&mut *tx, &req.response.item_id).await?;
        tx.commit().await.map_err(db)?;
        self.get_form(&req.form_id).await
    }

    pub async fn interrupt(
        &self,
        req: &MutateLearningAssessmentFormRequestDto,
        hash: &str,
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
            "interrupt",
            hash,
            "Operation ID was reused with different quiz data.",
        );
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return self.get_form(&req.form_id).await;
        }
        Self::active_program(&mut tx, &req.program_id).await?;
        let t = now();
        let updated = sqlx::query("UPDATE learning_assessment_forms SET status='interrupted',revision=revision+1,updated_at=? WHERE id=? AND program_id=? AND revision=? AND status='active'").bind(t).bind(&req.form_id).bind(&req.program_id).bind(req.expected_revision).execute(&mut *tx).await.map_err(db)?;
        if updated.rows_affected() != 1 {
            return Err(invalid(
                "This quiz changed or is not open; reload and retry.",
            ));
        }
        operation.record(&mut *tx, &()).await?;
        tx.commit().await.map_err(db)?;
        self.get_form(&req.form_id).await
    }
}
