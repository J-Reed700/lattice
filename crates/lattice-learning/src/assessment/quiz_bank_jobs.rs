//! The two quiz jobs on the learning job runner (sections 4.1 and 5.4):
//! `quiz_bank` writes a quiz's question bank, `quiz_grading` grades written
//! answers and short-answer near-misses, and contests' blind regrades. Both
//! are stoppable, size every model call to the window, and never time out; a
//! stopped grading job leaves its items pending, never failed. A lesson's
//! publication queues a `quiz_bank` job running [`lesson_bank_stage`].
use crate::assessment_engine::{self as engine, LearningItemFormat};
use crate::assessment_generation::{
    self as generation, AuthoredItem, AuthoringNeed, MaterialPassage, RawGrade, ShortAnswer,
    StoredItem, WrittenAnswer,
};
use crate::assessment_repository::{BankItem, ItemGrade, LearningAssessmentRepository};
use crate::curriculum_repository::lesson_rank_in;
use crate::dto::{
    LearningAssessmentGradeStatus, LearningQuizScopeDto, LearningQuizScopeKind, LessonAnswerKey,
    LessonQuestion, LessonQuestionKind,
};
use crate::grade_rationale::{self, Checked, Grader, Reference};
use crate::persistence::{db, now};
use crate::quiz_blueprints::{self, CourseFacts, LessonFacts};
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::PinnedModel;
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Transactions;
use lattice_core::shared::runtime::jobs::{
    Backoff, JobContext, JobFailure, JobHandler, JobKindConfig, JobOutcome, JobRecord, JobStore,
    NewJob, RecoveryPolicy,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{SqliteConnection, SqlitePool};
use std::sync::Arc;

pub const QUIZ_BANK: &str = "learning.quiz_bank";
pub const QUIZ_GRADING: &str = "learning.quiz_grading";

/// The checkpoint a lesson's quiz job saves its stage's output under.
pub(crate) const LESSON_BANK_CHECKPOINT: &str = "lesson-bank-v1";

/// The Learning model, pinned for the whole job.
pub type ModelLoader = Arc<dyn Fn() -> BoxFuture<'static, Result<PinnedModel>> + Send + Sync>;

/// One job per course at a time for each kind; a running job resumes after a
/// restart, and a disconnected model defers it rather than failing it.
pub fn quiz_job_config() -> JobKindConfig {
    JobKindConfig::new(RecoveryPolicy::Requeue)
        .exclusive_per_subject()
        .ranked()
        .backoff(Backoff::default())
}

fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// A UUID derived from text, so a repeated request finds the same job.
fn derived_uuid(seed: &str) -> String {
    let hash = Sha256::digest(seed.as_bytes());
    let mut bytes = [0u8; 16];
    for (slot, byte) in bytes.iter_mut().zip(hash.iter()) {
        *slot = *byte;
    }
    uuid::Builder::from_random_bytes(bytes)
        .into_uuid()
        .to_string()
}

async fn queue_in(
    connection: &mut SqliteConnection,
    kind: &str,
    program_id: &str,
    operation_id: String,
    requested: serde_json::Value,
    total: u32,
    message: &str,
    rank: Option<i64>,
) -> Result<String> {
    let admitted = JobStore::submit_with_rank_in(
        connection,
        &NewJob {
            kind: kind.into(),
            subject_id: Some(program_id.into()),
            operation_id,
            payload_hash: digest(&requested.to_string()),
            requested,
            progress_total: total.max(1),
            message: message.into(),
        },
        rank,
    )
    .await?;
    if admitted.created {
        let revision = repository::program_revision_in(connection, program_id).await?;
        repository::insert_learning_job_in(connection, &admitted.id, program_id, revision.max(0))
            .await?;
    }
    Ok(admitted.id)
}

/// Queue the grading of a submitted quiz's pending items, or (with a
/// contest) a blind regrade. The caller tells the runtime once it commits.
pub(crate) async fn queue_grading_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    form_id: &str,
    contest_id: Option<&str>,
    seed: &str,
) -> Result<String> {
    let pending = repository::pending_item_count_in(connection, form_id).await?;
    queue_in(
        connection,
        QUIZ_GRADING,
        program_id,
        derived_uuid(&format!("{QUIZ_GRADING}:{seed}")),
        json!({"programId": program_id, "formId": form_id, "contestId": contest_id}),
        if contest_id.is_some() {
            1
        } else {
            pending.max(1) as u32
        },
        if contest_id.is_some() {
            "Waiting to regrade"
        } else {
            "Waiting to grade written answers"
        },
        None,
    )
    .await
}

