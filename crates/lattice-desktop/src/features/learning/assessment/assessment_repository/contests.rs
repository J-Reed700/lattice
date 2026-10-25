//! "I think I was right" (section 4.4): a contest records the learner's
//! note and asks for a blind regrade; both grades are kept. After an upheld
//! regrade the learner may override, which is labelled and never becomes
//! observed evidence. Keyed items are reported instead, and leave the bank.
use super::{
    assessment_operation, decode, grade_name, invalid, json, parse_format, uuid,
    LearningAssessmentRepository,
};
use crate::features::learning::dto::*;
use crate::features::learning::persistence::{db, now};
use crate::features::learning::quiz_bank_jobs;
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqliteConnection};
use std::collections::HashMap;

/// A grade as kept in a contest row.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct KeptGrade {
    rationale: Option<LearningGradeRationaleDto>,
    detail: String,
}

/// What the blind regrade concluded.
#[derive(Debug, Clone)]
pub enum Regrade {
    Graded {
        fraction: f64,
        correct: Option<bool>,
        status: LearningAssessmentGradeStatus,
        rationale: LearningGradeRationaleDto,
        response_to_note: Option<String>,
    },
    Failed(String),
}

/// What a regrade job needs to know about its contest.
#[derive(Debug, Clone)]
pub struct ContestInput {
    pub form_id: String,
    pub ordinal: i64,
    pub note: String,
}

fn parse_contest_status(value: &str) -> Result<LearningGradeContestStatus> {
    Ok(match value {
        "open" => LearningGradeContestStatus::Open,
        "revised" => LearningGradeContestStatus::Revised,
        "upheld" => LearningGradeContestStatus::Upheld,
        "overridden" => LearningGradeContestStatus::Overridden,
        "failed" => LearningGradeContestStatus::Failed,
        _ => return Err(AppError::Database("Invalid contest status".into())),
    })
}

fn empty_rationale() -> LearningGradeRationaleDto {
    LearningGradeRationaleDto {
        summary: String::new(),
        criteria: vec![],
        grader_model: String::new(),
        rubric_version: String::new(),
        graded_at: 0,
    }
}

