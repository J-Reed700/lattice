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
use crate::bank_writing::BankWriting;
use crate::curriculum_repository::lesson_rank_in;
use crate::dto::{
    LearningAssessmentGradeStatus, LearningQuizScopeDto, LearningQuizScopeKind, LessonAnswerKey,
    LessonQuestion, LessonQuestionKind,
};
use crate::grade_rationale::{self, Checked, Grader, Reference};
use crate::lessons::trace;
use crate::persistence::now;
use crate::quiz_blueprints::{self, CourseFacts, LessonFacts};
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::PinnedModel;
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;
use lattice_core::shared::runtime::jobs::{
    Backoff, JobContext, JobFailure, JobHandler, JobKindConfig, JobOutcome, JobRecord, JobStatus,
    JobStore, RecoveryPolicy,
};
use serde_json::json;
use std::sync::Arc;
use tracing::Instrument;

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

/// Queue writing a quiz's questions: its missing bank, or with `top_up` one
/// more form's worth.
pub async fn queue_bank(
    database: &Database,
    operation_id: &str,
    program_id: &str,
    scope: &LearningQuizScopeDto,
    top_up: bool,
) -> Result<String> {
    quiz_blueprints::validate_scope(scope)?;
    let mut tx = database.begin_write().await?;
    let needs =
        LearningAssessmentRepository::bank_needs_in(&mut tx, program_id, scope, top_up).await?;
    let rank = match (&scope.kind, &scope.id) {
        (LearningQuizScopeKind::Lesson, Some(lesson)) => lesson_rank_in(&mut tx, lesson).await?,
        _ => None,
    };
    let id = repository::queue_in(
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
    tx.commit().await?;
    Ok(id)
}

/// What writing a bank produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BankReport {
    pub written: usize,
    /// Items the checks or the blinded key review left out, with why.
    pub dropped: Vec<String>,
}

/// Write a quiz's missing questions with `llm` now, as the `quiz_bank` job's
/// general path does (per-format authoring, code checks, shuffled options,
/// blinded key review), without the job runner. For the course eval's live
/// hook and other callers that hold a model already.
pub async fn prepare_bank_now(
    database: &Database,
    llm: &dyn LLMPort,
    program_id: &str,
    scope: &LearningQuizScopeDto,
    top_up: bool,
) -> Result<BankReport> {
    let needs =
        LearningAssessmentRepository::bank_needs(database, program_id, scope, top_up).await?;
    let mut report = BankReport::default();
    let mut writing = BankWriting::new(needs);
    while let Some(need) = writing.next() {
        let authored = generation::author_items(llm, &need).await?;
        writing.wrote(&need, &authored);
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
        report.written += LearningAssessmentRepository::insert_bank_items(
            database,
            program_id,
            &items,
            Some(llm.model_name()),
        )
        .await?;
    }
    Ok(report)
}

fn written_message(written: usize, dropped: usize) -> String {
    if dropped == 0 {
        format!("{written} questions written.")
    } else {
        format!("{written} questions written; {dropped} failed the checks and were left out.")
    }
}

/// What a finished bank job says: what it wrote, and when the quiz's first form still cannot be filled, that the
/// quiz cannot start (traced in the lesson's span for a lesson scope).
async fn completion_message(
    database: &Database,
    program_id: &str,
    scope: &LearningQuizScopeDto,
    written: usize,
    dropped: usize,
) -> Result<String> {
    let message = written_message(written, dropped);
    let Some(shortfall) =
        LearningAssessmentRepository::first_form_shortfall(database, program_id, scope).await?
    else {
        return Ok(message);
    };
    let requirement = &shortfall.requirement;
    let why = format!(
        "{} of {} {} {}-{} items for the first form",
        shortfall.eligible,
        requirement.count,
        crate::quiz_blueprints::format_name(requirement.format),
        requirement.difficulty_min,
        requirement.difficulty_max
    );
    match (&scope.kind, &scope.id) {
        (LearningQuizScopeKind::Lesson, Some(lesson_id)) => {
            trace::lesson_span(lesson_id).in_scope(|| {
                trace::decided(trace::Stage::Quiz, "quiz bank", "cannot start", &why);
            });
        }
        _ => trace::decided(trace::Stage::Quiz, "quiz bank", "cannot start", &why),
    }
    Ok(format!(
        "{message} The quiz cannot start yet: its questions could not fill a first form."
    ))
}