/// Queue writing a quiz's questions: its missing bank, or with `top_up` one
/// more form's worth.
pub async fn queue_bank(
    pool: &SqlitePool,
    operation_id: &str,
    program_id: &str,
    scope: &LearningQuizScopeDto,
    top_up: bool,
) -> Result<String> {
    quiz_blueprints::validate_scope(scope)?;
    let mut tx = pool.begin_write().await.map_err(db)?;
    let needs =
        LearningAssessmentRepository::bank_needs_in(&mut tx, program_id, scope, top_up).await?;
    let rank = match (&scope.kind, &scope.id) {
        (LearningQuizScopeKind::Lesson, Some(lesson)) => lesson_rank_in(&mut tx, lesson).await?,
        _ => None,
    };
    let id = queue_in(
        &mut tx,
        QUIZ_BANK,
        program_id,
        operation_id.to_string(),
        json!({"programId": program_id, "scope": scope, "topUp": top_up}),
        needs.len() as u32,
        "Waiting to write quiz questions",
        rank,
    )
    .await?;
    tx.commit().await.map_err(db)?;
    Ok(id)
}

/// Queue writing a published lesson's quiz bank, in the transaction that
/// publishes the lesson. The caller tells the runtime once it commits.
pub(crate) async fn queue_lesson_bank_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    lesson_id: &str,
) -> Result<String> {
    let rank = lesson_rank_in(connection, lesson_id).await?;
    queue_in(
        connection,
        QUIZ_BANK,
        program_id,
        uuid::Uuid::new_v4().to_string(),
        json!({
            "programId": program_id,
            "scope": {"kind": "lesson", "id": lesson_id},
            "topUp": false,
            "lessonStage": true,
        }),
        1,
        "Waiting to write quiz questions",
        rank,
    )
    .await
}

/// What writing a bank produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BankReport {
    pub written: usize,
    /// Items the checks or the blinded key review left out, with why.
    pub dropped: Vec<String>,
}

/// Write a quiz's missing questions with `llm` now, exactly as the
/// `quiz_bank` job does (per-format authoring, code checks, shuffled options,
/// blinded key review), without the job runner. For the course eval's live
/// hook and other callers that hold a model already.
pub async fn prepare_bank_now(
    pool: &SqlitePool,
    llm: &dyn LLMPort,
    program_id: &str,
    scope: &LearningQuizScopeDto,
    top_up: bool,
) -> Result<BankReport> {
    let needs = {
        let mut tx = pool.begin_write().await.map_err(db)?;
        let needs =
            LearningAssessmentRepository::bank_needs_in(&mut tx, program_id, scope, top_up).await?;
        tx.commit().await.map_err(db)?;
        needs
    };
    let mut report = BankReport::default();
    for need in &needs {
        let authored = generation::author_items(llm, need).await?;
        report.dropped.extend(authored.dropped);
        let items: Vec<BankItem> = authored
            .items
            .into_iter()
            .map(|item| BankItem {
                item: item.candidate,
                source_quote: item.source_quote,
                lesson_id: item.lesson_id,
                origin: "bank",
                review_status: "agreed",
            })
            .collect();
        let mut tx = pool.begin_write().await.map_err(db)?;
        report.written += crate::assessment_repository::insert_bank_items_in(
            &mut tx,
            program_id,
            &items,
            Some(llm.model_name()),
        )
        .await?;
        tx.commit().await.map_err(db)?;
    }
    Ok(report)
}

fn requested_str<'a>(job: &'a JobRecord, field: &str) -> Result<&'a str> {
    job.requested
        .get(field)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| AppError::InvalidInput(format!("The quiz job has no {field}.")))
}

/// Writes a quiz's question bank.
#[derive(Clone)]
pub struct QuizBankWorker {
    pub pool: SqlitePool,
    pub load_llm: ModelLoader,
}

