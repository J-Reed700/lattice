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
    bytes.copy_from_slice(&hash[..16]);
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

/// Grade one written answer as the `quiz_grading` job does: the model's
/// rubric grade, then the section 4.3a checks. `Checked::Unexplained` is
/// "Couldn't grade yet", never a wrong answer.
pub async fn grade_written_answer(
    llm: &dyn LLMPort,
    answer: &WrittenAnswer,
    reference_version: Option<&str>,
) -> Result<Checked> {
    let raw = generation::grade_written(llm, std::slice::from_ref(answer)).await?;
    Ok(checked_grade(
        raw.iter().find(|r| r.item_id == answer.item_id),
        &answer.rubric,
        &answer.answer,
        &answer.reference,
        reference_version,
        &Grader {
            model: llm.model_name(),
            graded_at: now(),
        },
    ))
}

/// Judge one short answer that matched no accepted variant, in the same
/// rationale shape.
pub async fn judge_short_answer(llm: &dyn LLMPort, answer: &ShortAnswer) -> Result<Checked> {
    let raw = generation::judge_short_answers(llm, std::slice::from_ref(answer)).await?;
    Ok(checked_grade(
        raw.iter().find(|r| r.item_id == answer.item_id),
        &grade_rationale::equivalence_rubric(),
        &answer.answer,
        &answer.reference,
        None,
        &Grader {
            model: llm.model_name(),
            graded_at: now(),
        },
    ))
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

/// Grades a submitted quiz's pending items, or regrades a contest.
#[derive(Clone)]
pub struct QuizGradingWorker {
    pub pool: SqlitePool,
    pub load_llm: ModelLoader,
}

/// What a pending item needs to be graded.
enum PendingItem {
    Written(
        WrittenAnswer,
        i64,
        Vec<engine::LearningRubricCriterion>,
        Option<String>,
    ),
    Short(ShortAnswer, i64, Option<String>),
}

/// Check a grader's answer and turn it into the item's grade.
fn checked_grade(
    raw: Option<&RawGrade>,
    rubric: &[engine::LearningRubricCriterion],
    answer: &str,
    reference: &str,
    version: Option<&str>,
    grader: &Grader<'_>,
) -> Checked {
    let Some(raw) = raw else {
        return Checked::Unexplained(grade_rationale::UNEXPLAINED.into());
    };
    grade_rationale::check(
        rubric,
        answer,
        (!reference.trim().is_empty()).then_some(Reference {
            text: reference,
            version_id: version,
        }),
        raw.item.clone(),
        grader,
    )
}

fn short_detail(
    answer: &str,
    fraction: f64,
    rationale: &crate::features::learning::dto::LearningGradeRationaleDto,
) -> String {
    if fraction >= 1.0 {
        return String::new();
    }
    let why = rationale
        .criteria
        .first()
        .map(|c| c.what_was_missing.clone())
        .unwrap_or_default();
    format!(
        "“{}” isn't an accepted answer; the model judged it not equivalent because {}",
        answer.trim(),
        why.trim_end_matches('.').to_string() + "."
    )
}

impl QuizGradingWorker {
    async fn pending_items(
        &self,
        program_id: &str,
        form_id: &str,
        only: Option<i64>,
        note: Option<&str>,
    ) -> Result<Vec<PendingItem>> {
        let repo = LearningAssessmentRepository::new(self.pool.clone());
        let form = repo.get_form(form_id).await?;
        let Some(submission) = &form.submission else {
            return Ok(vec![]);
        };
        let mut connection = self.pool.acquire().await.map_err(db)?;
        let mut out = Vec::new();
        for (ordinal, (item, result)) in form.items.iter().zip(&submission.item_results).enumerate()
        {
            let ordinal = ordinal as i64;
            let wanted = match only {
                Some(o) => o == ordinal,
                None => result.grade_status == LearningAssessmentGradeStatus::Pending,
            };
            if !wanted {
                continue;
            }
            let stored = LearningAssessmentRepository::stored_candidate_in(
                &mut connection,
                program_id,
                &item.id,
            )
            .await?;
            let version = stored.candidate.source_version_ids.first().cloned();
            match (&item.format, &stored.candidate.key) {
                (LearningItemFormat::ShortAnswer, engine::LearningAssessmentKey::Text(key)) => out
                    .push(PendingItem::Short(
                        ShortAnswer {
                            item_id: item.id.clone(),
                            prompt: item.prompt.clone(),
                            accepted: key.variants.clone(),
                            answer: item.text_response.clone().unwrap_or_default(),
                            reference: stored.source_quote.clone(),
                            learner_note: note.map(str::to_owned),
                        },
                        ordinal,
                        version,
                    )),
                (LearningItemFormat::Explanation | LearningItemFormat::Artifact, _) => {
                    let answer = match (&item.text_response, &item.artifact_json) {
                        (Some(text), Some(artifact)) => format!("{text}\n{artifact}"),
                        (Some(text), None) => text.clone(),
                        (None, Some(artifact)) => artifact.to_string(),
                        (None, None) => String::new(),
                    };
                    out.push(PendingItem::Written(
                        WrittenAnswer {
                            item_id: item.id.clone(),
                            prompt: item.prompt.clone(),
                            rubric: item.rubric.clone(),
                            answer,
                            reference: stored.source_quote.clone(),
                            learner_note: note.map(str::to_owned),
                        },
                        ordinal,
                        item.rubric.clone(),
                        version,
                    ))
                }
                _ => {}
            }
        }
        Ok(out)
    }

    /// Grade items through the model and check every rationale.
    async fn grade(
        llm: &dyn LLMPort,
        items: &[PendingItem],
    ) -> Result<Vec<(i64, Checked, Option<String>, bool, String)>> {
        let written: Vec<WrittenAnswer> = items
            .iter()
            .filter_map(|i| match i {
                PendingItem::Written(a, ..) => Some(a.clone()),
                _ => None,
            })
            .collect();
        let short: Vec<ShortAnswer> = items
            .iter()
            .filter_map(|i| match i {
                PendingItem::Short(a, ..) => Some(a.clone()),
                _ => None,
            })
            .collect();
        let mut raw = generation::grade_written(llm, &written).await?;
        raw.extend(generation::judge_short_answers(llm, &short).await?);
        let grader = Grader {
            model: llm.model_name(),
            graded_at: now(),
        };
        let equivalence = grade_rationale::equivalence_rubric();
        Ok(items
            .iter()
            .map(|item| match item {
                PendingItem::Written(a, ordinal, rubric, version) => {
                    let found = raw.iter().find(|r| r.item_id == a.item_id);
                    (
                        *ordinal,
                        checked_grade(
                            found,
                            rubric,
                            &a.answer,
                            &a.reference,
                            version.as_deref(),
                            &grader,
                        ),
                        found.and_then(|r| r.response_to_note.clone()),
                        false,
                        a.answer.clone(),
                    )
                }
                PendingItem::Short(a, ordinal, version) => {
                    let found = raw.iter().find(|r| r.item_id == a.item_id);
                    (
                        *ordinal,
                        checked_grade(
                            found,
                            &equivalence,
                            &a.answer,
                            &a.reference,
                            version.as_deref(),
                            &grader,
                        ),
                        found.and_then(|r| r.response_to_note.clone()),
                        true,
                        a.answer.clone(),
                    )
                }
            })
            .collect())
    }
}

#[async_trait::async_trait]
impl JobHandler for QuizGradingWorker {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        let job = context.job();
        let program_id = requested_str(job, "programId")?.to_string();
        let form_id = requested_str(job, "formId")?.to_string();
        let contest_id = job
            .requested
            .get("contestId")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let repo = LearningAssessmentRepository::new(self.pool.clone());
        let contest = match &contest_id {
            Some(id) => match repo.contest_input(id).await? {
                Some(contest) => Some(contest),
                None => {
                    return Ok(JobOutcome::Completed {
                        result_ref: format!("quiz_regrade:{id}"),
                        message: "This contest was already decided.".into(),
                    })
                }
            },
            None => None,
        };
        let items = self
            .pending_items(
                &program_id,
                &form_id,
                contest.as_ref().map(|c| c.ordinal),
                contest.as_ref().map(|c| c.note.as_str()),
            )
            .await?;
        if items.is_empty() {
            return Ok(JobOutcome::Completed {
                result_ref: format!("quiz_grading:{form_id}"),
                message: "Nothing is waiting to be graded.".into(),
            });
        }
        let Some(pin) = context.until_cancelled((self.load_llm)()).await else {
            return Ok(JobOutcome::Stopped);
        };
        let pin = pin?;
        let llm = pin.llm.as_ref();
        let model = llm.model_name().to_string();
        if let (Some(contest_id), Some(_)) = (&contest_id, &contest) {
            context.progress(0, "Regrading your answer", None).await?;
            let Some(graded) = context.until_cancelled(Self::grade(llm, &items)).await else {
                return Ok(JobOutcome::Stopped);
            };
            let (_, checked, response_to_note, short, _) = graded?
                .into_iter()
                .next()
                .ok_or_else(|| AppError::InvalidState("The regrade returned nothing.".into()))?;
            let regrade = match checked {
                Checked::Graded {
                    rationale,
                    fraction,
                    status,
                } => crate::features::learning::assessment_repository::Regrade::Graded {
                    fraction,
                    correct: short.then_some(fraction >= 1.0),
                    status,
                    rationale,
                    response_to_note,
                },
                Checked::Unexplained(reason) => {
                    crate::features::learning::assessment_repository::Regrade::Failed(reason)
                }
            };
            repo.apply_regrade(contest_id, regrade, &model).await?;
            return Ok(JobOutcome::Completed {
                result_ref: format!("quiz_regrade:{contest_id}"),
                message: "Regrade finished.".into(),
            });
        }
        // Grade a few answers at a time, so Stop keeps what was graded.
        let total = context.job().progress_total;
        let mut done = 0usize;
        let mut unexplained = 0usize;
        for chunk in items.chunks(4) {
            context
                .progress(
                    (done as u32).min(total),
                    &format!("Grading written answers ({} of {})", done + 1, items.len()),
                    None,
                )
                .await?;
            let Some(graded) = context.until_cancelled(Self::grade(llm, chunk)).await else {
                return Ok(JobOutcome::Stopped);
            };
            let grades: Vec<ItemGrade> = graded?
                .into_iter()
                .map(|(ordinal, checked, _, short, answer)| match checked {
                    Checked::Graded {
                        rationale,
                        fraction,
                        status,
                    } => ItemGrade::Graded {
                        ordinal,
                        fraction,
                        correct: short.then_some(fraction >= 1.0),
                        status,
                        detail: if short {
                            short_detail(&answer, fraction, &rationale)
                        } else {
                            String::new()
                        },
                        rationale: Some(rationale),
                    },
                    Checked::Unexplained(reason) => {
                        unexplained += 1;
                        ItemGrade::Unexplained { ordinal, reason }
                    }
                })
                .collect();
            repo.apply_grades(&form_id, &grades, &model).await?;
            done += chunk.len();
        }
        Ok(JobOutcome::Completed {
            result_ref: format!("quiz_grading:{form_id}"),
            message: if unexplained == 0 {
                "Written answers graded.".into()
            } else {
                format!("{unexplained} answers couldn't be graded yet; retry grading.")
            },
        })
    }

    fn failure(&self, job: &JobRecord, error: &AppError) -> JobFailure {
        crate::features::learning::curriculum_repository::lesson_failure(job, error)
    }
}

