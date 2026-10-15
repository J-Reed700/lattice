//! Grading a submitted form into evidence, and deciding the follow-ups it recommends.
use super::{
    assessment_operation, decode, dimension_for, grade_name, insert_followup, invalid, json,
    parse_format, parse_purpose, stronger_grade, uuid, LearningAssessmentRepository,
    STALE_EVIDENCE_MS,
};
use crate::features::learning::assessment_engine as engine;
use crate::features::learning::dto::*;
use crate::features::learning::persistence::{db, now};
use lattice_core::shared::error::{AppError, Result};
use sqlx::Row;
use std::collections::{HashMap, HashSet};

impl LearningAssessmentRepository {
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