#[async_trait::async_trait]
impl JobHandler for QuizBankWorker {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        let job = context.job();
        let program_id = requested_str(job, "programId")?.to_string();
        let scope: LearningQuizScopeDto =
            serde_json::from_value(job.requested.get("scope").cloned().unwrap_or_default())
                .map_err(|_| AppError::InvalidInput("The quiz job names no quiz.".into()))?;
        let top_up = job
            .requested
            .get("topUp")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let needs = {
            let mut tx = self.pool.begin_write().await.map_err(db)?;
            let needs =
                LearningAssessmentRepository::bank_needs_in(&mut tx, &program_id, &scope, top_up)
                    .await?;
            tx.commit().await.map_err(db)?;
            needs
        };
        let lesson_stage = job
            .requested
            .get("lessonStage")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if lesson_stage && !needs.is_empty() {
            return self.run_lesson_stage(context, &program_id, &scope).await;
        }
        if needs.is_empty() {
            return Ok(JobOutcome::Completed {
                result_ref: "quiz_bank:0".into(),
                message: "This quiz's questions are ready.".into(),
            });
        }
        let Some(pin) = context.until_cancelled((self.load_llm)()).await else {
            return Ok(JobOutcome::Stopped);
        };
        let pin = pin?;
        let llm = pin.llm.as_ref();
        let total = context.job().progress_total;
        let mut written = 0usize;
        let mut dropped = 0usize;
        for (index, need) in needs.iter().enumerate() {
            context
                .progress(
                    (index as u32).min(total),
                    &format!("Writing quiz questions ({} of {})", index + 1, needs.len()),
                    None,
                )
                .await?;
            let Some(authored) = context
                .until_cancelled(generation::author_items(llm, need))
                .await
            else {
                return Ok(JobOutcome::Stopped);
            };
            let authored = authored?;
            dropped += authored.dropped.len();
            let items: Vec<BankItem> = authored
                .items
                .into_iter()
                .map(|item| BankItem {
                    item: item.candidate,
                    source_quote: item.source_quote,
                    lesson_id: item.lesson_id,
                    origin: "bank",
                    review_status: "agreed",
                })
                .collect();
            let mut tx = self.pool.begin_write().await.map_err(db)?;
            written += crate::assessment_repository::insert_bank_items_in(
                &mut tx,
                &program_id,
                &items,
                Some(llm.model_name()),
            )
            .await?;
            tx.commit().await.map_err(db)?;
        }
        Ok(JobOutcome::Completed {
            result_ref: format!("quiz_bank:{written}"),
            message: if dropped == 0 {
                format!("{written} questions written.")
            } else {
                format!(
                    "{written} questions written; {dropped} failed the checks and were left out."
                )
            },
        })
    }

    fn failure(&self, job: &JobRecord, error: &AppError) -> JobFailure {
        crate::curriculum_repository::lesson_failure(job, error)
    }
}

impl QuizBankWorker {
    /// A published lesson's bank: its stage runs once on the lesson's own
    /// rows, and its output is saved before it is inserted.
    async fn run_lesson_stage(
        &self,
        context: &JobContext,
        program_id: &str,
        scope: &LearningQuizScopeDto,
    ) -> Result<JobOutcome> {
        let lesson_id = scope
            .id
            .as_deref()
            .ok_or_else(|| AppError::InvalidInput("The quiz job names no lesson.".into()))?;
        context
            .progress(0, "Writing this lesson's quiz questions", None)
            .await?;
        let saved: Option<LessonBank> = context
            .checkpoint(LESSON_BANK_CHECKPOINT)
            .await?
            .and_then(|value| serde_json::from_value(value).ok());
        let bank = match saved {
            Some(bank) => bank,
            None => {
                let (title, objective, material, seeds) = {
                    let mut tx = self.pool.begin_read().await.map_err(db)?;
                    let (title, objective) =
                        repository::lesson_title_objective_in(&mut tx, program_id, lesson_id)
                            .await?
                            .ok_or_else(|| {
                                AppError::NotFound("Lesson not found in this course".into())
                            })?;
                    let material =
                        LearningAssessmentRepository::lesson_material_in(&mut tx, lesson_id)
                            .await?;
                    let seeds = LearningAssessmentRepository::lesson_seeds_in(
                        &mut tx, program_id, lesson_id,
                    )
                    .await?;
                    tx.commit().await.map_err(db)?;
                    (title, objective, material, seeds)
                };
                let Some(pin) = context.until_cancelled((self.load_llm)()).await else {
                    return Ok(JobOutcome::Stopped);
                };
                let pin = pin?;
                let Some(bank) = context
                    .until_cancelled(lesson_bank_stage(
                        pin.llm.as_ref(),
                        &self.pool,
                        program_id,
                        lesson_id,
                        &title,
                        &objective,
                        material,
                        seeds,
                    ))
                    .await
                else {
                    return Ok(JobOutcome::Stopped);
                };
                let bank = bank?;
                context
                    .put_checkpoint(LESSON_BANK_CHECKPOINT, &serde_json::to_value(&bank)?)
                    .await?;
                bank
            }
        };
        let items: Vec<BankItem> = bank
            .items
            .into_iter()
            .map(|authored| {
                let mut item = authored.candidate;
                item.outcome_ids = vec![bank.outcome_id.clone()];
                BankItem {
                    item,
                    source_quote: authored.source_quote,
                    lesson_id: Some(lesson_id.to_owned()),
                    origin: "bank",
                    review_status: "agreed",
                }
            })
            .collect();
        let mut tx = self.pool.begin_write().await.map_err(db)?;
        let written = crate::assessment_repository::insert_bank_items_in(
            &mut tx,
            program_id,
            &items,
            bank.author_model.as_deref(),
        )
        .await?;
        tx.commit().await.map_err(db)?;
        Ok(JobOutcome::Completed {
            result_ref: format!("quiz_bank:{written}"),
            message: format!("{written} questions written."),
        })
    }
}

mod grading;
mod lesson_stage;
mod repository;

pub use grading::{grade_written_answer, judge_short_answer, QuizGradingWorker};
pub(crate) use lesson_stage::{lesson_bank_stage, lesson_seed_items, LessonBank};
