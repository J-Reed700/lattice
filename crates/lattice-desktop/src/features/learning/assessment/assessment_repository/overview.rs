//! The Quizzes tab: every lesson quiz, module test and the course exam with
//! its verdict in words, and the course's quiz settings.
use super::bank::bank_state;
use super::{
    assessment_operation, decode, parse_action, parse_dimension, parse_evidence_kind,
    parse_evidence_result, parse_follow_status, parse_grade, parse_reason, scope_kind_name, uuid,
    LearningAssessmentRepository, StoredCandidate,
};
use crate::features::learning::dto::*;
use crate::features::learning::persistence::{db, now};
use crate::features::learning::quiz_bank_jobs::QUIZ_BANK;
use crate::features::learning::quiz_blueprints;
use lattice_core::shared::error::{AppError, Result};
use sqlx::{Row, SqliteConnection};
use std::collections::HashSet;

fn attempt(row: &sqlx::sqlite::SqliteRow) -> Result<LearningQuizAttemptSummaryDto> {
    Ok(LearningQuizAttemptSummaryDto {
        form_id: row.get("form_id"),
        score: row.get("score"),
        passed: row.get::<Option<i64>, _>("passed").map(|p| p != 0),
        grade_status: parse_grade(row.get("grade_status"))?,
        submitted_at: row.get("submitted_at"),
        retake_of_form_id: row.get("retake_of_form_id"),
        overridden_items: row.get("overridden"),
        pending_items: row.get("pending_items"),
        attempt_number: 0,
    })
}

