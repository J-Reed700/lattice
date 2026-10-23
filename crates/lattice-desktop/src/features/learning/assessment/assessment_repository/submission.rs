//! Two-phase submission (section 4.1): keyed answers are graded in the submit
//! transaction; written answers and short-answer near-misses wait as
//! `pending` for the grading job, and the verdict waits for them.
use super::{
    assessment_operation, candidate_in, decode, dimension_for, grade_name, insert_followup,
    invalid, json, parse_format, parse_grade, parse_mode, parse_purpose, stronger_grade, uuid,
    LearningAssessmentRepository, STALE_EVIDENCE_MS,
};
use crate::features::learning::assessment_engine as engine;
use crate::features::learning::dto::*;
use crate::features::learning::persistence::{db, now};
use crate::features::learning::quiz_bank_jobs;
use lattice_core::shared::error::{AppError, Result};
use sqlx::{Row, SqliteConnection};

/// What grading decided for one pending item.
#[derive(Debug, Clone)]
pub enum ItemGrade {
    Graded {
        ordinal: i64,
        fraction: f64,
        correct: Option<bool>,
        status: LearningAssessmentGradeStatus,
        detail: String,
        rationale: Option<LearningGradeRationaleDto>,
    },
    /// "Couldn't grade yet": the item stays pending, with the reason.
    Unexplained { ordinal: i64, reason: String },
}

/// The form facts evidence needs.
struct FormFacts {
    program_id: String,
    purpose: engine::LearningAssessmentPurpose,
    practice: bool,
    allowed_aids: Vec<String>,
}