fn seed_difficulty(kind: &LessonQuestionKind) -> u8 {
    match kind {
        LessonQuestionKind::Practice => 2,
        LessonQuestionKind::Quiz => 3,
        LessonQuestionKind::Test => 4,
    }
}

/// A prepared lesson's own reviewed questions as bank items (single choice;
/// difficulty from their kind: practice 2, quiz 3, test 4). Options are
/// reshuffled so the key's position is code's choice. Questions that fail the
/// bank's checks are left out.
pub(crate) fn lesson_seed_items(
    lesson_id: &str,
    outcome_id: &str,
    questions: &[LessonQuestion],
    keys: &[LessonAnswerKey],
) -> Vec<BankItem> {
    questions
        .iter()
        .filter_map(|q| {
            let key = keys.iter().find(|k| k.question_id == q.id)?;
            let mut candidate = engine::LearningAssessmentCandidate {
                id: q.id.clone(),
                outcome_ids: vec![outcome_id.to_string()],
                format: LearningItemFormat::SingleChoice,
                difficulty: seed_difficulty(&q.kind),
                prompt: q.prompt.clone(),
                options: q.options.clone(),
                left: vec![],
                source_version_ids: q
                    .source_ids
                    .iter()
                    .filter(|id| uuid::Uuid::parse_str(id).is_ok())
                    .take(24)
                    .cloned()
                    .collect(),
                rubric: vec![],
                key: engine::LearningAssessmentKey::Choice(key.correct_index),
                explanation: key.explanation.clone(),
                distractor_rationales: vec![],
            };
            if let Err(error) = engine::validate_candidate(&candidate) {
                tracing::info!(question = %q.id, %error, "A lesson question did not join the quiz bank");
                return None;
            }
            engine::shuffle_authored(&mut candidate);
            Some(BankItem {
                item: candidate.into(),
                source_quote: String::new(),
                lesson_id: Some(lesson_id.to_string()),
                origin: "lesson",
            })
        })
        .collect()
}