async fn ordinal_of(
    connection: &mut SqliteConnection,
    form_id: &str,
    item_id: &str,
) -> Result<i64> {
    sqlx::query_scalar(
        "SELECT ordinal FROM learning_assessment_form_items WHERE form_id=? AND candidate_id=?",
    )
    .bind(form_id)
    .bind(item_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(db)?
    .ok_or_else(|| invalid("That question is not part of this quiz."))
}

impl LearningAssessmentRepository {
    pub(super) async fn contests_in(
        connection: &mut SqliteConnection,
        form_id: &str,
    ) -> Result<HashMap<i64, LearningGradeContestDto>> {
        let rows = sqlx::query("SELECT * FROM learning_assessment_grade_contests WHERE form_id=?")
            .bind(form_id)
            .fetch_all(&mut *connection)
            .await
            .map_err(db)?;
        let mut contests = HashMap::new();
        for row in rows {
            let first: KeptGrade = decode(row.get("first_results_json")).unwrap_or_default();
            let regrade: Option<KeptGrade> = row
                .get::<Option<String>, _>("regrade_results_json")
                .and_then(|r| decode(&r).ok());
            contests.insert(
                row.get("item_ordinal"),
                LearningGradeContestDto {
                    id: row.get("id"),
                    status: parse_contest_status(row.get("status"))?,
                    note: row.get("note"),
                    first_score: row.get("first_score"),
                    regrade_score: row.get("regrade_score"),
                    regrade_reason: row.get("regrade_reason"),
                    first_rationale: first.rationale.unwrap_or_else(empty_rationale),
                    regrade_rationale: regrade.and_then(|r| r.rationale),
                    response_to_note: row.get("response_to_note"),
                    created_at: row.get("created_at"),
                    decided_at: row.get("decided_at"),
                },
            );
        }
        Ok(contests)
    }

    /// Record the learner's note and queue a blind regrade.
    pub async fn contest(
        &self,
        req: &ContestLearningQuizGradeRequestDto,
        hash: &str,
    ) -> Result<(LearningAssessmentFormDto, Option<String>)> {
        uuid(&req.operation_id, "operation")?;
        uuid(&req.program_id, "program")?;
        uuid(&req.form_id, "form")?;
        if req.note.chars().count() > 2_000 {
            return Err(invalid("A note is at most 2,000 characters."));
        }
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &req.program_id).await?;
        let operation = assessment_operation(
            &req.operation_id,
            &req.program_id,
            Some(&req.form_id),
            "contest",
            hash,
            "Operation ID was reused with a different contest.",
        );
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return Ok((self.get_form(&req.form_id).await?, None));
        }
        Self::active_program(&mut tx, &req.program_id).await?;
        let ordinal = ordinal_of(&mut tx, &req.form_id, &req.item_id).await?;
        let result = sqlx::query("SELECT r.grade_status,r.score,r.rationale_json,r.detail FROM learning_assessment_item_results r JOIN learning_assessment_forms f ON f.id=r.form_id WHERE r.form_id=? AND r.item_ordinal=? AND f.program_id=?")
            .bind(&req.form_id)
            .bind(ordinal)
            .bind(&req.program_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or_else(|| invalid("This answer has no grade to contest yet."))?;
        if !matches!(
            result.get::<String, _>("grade_status").as_str(),
            "provisional" | "uncertain" | "needs_review"
        ) {
            return Err(invalid(
                "Only a grade the model gave can be contested; report a question whose key looks wrong.",
            ));
        }
        let existing: Option<String> = sqlx::query_scalar("SELECT status FROM learning_assessment_grade_contests WHERE form_id=? AND item_ordinal=?")
            .bind(&req.form_id)
            .bind(ordinal)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
        let rationale_json: String = result.get("rationale_json");
        let first = KeptGrade {
            rationale: decode(&rationale_json).ok(),
            detail: result.get("detail"),
        };
        let contest_id = match existing.as_deref() {
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                sqlx::query("INSERT INTO learning_assessment_grade_contests(id,program_id,form_id,item_ordinal,note,status,first_score,first_results_json,created_at) VALUES(?,?,?,?,?,'open',?,?,?)")
                    .bind(&id)
                    .bind(&req.program_id)
                    .bind(&req.form_id)
                    .bind(ordinal)
                    .bind(req.note.trim())
                    .bind(result.get::<f64, _>("score"))
                    .bind(json(&first)?)
                    .bind(now())
                    .execute(&mut *tx)
                    .await
                    .map_err(db)?;
                id
            }
            // A regrade that failed may be asked for again.
            Some("failed") => {
                sqlx::query("UPDATE learning_assessment_grade_contests SET status='open',note=?,regrade_reason=NULL,decided_at=NULL WHERE form_id=? AND item_ordinal=?")
                    .bind(req.note.trim())
                    .bind(&req.form_id)
                    .bind(ordinal)
                    .execute(&mut *tx)
                    .await
                    .map_err(db)?;
                sqlx::query_scalar("SELECT id FROM learning_assessment_grade_contests WHERE form_id=? AND item_ordinal=?")
                    .bind(&req.form_id)
                    .bind(ordinal)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(db)?
            }
            Some(_) => return Err(invalid("This grade was already contested.")),
        };
        let job = quiz_bank_jobs::queue_grading_in(
            &mut tx,
            &req.program_id,
            &req.form_id,
            Some(&contest_id),
            &format!("contest:{}", req.operation_id),
        )
        .await?;
        operation.record(&mut *tx, &contest_id).await?;
        tx.commit().await.map_err(db)?;
        Ok((self.get_form(&req.form_id).await?, Some(job)))
    }

    pub(crate) async fn contest_input(&self, contest_id: &str) -> Result<Option<ContestInput>> {
        let row = sqlx::query("SELECT form_id,item_ordinal,note FROM learning_assessment_grade_contests WHERE id=? AND status='open'")
            .bind(contest_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        Ok(row.map(|r| ContestInput {
            form_id: r.get("form_id"),
            ordinal: r.get("item_ordinal"),
            note: r.get("note"),
        }))
    }

    /// Keep the regrade beside the first grade. A higher regrade revises the
    /// item; otherwise the first grade is upheld, with the regrade's reply.
    pub async fn apply_regrade(
        &self,
        contest_id: &str,
        regrade: Regrade,
        model: &str,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let contest = sqlx::query("SELECT form_id,item_ordinal,first_score,program_id FROM learning_assessment_grade_contests WHERE id=? AND status='open'")
            .bind(contest_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Open contest not found".into()))?;
        let form_id: String = contest.get("form_id");
        let ordinal: i64 = contest.get("item_ordinal");
        let program_id: String = contest.get("program_id");
        Self::writer_lock(&mut tx, &program_id).await?;
        match regrade {
            Regrade::Failed(reason) => {
                sqlx::query("UPDATE learning_assessment_grade_contests SET status='failed',regrade_reason=?,regrade_model=?,decided_at=? WHERE id=?")
                    .bind(reason)
                    .bind(model)
                    .bind(now())
                    .bind(contest_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(db)?;
            }
            Regrade::Graded {
                fraction,
                correct,
                status,
                rationale,
                response_to_note,
            } => {
                let first: f64 = contest.get("first_score");
                let revised = fraction > first + 1e-9;
                sqlx::query("UPDATE learning_assessment_grade_contests SET status=?,regrade_score=?,regrade_results_json=?,regrade_model=?,regrade_reason=?,response_to_note=?,decided_at=? WHERE id=?")
                    .bind(if revised { "revised" } else { "upheld" })
                    .bind(fraction)
                    .bind(json(&KeptGrade {
                        rationale: Some(rationale.clone()),
                        detail: String::new(),
                    })?)
                    .bind(model)
                    .bind(&rationale.summary)
                    .bind(response_to_note)
                    .bind(now())
                    .bind(contest_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(db)?;
                if revised {
                    sqlx::query("UPDATE learning_assessment_item_results SET score=?,correct=?,grade_status=?,feedback=?,points_earned=?*points_possible,rationale_json=? WHERE form_id=? AND item_ordinal=?")
                        .bind(fraction)
                        .bind(correct.map(i64::from))
                        .bind(grade_name(&status))
                        .bind(&rationale.summary)
                        .bind(fraction)
                        .bind(json(&rationale)?)
                        .bind(&form_id)
                        .bind(ordinal)
                        .execute(&mut *tx)
                        .await
                        .map_err(db)?;
                    Self::finalize_in(&mut tx, &form_id).await?;
                }
            }
        }
        tx.commit().await.map_err(db)
    }

    /// Count an item as right after its regrade was upheld. Labelled, kept
    /// apart from the score without overrides, and recorded as uncertain.
    pub async fn override_grade(
        &self,
        req: &OverrideLearningQuizGradeRequestDto,
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
            "override",
            hash,
            "Operation ID was reused with a different override.",
        );
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return self.get_form(&req.form_id).await;
        }
        Self::active_program(&mut tx, &req.program_id).await?;
        let ordinal = ordinal_of(&mut tx, &req.form_id, &req.item_id).await?;
        let changed = sqlx::query("UPDATE learning_assessment_grade_contests SET status='overridden',decided_at=? WHERE form_id=? AND item_ordinal=? AND program_id=? AND status='upheld'")
            .bind(now())
            .bind(&req.form_id)
            .bind(ordinal)
            .bind(&req.program_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        if changed.rows_affected() != 1 {
            return Err(invalid(
                "An override is possible only after a regrade upheld the grade.",
            ));
        }
        sqlx::query("UPDATE learning_assessment_item_results SET learner_override=1 WHERE form_id=? AND item_ordinal=?")
            .bind(&req.form_id)
            .bind(ordinal)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        Self::record_override_evidence_in(&mut tx, &req.form_id, ordinal).await?;
        Self::finalize_in(&mut tx, &req.form_id).await?;
        operation.record(&mut *tx, &()).await?;
        tx.commit().await.map_err(db)?;
        self.get_form(&req.form_id).await
    }

    /// "Report this question": a keyed item whose key looks wrong leaves the
    /// bank for future forms and is listed for the eval.
    pub async fn report_item(
        &self,
        req: &ReportLearningQuizItemRequestDto,
        hash: &str,
    ) -> Result<LearningAssessmentFormDto> {
        uuid(&req.operation_id, "operation")?;
        uuid(&req.program_id, "program")?;
        uuid(&req.form_id, "form")?;
        if req.note.chars().count() > 2_000 {
            return Err(invalid("A note is at most 2,000 characters."));
        }
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &req.program_id).await?;
        let operation = assessment_operation(
            &req.operation_id,
            &req.program_id,
            Some(&req.form_id),
            "report",
            hash,
            "Operation ID was reused with a different report.",
        );
        if operation.seen(&mut *tx).await? {
            tx.commit().await.map_err(db)?;
            return self.get_form(&req.form_id).await;
        }
        Self::active_program(&mut tx, &req.program_id).await?;
        let ordinal = ordinal_of(&mut tx, &req.form_id, &req.item_id).await?;
        let format: String = sqlx::query_scalar(
            "SELECT format FROM learning_assessment_form_items WHERE form_id=? AND ordinal=?",
        )
        .bind(&req.form_id)
        .bind(ordinal)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        if !parse_format(&format)?.is_keyed() {
            return Err(invalid(
                "Written answers are contested, not reported: use “I think I was right”.",
            ));
        }
        sqlx::query("INSERT INTO learning_assessment_item_reports(id,program_id,candidate_id,form_id,note,created_at) VALUES(?,?,?,?,?,?)")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&req.program_id)
            .bind(&req.item_id)
            .bind(&req.form_id)
            .bind(req.note.trim())
            .bind(now())
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query("UPDATE learning_assessment_candidates SET review_status='reported' WHERE id=? AND program_id=?")
            .bind(&req.item_id)
            .bind(&req.program_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        operation.record(&mut *tx, &()).await?;
        tx.commit().await.map_err(db)?;
        self.get_form(&req.form_id).await
    }
}