async fn form_facts(connection: &mut SqliteConnection, form_id: &str) -> Result<FormFacts> {
    let h = sqlx::query("SELECT program_id,purpose,mode,allowed_aids_json FROM learning_assessment_forms WHERE id=?")
        .bind(form_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Quiz not found".into()))?;
    Ok(FormFacts {
        program_id: h.get("program_id"),
        purpose: parse_purpose(h.get("purpose"))?,
        practice: parse_mode(h.get("mode"))? == LearningQuizMode::Practice,
        allowed_aids: decode(h.get("allowed_aids_json")).unwrap_or_default(),
    })
}

/// Evidence for one graded item, and the follow-ups it suggests.
#[allow(clippy::too_many_arguments)]
async fn record_evidence(
    connection: &mut SqliteConnection,
    facts: &FormFacts,
    form_id: &str,
    ordinal: i64,
    fraction: f64,
    status: &LearningAssessmentGradeStatus,
    observation: &str,
    quote: Option<&str>,
    overridden: bool,
) -> Result<()> {
    let item = sqlx::query("SELECT format,difficulty FROM learning_assessment_form_items WHERE form_id=? AND ordinal=?")
        .bind(form_id)
        .bind(ordinal)
        .fetch_one(&mut *connection)
        .await
        .map_err(db)?;
    let format = parse_format(item.get("format"))?;
    let difficulty = item.get::<i64, _>("difficulty").clamp(1, 5) as u8;
    let assistance: Option<String> = sqlx::query_scalar("SELECT assistance_json FROM learning_assessment_response_revisions WHERE form_id=? AND item_ordinal=? ORDER BY revision DESC LIMIT 1")
        .bind(form_id)
        .bind(ordinal)
        .fetch_optional(&mut *connection)
        .await
        .map_err(db)?;
    let used: Vec<String> = assistance
        .as_deref()
        .map(|a| decode(a).unwrap_or_default())
        .unwrap_or_default();
    let outcomes = sqlx::query("SELECT fio.outcome_id,o.lesson_id FROM learning_assessment_form_item_outcomes fio LEFT JOIN learning_outcome_definitions o ON o.id=fio.outcome_id WHERE fio.form_id=? AND fio.item_ordinal=? ORDER BY fio.outcome_id")
        .bind(form_id)
        .bind(ordinal)
        .fetch_all(&mut *connection)
        .await
        .map_err(db)?;
    let uncertain = overridden
        || matches!(
            status,
            LearningAssessmentGradeStatus::Uncertain | LearningAssessmentGradeStatus::NeedsReview
        );
    let result = if uncertain {
        "uncertain"
    } else if fraction >= 0.5 {
        "observed"
    } else {
        "not_observed"
    };
    let dimension = dimension_for(format, difficulty, facts.purpose);
    let source_kind = if facts.practice {
        "practice"
    } else {
        "assessment"
    };
    let assistance_json = serde_json::json!({"allowedAids": facts.allowed_aids, "used": used});
    for outcome in outcomes {
        let outcome_id: String = outcome.get("outcome_id");
        let lesson_id: Option<String> = outcome.get("lesson_id");
        let event = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO learning_evidence_events(id,program_id,outcome_id,source_kind,source_id,dimension,result,score,observation,evidence_quote,assistance_json,observed_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&event)
            .bind(&facts.program_id)
            .bind(&outcome_id)
            .bind(source_kind)
            .bind(form_id)
            .bind(dimension)
            .bind(result)
            .bind((!uncertain).then_some(fraction))
            .bind(observation.chars().take(2_000).collect::<String>())
            .bind(quote)
            .bind(json(&assistance_json)?)
            .bind(now())
            .execute(&mut *connection)
            .await
            .map_err(db)?;
        let events = std::slice::from_ref(&event);
        if result == "not_observed" {
            let latest: Option<i64> = sqlx::query_scalar("SELECT max(observed_at) FROM learning_evidence_events WHERE program_id=? AND outcome_id=? AND source_id<>?")
                .bind(&facts.program_id)
                .bind(&outcome_id)
                .bind(form_id)
                .fetch_one(&mut *connection)
                .await
                .map_err(db)?;
            if latest.is_some_and(|date| now() - date > STALE_EVIDENCE_MS) {
                insert_followup(connection, &facts.program_id, Some(&outcome_id), "stale_evidence", "Earlier evidence for this outcome is old enough to benefit from a fresh check.", "assessment", Some(form_id), events).await?;
            }
        }
        if result == "observed" && !used.is_empty() {
            insert_followup(
                connection,
                &facts.program_id,
                Some(&outcome_id),
                "assisted_success",
                "This answer used recorded help; consider a short check without it.",
                "assessment",
                Some(form_id),
                events,
            )
            .await?;
        }
        if uncertain && !overridden {
            insert_followup(connection, &facts.program_id, Some(&outcome_id), "uncertain_grade", "This answer could not be graded with confidence; review it or answer another question on the same point.", "assessment", Some(form_id), events).await?;
        } else if result == "not_observed" && dimension == "transfer" {
            insert_followup(connection, &facts.program_id, Some(&outcome_id), "low_transfer", "The changed-context answer did not yet show this outcome; try a new transfer example.", "practice", Some(&outcome_id), events).await?;
        } else if result == "not_observed" {
            insert_followup(
                connection,
                &facts.program_id,
                Some(&outcome_id),
                "missed_outcome",
                "This answer did not yet show this outcome; revisit the lesson and try again.",
                "lesson",
                lesson_id.as_deref(),
                events,
            )
            .await?;
        }
    }
    Ok(())
}

impl LearningAssessmentRepository {
    /// Freeze the answers and grade every keyed item now. Returns the form
    /// and, when items wait for the model, the grading job to start.
    pub async fn submit(
        &self,
        req: &MutateLearningAssessmentFormRequestDto,
        hash: &str,
    ) -> Result<(LearningAssessmentFormDto, Option<String>)> {
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
            "Operation ID was reused with different quiz data.",
        );
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return Ok((self.get_form(&req.form_id).await?, None));
        }
        Self::active_program(&mut tx, &req.program_id).await?;
        let h = sqlx::query(
            "SELECT status,revision FROM learning_assessment_forms WHERE id=? AND program_id=?",
        )
        .bind(&req.form_id)
        .bind(&req.program_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Quiz not found".into()))?;
        if h.get::<String, _>("status") == "submitted" {
            return Err(invalid("This quiz was already submitted."));
        }
        if h.get::<i64, _>("revision") != req.expected_revision {
            return Err(invalid("This quiz changed; reload and retry."));
        }
        let form = Self::form_in(&mut tx, &req.form_id).await?;
        let timestamp = now();
        sqlx::query("INSERT INTO learning_assessment_submissions(form_id,program_id,operation_id,payload_hash,score,passed,grade_status,grader_model,grader_disagreement_json,feedback,submitted_at,pending_items) VALUES(?,?,?,?,0,NULL,'deterministic',NULL,'[]','',?,0)")
            .bind(&req.form_id)
            .bind(&req.program_id)
            .bind(&req.operation_id)
            .bind(hash)
            .bind(timestamp)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        let facts = form_facts(&mut tx, &req.form_id).await?;
        let mut pending = 0usize;
        for (ordinal, item) in form.items.iter().enumerate() {
            let ordinal = ordinal as i64;
            let stored = candidate_in(&mut tx, &req.program_id, &item.id).await?;
            let order: Vec<usize> = decode(
                &sqlx::query_scalar::<_, String>("SELECT option_order_json FROM learning_assessment_form_items WHERE form_id=? AND ordinal=?")
                    .bind(&req.form_id)
                    .bind(ordinal)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(db)?,
            )?;
            let response = engine::LearningAssessmentResponse {
                item_id: item.id.clone(),
                selected_indices: item.selected_indices.clone(),
                text: item.text_response.clone(),
                blank_responses: item.blank_responses.clone(),
                matches: item.matches.clone(),
                ordered_values: item.ordered_values.clone(),
                artifact_json: item.artifact_json.clone(),
            };
            let graded = if item.format.is_keyed() {
                match engine::grade_keyed(
                    &stored.candidate,
                    &engine::to_authored(&response, &order),
                )? {
                    engine::KeyedOutcome::Graded(grade) => Some(grade),
                    engine::KeyedOutcome::NeedsJudge => None,
                }
            } else if !response.is_answered() {
                Some(engine::KeyedGrade {
                    fraction: 0.0,
                    correct: false,
                    detail: engine::NO_ANSWER.into(),
                })
            } else {
                None
            };
            match graded {
                Some(grade) => {
                    sqlx::query("INSERT INTO learning_assessment_item_results(form_id,item_ordinal,score,correct,grade_status,feedback,points_earned,points_possible,detail,rationale_json) VALUES(?,?,?,?,'deterministic',?,?,?,?,'{}')")
                        .bind(&req.form_id)
                        .bind(ordinal)
                        .bind(grade.fraction)
                        .bind(item.format.is_keyed().then_some(i64::from(grade.correct)))
                        .bind(&grade.detail)
                        .bind(grade.fraction * item.points)
                        .bind(item.points)
                        .bind(&grade.detail)
                        .execute(&mut *tx)
                        .await
                        .map_err(db)?;
                    record_evidence(
                        &mut tx,
                        &facts,
                        &req.form_id,
                        ordinal,
                        grade.fraction,
                        &LearningAssessmentGradeStatus::Deterministic,
                        if grade.detail.is_empty() {
                            "Answered correctly."
                        } else {
                            &grade.detail
                        },
                        None,
                        false,
                    )
                    .await?;
                }
                None => {
                    pending += 1;
                    sqlx::query("INSERT INTO learning_assessment_item_results(form_id,item_ordinal,score,correct,grade_status,feedback,points_earned,points_possible,detail,rationale_json) VALUES(?,?,0,NULL,'pending','',0,?,'','{}')")
                        .bind(&req.form_id)
                        .bind(ordinal)
                        .bind(item.points)
                        .execute(&mut *tx)
                        .await
                        .map_err(db)?;
                }
            }
        }
        let updated = sqlx::query("UPDATE learning_assessment_forms SET status='submitted',revision=revision+1,submitted_at=?,updated_at=? WHERE id=? AND program_id=? AND revision=? AND status IN ('active','interrupted')").bind(timestamp).bind(timestamp).bind(&req.form_id).bind(&req.program_id).bind(req.expected_revision).execute(&mut *tx).await.map_err(db)?;
        if updated.rows_affected() != 1 {
            return Err(invalid("This quiz changed; reload and retry."));
        }
        Self::finalize_in(&mut tx, &req.form_id).await?;
        let job = if pending > 0 {
            Some(
                quiz_bank_jobs::queue_grading_in(
                    &mut tx,
                    &req.program_id,
                    &req.form_id,
                    None,
                    &format!("submit:{}", req.operation_id),
                )
                .await?,
            )
        } else {
            None
        };
        operation.record(&mut *tx, &()).await?;
        tx.commit().await.map_err(db)?;
        Ok((self.get_form(&req.form_id).await?, job))
    }

    /// Write the grading job's results for pending items, with their evidence,
    /// and settle the score and verdict when nothing is left pending.
    pub async fn apply_grades(
        &self,
        form_id: &str,
        grades: &[ItemGrade],
        grader_model: &str,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let facts = form_facts(&mut tx, form_id).await?;
        Self::writer_lock(&mut tx, &facts.program_id).await?;
        for grade in grades {
            match grade {
                ItemGrade::Graded {
                    ordinal,
                    fraction,
                    correct,
                    status,
                    detail,
                    rationale,
                } => {
                    let changed = sqlx::query("UPDATE learning_assessment_item_results SET score=?,correct=?,grade_status=?,feedback=?,points_earned=?*points_possible,detail=?,rationale_json=? WHERE form_id=? AND item_ordinal=? AND grade_status='pending'")
                        .bind(fraction)
                        .bind(correct.map(i64::from))
                        .bind(grade_name(status))
                        .bind(rationale.as_ref().map(|r| r.summary.clone()).unwrap_or_default())
                        .bind(fraction)
                        .bind(detail)
                        .bind(match rationale {
                            Some(r) => json(r)?,
                            None => "{}".into(),
                        })
                        .bind(form_id)
                        .bind(ordinal)
                        .execute(&mut *tx)
                        .await
                        .map_err(db)?;
                    if changed.rows_affected() == 1 {
                        let quote = rationale
                            .as_ref()
                            .and_then(|r| {
                                r.criteria.iter().flat_map(|c| c.your_words.first()).next()
                            })
                            .cloned();
                        let observation = rationale
                            .as_ref()
                            .map(|r| r.summary.clone())
                            .unwrap_or_else(|| detail.clone());
                        record_evidence(
                            &mut tx,
                            &facts,
                            form_id,
                            *ordinal,
                            *fraction,
                            status,
                            &observation,
                            quote.as_deref(),
                            false,
                        )
                        .await?;
                    }
                }
                ItemGrade::Unexplained { ordinal, reason } => {
                    sqlx::query("UPDATE learning_assessment_item_results SET detail=?,feedback=? WHERE form_id=? AND item_ordinal=? AND grade_status='pending'")
                        .bind(reason)
                        .bind(reason)
                        .bind(form_id)
                        .bind(ordinal)
                        .execute(&mut *tx)
                        .await
                        .map_err(db)?;
                }
            }
        }
        sqlx::query("UPDATE learning_assessment_submissions SET grader_model=? WHERE form_id=?")
            .bind(grader_model)
            .bind(form_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        Self::finalize_in(&mut tx, form_id).await?;
        tx.commit().await.map_err(db)
    }

    /// Recompute a submission from its item results: score (an overridden
    /// item counts as full credit), pending count, status and verdict.
    pub(crate) async fn finalize_in(
        connection: &mut SqliteConnection,
        form_id: &str,
    ) -> Result<()> {
        let rows = sqlx::query("SELECT grade_status,points_earned,points_possible,learner_override FROM learning_assessment_item_results WHERE form_id=?")
            .bind(form_id)
            .fetch_all(&mut *connection)
            .await
            .map_err(db)?;
        let passing: f64 =
            sqlx::query_scalar("SELECT passing_score FROM learning_assessment_forms WHERE id=?")
                .bind(form_id)
                .fetch_one(&mut *connection)
                .await
                .map_err(db)?;
        let mut earned = 0.0;
        let mut earned_without = 0.0;
        let mut possible = 0.0;
        let mut pending = 0i64;
        let mut overridden = false;
        let mut status = LearningAssessmentGradeStatus::Deterministic;
        for row in &rows {
            let item_status = parse_grade(row.get("grade_status"))?;
            let points_possible: f64 = row.get("points_possible");
            let points_earned: f64 = row.get("points_earned");
            let override_ = row.get::<i64, _>("learner_override") != 0;
            if item_status == LearningAssessmentGradeStatus::Pending {
                pending += 1;
            }
            overridden |= override_;
            possible += points_possible;
            earned_without += points_earned;
            earned += if override_ {
                points_possible
            } else {
                points_earned
            };
            status = stronger_grade(status, item_status);
        }
        let ratio = |e: f64| {
            if possible > 0.0 {
                ((e / possible) * 10_000.0).round() / 10_000.0
            } else {
                0.0
            }
        };
        let score = ratio(earned).clamp(0.0, 1.0);
        let passed = (pending == 0).then_some(score + 1e-9 >= passing);
        let feedback = match status {
            LearningAssessmentGradeStatus::Pending => "Written answers are waiting to be graded; the result follows once they are.",
            LearningAssessmentGradeStatus::Uncertain | LearningAssessmentGradeStatus::NeedsReview => "Some answers were graded with doubts the grader could not resolve; you can ask for a regrade.",
            LearningAssessmentGradeStatus::Provisional => "Written answers were graded by the model against their rubrics, with the reasons shown.",
            LearningAssessmentGradeStatus::Deterministic => "Every answer was checked against its key.",
        };
        let disagreement: Vec<String> =
            if matches!(status, LearningAssessmentGradeStatus::Deterministic) {
                vec![]
            } else {
                vec![
                    "Written-answer grades are the model's and have not been checked by a person."
                        .into(),
                ]
            };
        sqlx::query("UPDATE learning_assessment_submissions SET score=?,passed=?,grade_status=?,pending_items=?,score_without_overrides=?,feedback=?,grader_disagreement_json=? WHERE form_id=?")
            .bind(score)
            .bind(passed.map(i64::from))
            .bind(grade_name(&status))
            .bind(pending)
            .bind(overridden.then(|| ratio(earned_without).clamp(0.0, 1.0)))
            .bind(feedback)
            .bind(json(&disagreement)?)
            .bind(form_id)
            .execute(&mut *connection)
            .await
            .map_err(db)?;
        Ok(())
    }

    pub async fn decide_follow_up(
        &self,
        req: &DecideLearningFollowUpRequestDto,
        accept: bool,
        hash: &str,
    ) -> Result<()> {
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
            return tx.commit().await.map_err(db);
        }
        Self::active_program(&mut tx, &req.program_id).await?;
        let status = if accept { "accepted" } else { "dismissed" };
        let result = sqlx::query("UPDATE learning_follow_up_recommendations SET status=?,decided_at=? WHERE id=? AND program_id=? AND status='pending'").bind(status).bind(now()).bind(&req.follow_up_id).bind(&req.program_id).execute(&mut *tx).await.map_err(db)?;
        if result.rows_affected() != 1 {
            return Err(AppError::NotFound("Pending follow-up not found".into()));
        }
        operation.record(&mut *tx, &req.follow_up_id).await?;
        tx.commit().await.map_err(db)
    }

    /// Evidence for an item the learner overrode: never `observed`.
    pub(crate) async fn record_override_evidence_in(
        connection: &mut SqliteConnection,
        form_id: &str,
        ordinal: i64,
    ) -> Result<()> {
        let facts = form_facts(connection, form_id).await?;
        record_evidence(
            connection,
            &facts,
            form_id,
            ordinal,
            1.0,
            &LearningAssessmentGradeStatus::Uncertain,
            "Counted as right by the learner's override after an upheld regrade.",
            None,
            true,
        )
        .await
    }
}
