//! The two quiz jobs on the learning job runner (sections 4.1 and 5.4):
//! `quiz_bank` writes a quiz's question bank, `quiz_grading` grades written
//! answers and short-answer near-misses, and contests' blind regrades. Both
//! are stoppable, size every model call to the window, and never time out; a
//! stopped grading job leaves its items pending, never failed. The lesson
//! job's own bank stage is [`lesson_bank_stage`].
use crate::features::learning::assessment_engine::{self as engine, LearningItemFormat};
use crate::features::learning::assessment_generation::{
    self as generation, AuthoredItem, AuthoringNeed, MaterialPassage, RawGrade, ShortAnswer,
    StoredItem, WrittenAnswer,
};
use crate::features::learning::assessment_repository::{
    BankItem, ItemGrade, LearningAssessmentRepository,
};
use crate::features::learning::dto::{
    LearningAssessmentGradeStatus, LearningBlockDto, LearningQuizScopeDto, LearningQuizScopeKind,
    LessonAnswerKey, LessonQuestion, LessonQuestionKind,
};
use crate::features::learning::grade_rationale::{self, Checked, Grader, Reference};
use crate::features::learning::persistence::{db, now};
use crate::features::learning::quiz_blueprints::{self, CourseFacts, LessonFacts};
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::PinnedModel;
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::{AppError, Result};
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

/// The Learning model, pinned for the whole job.
pub type ModelLoader = Arc<dyn Fn() -> BoxFuture<'static, Result<PinnedModel>> + Send + Sync>;

/// One job per course at a time for each kind; a running job resumes after a
/// restart, and a disconnected model defers it rather than failing it.
pub fn quiz_job_config() -> JobKindConfig {
    JobKindConfig::new(RecoveryPolicy::Requeue)
        .exclusive_per_subject()
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
) -> Result<String> {
    let admitted = JobStore::submit_in(
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
    )
    .await?;
    if admitted.created {
        let revision: i64 = sqlx::query_scalar("SELECT revision FROM learning_programs WHERE id=?")
            .bind(program_id)
            .fetch_one(&mut *connection)
            .await
            .map_err(db)?;
        sqlx::query("INSERT INTO learning_jobs(job_id,program_id,base_revision_number,content_revision) VALUES(?,?,?,NULL)")
            .bind(&admitted.id)
            .bind(program_id)
            .bind(revision.max(0))
            .execute(&mut *connection)
            .await
            .map_err(db)?;
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
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM learning_assessment_item_results WHERE form_id=? AND grade_status='pending'")
        .bind(form_id)
        .fetch_one(&mut *connection)
        .await
        .map_err(db)?;
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
    let mut tx = pool.begin().await.map_err(db)?;
    let needs =
        LearningAssessmentRepository::bank_needs_in(&mut tx, program_id, scope, top_up).await?;
    let id = queue_in(
        &mut tx,
        QUIZ_BANK,
        program_id,
        operation_id.to_string(),
        json!({"programId": program_id, "scope": scope, "topUp": top_up}),
        needs.len() as u32,
        "Waiting to write quiz questions",
    )
    .await?;
    tx.commit().await.map_err(db)?;
    Ok(id)
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
        let mut tx = pool.begin().await.map_err(db)?;
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
            })
            .collect();
        let mut tx = pool.begin().await.map_err(db)?;
        report.written += crate::features::learning::assessment_repository::insert_bank_items_in(
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
            let mut tx = self.pool.begin().await.map_err(db)?;
            let needs =
                LearningAssessmentRepository::bank_needs_in(&mut tx, &program_id, &scope, top_up)
                    .await?;
            tx.commit().await.map_err(db)?;
            needs
        };
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
                })
                .collect();
            let mut tx = self.pool.begin().await.map_err(db)?;
            written += crate::features::learning::assessment_repository::insert_bank_items_in(
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
        crate::features::learning::curriculum_repository::lesson_failure(job, error)
    }
}

mod grading;
mod lesson_stage;

pub use grading::{grade_written_answer, judge_short_answer, QuizGradingWorker};
pub(crate) use lesson_stage::{lesson_bank_stage, lesson_seed_items, LessonBank};