/// A completed lesson quiz job's numbers: what it stored, from its result,
/// and what its checks or the blinded key review left out, from its saved
/// stage output. `None` until the job has completed.
pub async fn lesson_stage_report(database: &Database, job_id: &str) -> Result<Option<BankReport>> {
    let store = JobStore::new(database.clone());
    let job = store.get(job_id).await?;
    if job.status != JobStatus::Completed {
        return Ok(None);
    }
    let written = job
        .result_ref
        .as_deref()
        .and_then(|result| result.strip_prefix("quiz_bank:"))
        .and_then(|count| count.parse().ok())
        .unwrap_or(0);
    let dropped = match store.checkpoint(job_id, LESSON_BANK_CHECKPOINT).await? {
        Some(saved) => serde_json::from_value::<LessonBank>(saved)?.dropped,
        None => Vec::new(),
    };
    Ok(Some(BankReport { written, dropped }))
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
    pub database: Database,
    pub load_llm: ModelLoader,
}

#[async_trait::async_trait]
impl JobHandler for QuizBankWorker {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        let _activity =
            lattice_core::shared::runtime::user_activity::UserActivity::begin("Preparing a quiz");
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
        let needs =
            LearningAssessmentRepository::bank_needs(&self.database, &program_id, &scope, top_up)
                .await?;
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
        let planned = needs.len();
        let mut written = 0usize;
        let mut dropped = 0usize;
        let mut taken = 0usize;
        let mut writing = BankWriting::new(needs);
        while let Some(need) = writing.next() {
            context
                .progress(
                    (taken as u32).min(total),
                    &format!("Writing quiz questions ({} of {planned})", taken + 1),
                    None,
                )
                .await?;
            taken += 1;
            let Some(authored) = context
                .until_cancelled(generation::author_items(llm, &need))
                .await
            else {
                return Ok(JobOutcome::Stopped);
            };
            let authored = authored?;
            writing.wrote(&need, &authored);
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
            written += LearningAssessmentRepository::insert_bank_items(
                &self.database,
                &program_id,
                &items,
                Some(llm.model_name()),
            )
            .await?;
        }
        Ok(JobOutcome::Completed {
            result_ref: format!("quiz_bank:{written}"),
            message: completion_message(&self.database, &program_id, &scope, written, dropped)
                .await?,
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
                let inputs =
                    repository::lesson_inputs(&self.database, program_id, lesson_id).await?;
                let Some(pin) = context.until_cancelled((self.load_llm)()).await else {
                    return Ok(JobOutcome::Stopped);
                };
                let pin = pin?;
                let Some(bank) = context
                    .until_cancelled(
                        lesson_bank_stage(
                            pin.llm.as_ref(),
                            &self.database,
                            program_id,
                            lesson_id,
                            &inputs.title,
                            &inputs.objective,
                            inputs.material,
                            inputs.seeds,
                        )
                        .instrument(trace::lesson_span(lesson_id)),
                    )
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
        trace::lesson_span(lesson_id).in_scope(|| {
            for authored in &bank.items {
                trace::decided(
                    trace::Stage::Quiz,
                    &format!("question {}", authored.candidate.prompt),
                    "kept",
                    "written for the quiz bank and reviewed",
                );
            }
            for reason in &bank.dropped {
                trace::decided(trace::Stage::Quiz, "quiz item", "dropped", reason);
            }
        });
        let dropped = bank.dropped.len();
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
        let written = LearningAssessmentRepository::insert_bank_items(
            &self.database,
            program_id,
            &items,
            bank.author_model.as_deref(),
        )
        .await?;
        trace::lesson_span(lesson_id).in_scope(|| {
            trace::decided(
                trace::Stage::Quiz,
                "quiz bank",
                "written",
                &format!("{written} items inserted, {dropped} dropped"),
            );
        });
        Ok(JobOutcome::Completed {
            result_ref: format!("quiz_bank:{written}"),
            message: completion_message(&self.database, program_id, scope, written, dropped)
                .await?,
        })
    }
}

mod grading;
mod lesson_stage;
mod repository;

pub(crate) use repository::{queue_grading_in, queue_lesson_bank_in};

pub use grading::{grade_written_answer, judge_short_answer, QuizGradingWorker};
pub(crate) use lesson_stage::{
    lesson_bank_stage, lesson_question_item, lesson_seed_items, LessonBank,
};