/// The bank job writing a scope's questions now, if any.
async fn bank_job(
    connection: &mut SqliteConnection,
    program_id: &str,
    scope: &LearningQuizScopeDto,
) -> Result<Option<(String, String)>> {
    let row = sqlx::query("SELECT id,status FROM jobs WHERE kind=? AND subject_id=? AND json_extract(requested_json,'$.scope.kind')=? AND json_extract(requested_json,'$.scope.id') IS ? ORDER BY created_at DESC LIMIT 1")
        .bind(QUIZ_BANK)
        .bind(program_id)
        .bind(scope_kind_name(scope.kind))
        .bind(&scope.id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(db)?;
    Ok(row.map(|r| (r.get("id"), r.get("status"))))
}

impl LearningAssessmentRepository {
    async fn quiz_status_in(
        connection: &mut SqliteConnection,
        program_id: &str,
        scope: LearningQuizScopeDto,
        fallback_title: String,
        course: (&quiz_blueprints::CourseFacts, &LearningQuizSettingsDto),
        bank: &[StoredCandidate],
        exposed: &HashSet<String>,
    ) -> Result<LearningQuizStatusDto> {
        let ensured = quiz_blueprints::ensure_blueprint_with(
            connection, program_id, &scope, course.0, course.1,
        )
        .await?;
        let job = bank_job(connection, program_id, &scope).await?;
        let preparing_job_id = job
            .as_ref()
            .filter(|(_, status)| matches!(status.as_str(), "pending" | "running"))
            .map(|(id, _)| id.clone());
        let attempts = sqlx::query("SELECT s.form_id,s.score,s.passed,s.grade_status,s.submitted_at,s.pending_items,f.retake_of_form_id,(SELECT count(*) FROM learning_assessment_item_results r WHERE r.form_id=s.form_id AND r.learner_override=1) overridden FROM learning_assessment_submissions s JOIN learning_assessment_forms f ON f.id=s.form_id WHERE f.program_id=? AND f.scope_kind=? AND f.scope_id IS ? AND f.mode='graded' ORDER BY s.submitted_at DESC")
            .bind(program_id)
            .bind(scope_kind_name(scope.kind))
            .bind(&scope.id)
            .fetch_all(&mut *connection)
            .await
            .map_err(db)?
            .iter()
            .map(attempt)
            .collect::<Result<Vec<_>>>()?;
        let count = attempts.len() as i64;
        let attempts: Vec<LearningQuizAttemptSummaryDto> = attempts
            .into_iter()
            .enumerate()
            .map(|(index, a)| LearningQuizAttemptSummaryDto {
                attempt_number: count - index as i64,
                ..a
            })
            .collect();
        let latest = attempts.first().cloned();
        // Best: the highest completed score; a pending attempt is best only
        // when nothing has completed.
        let best = attempts
            .iter()
            .filter(|a| a.pending_items == 0)
            .max_by(|a, b| a.score.total_cmp(&b.score))
            .or_else(|| attempts.first())
            .cloned();
        let verdict = match &best {
            None => LearningQuizVerdict::NotTaken,
            Some(b) => match b.passed {
                None => LearningQuizVerdict::Pending,
                Some(true) => LearningQuizVerdict::Passed,
                Some(false) => LearningQuizVerdict::NotPassed,
            },
        };
        let active = sqlx::query("SELECT id,status,created_at,updated_at FROM learning_assessment_forms WHERE program_id=? AND scope_kind=? AND scope_id IS ? AND status IN ('active','interrupted') ORDER BY updated_at DESC LIMIT 1")
            .bind(program_id)
            .bind(scope_kind_name(scope.kind))
            .bind(&scope.id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(db)?;
        let active_form_id: Option<String> = active.as_ref().map(|r| r.get("id"));
        let active_form_started_at: Option<i64> = active.as_ref().map(|r| r.get("created_at"));
        let active_form_updated_at: Option<i64> = active.as_ref().map(|r| r.get("updated_at"));
        let active_form_paused = active
            .as_ref()
            .is_some_and(|r| r.get::<String, _>("status") == "interrupted");
        let Some((blueprint, desired)) = ensured else {
            return Ok(LearningQuizStatusDto {
                title: fallback_title,
                bank_status: LearningQuizBankStatus::NotPrepared,
                bank_message: match scope.kind {
                    LearningQuizScopeKind::Lesson => {
                        "Prepare the lesson first; its quiz is written as part of preparation."
                            .into()
                    }
                    _ => "Prepare a lesson first; this test asks about prepared lessons.".into(),
                },
                scope,
                bank_size: 0,
                fresh_items: 0,
                form_size: 0,
                expected_minutes: 0,
                passing_score: 0.0,
                coverage: None,
                active_form_id,
                active_form_started_at,
                active_form_updated_at,
                active_form_paused,
                verdict,
                best,
                latest,
                attempts: attempts.len() as i64,
                preparing_job_id,
            });
        };
        let requirements = quiz_blueprints::requirements_in(connection, &blueprint).await?;
        let state = bank_state(&requirements, bank, exposed);
        let form_size: usize = requirements.iter().map(|r| r.count).sum();
        let bank_status = if preparing_job_id.is_some() {
            LearningQuizBankStatus::Preparing
        } else if state.ready {
            LearningQuizBankStatus::Ready
        } else if job.as_ref().is_some_and(|(_, s)| s == "failed") {
            LearningQuizBankStatus::Failed
        } else {
            LearningQuizBankStatus::NotPrepared
        };
        let bank_message = match bank_status {
            LearningQuizBankStatus::Preparing => "Writing quiz questions.".to_string(),
            LearningQuizBankStatus::Failed => {
                "Questions could not be written last time. Prepare them again.".into()
            }
            LearningQuizBankStatus::NotPrepared => "Questions not prepared yet.".into(),
            LearningQuizBankStatus::Ready if state.size < state.target => format!(
                "{} of {} questions prepared; retakes may repeat questions sooner.",
                state.size, state.target
            ),
            LearningQuizBankStatus::Ready if state.fresh < form_size => {
                "You have seen most of these questions; prepare more for a fresh retake.".into()
            }
            LearningQuizBankStatus::Ready => String::new(),
        };
        Ok(LearningQuizStatusDto {
            scope,
            title: desired.title,
            bank_status,
            bank_message,
            bank_size: state.size as i64,
            fresh_items: state.fresh as i64,
            form_size: form_size as i64,
            expected_minutes: i64::from(desired.expected_minutes),
            passing_score: desired.passing_score,
            coverage: desired
                .coverage
                .map(|(covered, total)| LearningQuizCoverageDto {
                    covered_lessons: covered as i64,
                    total_lessons: total as i64,
                }),
            active_form_id,
            active_form_started_at,
            active_form_updated_at,
            active_form_paused,
            verdict,
            best,
            latest,
            attempts: attempts.len() as i64,
            preparing_job_id,
        })
    }

    pub async fn overview(&self, program_id: &str) -> Result<LearningQuizOverviewDto> {
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
        // Blueprints follow the course: deriving them here keeps every
        // status current without a separate refresh.
        Self::writer_lock(&mut tx, program_id).await?;
        Self::ensure_outcomes(&mut tx, program_id).await?;
        let facts = quiz_blueprints::course_facts_in(&mut tx, program_id).await?;
        for lesson in facts
            .lessons
            .iter()
            .filter(|l| l.prepared && l.outcome_id.is_none())
        {
            quiz_blueprints::ensure_lesson_outcome_in(&mut tx, program_id, &lesson.id, None)
                .await?;
        }
        let settings = quiz_blueprints::settings_in(&mut tx, program_id).await?;
        let bank = Self::bank_in(&mut tx, program_id).await?;
        let exposed = Self::exposed_in(&mut tx, program_id).await?;
        let facts = quiz_blueprints::course_facts_in(&mut tx, program_id).await?;
        let mut modules = Vec::new();
        let mut quizzes_passed = 0;
        let mut quizzes_total = 0;
        for module in &facts.modules {
            let mut lessons = Vec::new();
            for lesson in facts.lessons.iter().filter(|l| l.module_id == module.id) {
                let quiz = Self::quiz_status_in(
                    &mut tx,
                    program_id,
                    LearningQuizScopeDto {
                        kind: LearningQuizScopeKind::Lesson,
                        id: Some(lesson.id.clone()),
                    },
                    format!("Lesson quiz: {}", lesson.title),
                    (&facts, &settings),
                    &bank,
                    &exposed,
                )
                .await?;
                quizzes_total += 1;
                if quiz.verdict == LearningQuizVerdict::Passed {
                    quizzes_passed += 1;
                }
                lessons.push(LearningLessonQuizDto {
                    lesson_id: lesson.id.clone(),
                    quiz,
                });
            }
            let test = Self::quiz_status_in(
                &mut tx,
                program_id,
                LearningQuizScopeDto {
                    kind: LearningQuizScopeKind::Module,
                    id: Some(module.id.clone()),
                },
                format!("Module test: {}", module.title),
                (&facts, &settings),
                &bank,
                &exposed,
            )
            .await?;
            modules.push(LearningModuleQuizzesDto {
                module_id: module.id.clone(),
                test,
                lessons,
            });
        }
        let exam = Self::quiz_status_in(
            &mut tx,
            program_id,
            LearningQuizScopeDto {
                kind: LearningQuizScopeKind::Course,
                id: None,
            },
            format!("Course exam: {}", facts.program_title),
            (&facts, &settings),
            &bank,
            &exposed,
        )
        .await?;
        let outcomes = Self::outcomes_in(&mut tx, program_id).await?;
        let evidence = Self::evidence_in(&mut tx, program_id).await?;
        let follow_ups = Self::follow_ups_in(&mut tx, program_id).await?;
        tx.commit().await.map_err(db)?;
        Ok(LearningQuizOverviewDto {
            program_id: program_id.into(),
            settings,
            exam,
            modules,
            quizzes_passed,
            quizzes_total,
            outcomes,
            evidence,
            follow_ups,
        })
    }

    pub async fn update_settings(
        &self,
        req: &UpdateLearningQuizSettingsRequestDto,
        hash: &str,
    ) -> Result<()> {
        uuid(&req.operation_id, "operation")?;
        uuid(&req.program_id, "program")?;
        let s = &req.settings;
        if [s.quiz_pass, s.test_pass, s.exam_pass]
            .iter()
            .any(|p| !p.is_finite() || *p <= 0.0 || *p > 1.0)
        {
            return Err(super::invalid(
                "A pass mark is a share of the points between 1 % and 100 %.",
            ));
        }
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &req.program_id).await?;
        let operation = assessment_operation(
            &req.operation_id,
            &req.program_id,
            None,
            "update_quiz_settings",
            hash,
            "Operation ID was reused with different settings.",
        );
        if operation.seen(&mut *tx).await? {
            return tx.commit().await.map_err(db);
        }
        Self::active_program(&mut tx, &req.program_id).await?;
        sqlx::query("INSERT INTO learning_quiz_settings(program_id,quiz_pass,test_pass,exam_pass,written_answers,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(program_id) DO UPDATE SET quiz_pass=excluded.quiz_pass,test_pass=excluded.test_pass,exam_pass=excluded.exam_pass,written_answers=excluded.written_answers,updated_at=excluded.updated_at")
            .bind(&req.program_id)
            .bind(s.quiz_pass)
            .bind(s.test_pass)
            .bind(s.exam_pass)
            .bind(i64::from(s.written_answers))
            .bind(now())
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        operation.record(&mut *tx, &()).await?;
        tx.commit().await.map_err(db)
    }

    /// The course's module outcomes, created from the modules' outcome lists
    /// when missing.
    pub async fn module_outcomes(
        &self,
        program_id: &str,
    ) -> Result<Vec<LearningOutcomeDefinitionDto>> {
        uuid(program_id, "program")?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::ensure_outcomes(&mut tx, program_id).await?;
        let outcomes = Self::outcomes_in(&mut tx, program_id)
            .await?
            .into_iter()
            .filter(|o| o.lesson_id.is_none())
            .collect();
        tx.commit().await.map_err(db)?;
        Ok(outcomes)
    }

    async fn outcomes_in(
        connection: &mut SqliteConnection,
        program_id: &str,
    ) -> Result<Vec<LearningOutcomeDefinitionDto>> {
        sqlx::query("SELECT * FROM learning_outcome_definitions WHERE program_id=? ORDER BY module_id,ordinal,id")
            .bind(program_id)
            .fetch_all(&mut *connection)
            .await
            .map_err(db)?
            .into_iter()
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

    async fn evidence_in(
        connection: &mut SqliteConnection,
        program_id: &str,
    ) -> Result<Vec<LearningEvidenceEventDto>> {
        sqlx::query("SELECT * FROM learning_evidence_events WHERE program_id=? ORDER BY observed_at DESC,id LIMIT 1000")
            .bind(program_id)
            .fetch_all(&mut *connection)
            .await
            .map_err(db)?
            .into_iter()
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

    async fn follow_ups_in(
        connection: &mut SqliteConnection,
        program_id: &str,
    ) -> Result<Vec<LearningFollowUpRecommendationDto>> {
        sqlx::query("SELECT * FROM learning_follow_up_recommendations WHERE program_id=? ORDER BY created_at DESC,id LIMIT 500")
            .bind(program_id)
            .fetch_all(&mut *connection)
            .await
            .map_err(db)?
            .into_iter()
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