/// What the lesson job's bank stage wrote for one lesson, published with it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LessonBank {
    /// The outcome the items address; the lesson's outcome is created with
    /// this ID when it does not exist yet.
    pub outcome_id: String,
    pub items: Vec<AuthoredItem>,
    pub author_model: Option<String>,
}

/// The lesson job's last stage, after claim verification and before the
/// lesson is published (section 5.4): write the rest of the lesson's quiz
/// bank from its verified blocks, around its own reviewed questions.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn lesson_bank_stage(
    llm: &dyn LLMPort,
    pool: &SqlitePool,
    program_id: &str,
    lesson_id: &str,
    title: &str,
    objective: &str,
    blocks: &[LearningBlockDto],
    questions: &[LessonQuestion],
    keys: &[LessonAnswerKey],
) -> Result<LessonBank> {
    let (outcome_id, settings) = {
        let mut connection = pool.acquire().await.map_err(db)?;
        let existing: Option<String> = sqlx::query_scalar("SELECT id FROM learning_outcome_definitions WHERE program_id=? AND lesson_id=? ORDER BY created_at,id LIMIT 1")
            .bind(program_id)
            .bind(lesson_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(db)?;
        (
            existing.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            quiz_blueprints::settings_in(&mut connection, program_id).await?,
        )
    };
    let facts = CourseFacts {
        program_title: String::new(),
        modules: vec![],
        lessons: vec![LessonFacts {
            id: lesson_id.into(),
            module_id: String::new(),
            title: title.into(),
            objective: objective.into(),
            prepared: true,
            outcome_id: Some(outcome_id.clone()),
            source_version_ids: vec![],
        }],
    };
    let scope = LearningQuizScopeDto {
        kind: LearningQuizScopeKind::Lesson,
        id: Some(lesson_id.into()),
    };
    let Some(blueprint) = quiz_blueprints::blueprint_for(&scope, &facts, &settings) else {
        return Ok(LessonBank {
            outcome_id,
            ..LessonBank::default()
        });
    };
    let material: Vec<MaterialPassage> = blocks
        .iter()
        .filter(|b| !b.body.trim().is_empty())
        .map(|b| MaterialPassage {
            lesson_title: title.into(),
            title: b.title.clone(),
            text: b.body.clone(),
            source_version_ids: b.source_ids.clone(),
        })
        .collect();
    let seeds: Vec<StoredItem> = lesson_seed_items(lesson_id, &outcome_id, questions, keys)
        .into_iter()
        .map(|s| s.item)
        .collect();
    let mut written: Vec<AuthoredItem> = Vec::new();
    let total = blueprint.slots.len();
    for (index, slot) in blueprint.slots.iter().enumerate() {
        let requirement = &slot.requirement;
        let have = seeds
            .iter()
            .map(|s| engine::LearningAssessmentCandidate::from(s.clone()))
            .chain(written.iter().map(|w| w.candidate.clone().into()))
            .filter(|c| engine::eligible_for(c, requirement))
            .count();
        let count = (3 * requirement.count).saturating_sub(have);
        if count == 0 {
            continue;
        }
        crate::features::learning::lesson_progress::stage(format!(
            "Writing quiz questions (step 3 of 3): {} of {total}",
            index + 1
        ));
        let need = AuthoringNeed {
            requirement: requirement.clone(),
            outcome_title: title.into(),
            outcome_description: objective.into(),
            lesson_id: Some(lesson_id.into()),
            count,
            material: material.clone(),
            existing_prompts: seeds
                .iter()
                .map(|s| s.prompt.clone())
                .chain(written.iter().map(|w| w.candidate.prompt.clone()))
                .collect(),
        };
        written.extend(generation::author_items(llm, &need).await?.items);
    }
    Ok(LessonBank {
        outcome_id,
        items: written,
        author_model: Some(llm.model_name().to_string()),
    })
}
