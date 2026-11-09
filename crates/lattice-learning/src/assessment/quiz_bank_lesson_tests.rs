//! The quiz job a lesson's publication queues: it writes the published
//! lesson's bank once, from what the lesson's rows hold.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use crate::{
    assessment_engine::{LearningAssessmentKey, LearningItemFormat},
    assessment_generation::{AuthoredItem, StoredItem},
    assessment_repository::{insert_bank_items_in, BankItem, LearningAssessmentRepository},
    dto::*,
    generation_jobs::overlap_tests::{quiz_jobs, run_lesson_job, Published, Recorder, BODY_MARK},
    quiz_bank_jobs::{lesson_stage_report, LessonBank, QuizBankWorker, LESSON_BANK_CHECKPOINT},
};
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{JobContext, JobHandler, JobOutcome, JobStore};
use sqlx::SqlitePool;
use std::sync::Arc;

type BankRow = (
    String,
    String,
    Option<String>,
    String,
    Option<String>,
    String,
);

fn worker(pool: &SqlitePool, model: Arc<Recorder>) -> QuizBankWorker {
    let llm: Arc<dyn LLMPort> = model;
    QuizBankWorker {
        pool: pool.clone(),
        load_llm: Arc::new(move || {
            let llm = llm.clone();
            Box::pin(async move { Ok(crate::generation_jobs::pinned_for_test(llm)) })
        }),
    }
}

/// The lesson published by its job, with the one quiz job that publication
/// queued.
async fn published(pool: &SqlitePool) -> Result<(Published, String)> {
    let published = run_lesson_job(pool, Recorder::new()).await?;
    let jobs = quiz_jobs(pool, &published.program_id).await?;
    assert_eq!(jobs.len(), 1, "publication queued no quiz job");
    let id = jobs[0].id.clone();
    Ok((published, id))
}

fn lesson_scope(lesson_id: &str) -> LearningQuizScopeDto {
    LearningQuizScopeDto {
        kind: LearningQuizScopeKind::Lesson,
        id: Some(lesson_id.to_owned()),
    }
}

/// The job as the runtime hands it to its worker: claimed, running.
async fn claimed(pool: &SqlitePool, job_id: &str) -> Result<JobContext> {
    let store = JobStore::new(pool.clone());
    store.claim(job_id).await?.unwrap();
    JobContext::detached(&store, job_id).await
}

async fn lesson_outcome(pool: &SqlitePool, lesson_id: &str) -> String {
    sqlx::query_scalar("SELECT id FROM learning_outcome_definitions WHERE lesson_id=?")
        .bind(lesson_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn stored(id: &str, outcome: &str, format: LearningItemFormat, difficulty: u8) -> StoredItem {
    StoredItem {
        id: id.to_owned(),
        outcome_ids: vec![outcome.to_owned()],
        format,
        difficulty,
        prompt: format!("What does the lesson say about case {id}?"),
        options: vec![
            "The first reading".into(),
            "The second reading".into(),
            "The third reading".into(),
            "The fourth reading".into(),
        ],
        left: vec![],
        source_version_ids: vec![],
        rubric: vec![],
        key: LearningAssessmentKey::Choice(1),
        explanation: "The lesson says so.".into(),
        distractor_rationales: vec![],
    }
}

#[tokio::test]
async fn the_quiz_job_writes_from_the_published_lessons_own_text() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let (_, job_id) = published(&pool).await?;
    let model = Recorder::new();
    let context = claimed(&pool, &job_id).await?;
    // The scripted model writes no items; what matters is what the stage asks.
    let _ = worker(&pool, model.clone()).run(&context).await;
    let requests = model.requests();
    assert!(!requests.is_empty(), "the stage made no model call");
    assert!(
        requests.iter().any(|request| request.contains(BODY_MARK)),
        "the stage was not given the published lesson's text: {requests:?}"
    );
    Ok(())
}

#[tokio::test]
async fn the_quiz_job_does_nothing_when_the_lessons_bank_is_already_written() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let (published, job_id) = published(&pool).await?;
    let scope = lesson_scope(&published.lesson_id);
    let needs = {
        let mut tx = pool.begin().await.unwrap();
        let needs = LearningAssessmentRepository::bank_needs_in(
            &mut tx,
            &published.program_id,
            &scope,
            false,
        )
        .await?;
        tx.commit().await.unwrap();
        needs
    };
    assert!(!needs.is_empty(), "the lesson's own questions meet no need");
    let items: Vec<BankItem> = needs
        .iter()
        .flat_map(|need| {
            (0..need.count).map(|_| BankItem {
                item: stored(
                    &uuid::Uuid::new_v4().to_string(),
                    &need.requirement.outcome_id,
                    need.requirement.format,
                    need.requirement.difficulty_min,
                ),
                source_quote: String::new(),
                lesson_id: Some(published.lesson_id.clone()),
                origin: "lesson",
                review_status: "agreed",
            })
        })
        .collect();
    let mut tx = pool.begin().await.unwrap();
    insert_bank_items_in(&mut tx, &published.program_id, &items, None).await?;
    tx.commit().await.unwrap();

    let model = Recorder::new();
    let context = claimed(&pool, &job_id).await?;
    let outcome = worker(&pool, model.clone()).run(&context).await?;
    assert!(
        matches!(outcome, JobOutcome::Completed { .. }),
        "{outcome:?}"
    );
    assert!(model.requests().is_empty(), "the stage was called again");
    Ok(())
}

#[tokio::test]
async fn a_resumed_quiz_job_inserts_its_saved_stage_output_once() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let (published, job_id) = published(&pool).await?;
    let outcome_id = lesson_outcome(&pool, &published.lesson_id).await;
    let item_id = uuid::Uuid::new_v4().to_string();
    let bank = LessonBank {
        outcome_id: outcome_id.clone(),
        items: vec![AuthoredItem {
            candidate: stored(
                &item_id,
                "an outcome the stage did not know",
                LearningItemFormat::SingleChoice,
                3,
            ),
            source_quote: "a quote from the lesson".into(),
            lesson_id: Some(published.lesson_id.clone()),
        }],
        dropped: vec![],
        author_model: Some("the-authoring-model".into()),
    };
    let context = claimed(&pool, &job_id).await?;
    assert!(
        context
            .put_checkpoint(LESSON_BANK_CHECKPOINT, &serde_json::to_value(&bank)?)
            .await?
    );

    let model = Recorder::new();
    let resumed = worker(&pool, model.clone());
    for _ in 0..2 {
        let outcome = resumed.run(&context).await?;
        assert!(
            matches!(outcome, JobOutcome::Completed { .. }),
            "{outcome:?}"
        );
    }
    assert!(model.requests().is_empty(), "the stage ran again");
    let rows: Vec<BankRow> = sqlx::query_as(
        "SELECT origin,review_status,lesson_id,source_quote,author_model,outcome_id FROM learning_assessment_candidates WHERE id=?",
    )
    .bind(&item_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        vec![(
            "bank".to_owned(),
            "agreed".to_owned(),
            Some(published.lesson_id.clone()),
            "a quote from the lesson".to_owned(),
            Some("the-authoring-model".to_owned()),
            outcome_id,
        )]
    );
    Ok(())
}

#[tokio::test]
async fn a_failed_quiz_job_leaves_the_lesson_published_and_a_prepare_can_still_write() -> Result<()>
{
    let pool = crate::tests::pool().await?;
    let (published, job_id) = published(&pool).await?;
    let context = claimed(&pool, &job_id).await?;
    let failing = QuizBankWorker {
        pool: pool.clone(),
        load_llm: Arc::new(|| {
            Box::pin(async { Err(AppError::InvalidInput("the model is gone".into())) })
        }),
    };
    assert!(failing.run(&context).await.is_err());
    JobStore::new(pool.clone())
        .fail(&job_id, "generation_failed", "the model is gone")
        .await?;

    let lesson = &published.lesson_id;
    let ready: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM learning_lessons WHERE id=? AND preparation='ready'",
    )
    .bind(lesson)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ready, 1);
    let seeds: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM learning_assessment_candidates WHERE lesson_id=? AND origin='lesson' AND review_status='agreed'",
    )
    .bind(lesson)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(seeds, 3);

    let again = crate::quiz_bank_jobs::queue_bank(
        &pool,
        &uuid::Uuid::new_v4().to_string(),
        &published.program_id,
        &lesson_scope(lesson),
        false,
    )
    .await?;
    assert_ne!(again, job_id);
    let model = Recorder::new();
    let context = claimed(&pool, &again).await?;
    let _ = worker(&pool, model.clone()).run(&context).await;
    assert!(
        !model.requests().is_empty(),
        "an ordinary prepare wrote nothing for the lesson"
    );
    Ok(())
}

#[tokio::test]
async fn a_lessons_quiz_is_preparing_while_its_quiz_job_is_unfinished() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let (published, job_id) = published(&pool).await?;
    let repo = LearningAssessmentRepository::new(pool.clone());
    let quiz_of = |overview: LearningQuizOverviewDto| {
        overview
            .modules
            .into_iter()
            .flat_map(|module| module.lessons)
            .find(|lesson| lesson.lesson_id == published.lesson_id)
            .unwrap()
            .quiz
    };
    let waiting = quiz_of(repo.overview(&published.program_id).await?);
    assert_eq!(waiting.bank_status, LearningQuizBankStatus::Preparing);
    assert_eq!(waiting.preparing_job_id.as_deref(), Some(job_id.as_str()));
    assert_eq!(waiting.bank_message, "Writing quiz questions.");

    claimed(&pool, &job_id).await?;
    JobStore::new(pool.clone())
        .fail(&job_id, "generation_failed", "the model is gone")
        .await?;
    let failed = quiz_of(repo.overview(&published.program_id).await?);
    assert_eq!(failed.bank_status, LearningQuizBankStatus::Failed);
    assert_eq!(failed.preparing_job_id, None);
    Ok(())
}

/// Answers every quiz authoring request with one item that cites nothing, so
/// the bank's checks refuse it, and every review with no issues.
struct Unciting;

#[async_trait::async_trait]
impl LLMPort for Unciting {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let (system, _) = crate::lessons::structured_call::task_messages(request);
        Ok(CompletionResponse::from_text(
            if system.starts_with("Write quiz questions") {
                "{\"items\":[{}]}"
            } else {
                "{\"issues\":[]}"
            },
        ))
    }
    fn model_name(&self) -> &str {
        "unciting-fixture"
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    fn max_context_tokens(&self) -> usize {
        128_000
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn a_lesson_quiz_job_keeps_what_its_checks_left_out() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let (_, job_id) = published(&pool).await?;
    let context = claimed(&pool, &job_id).await?;
    let llm: Arc<dyn LLMPort> = Arc::new(Unciting);
    let quiz_worker = QuizBankWorker {
        pool: pool.clone(),
        load_llm: Arc::new(move || {
            let llm = llm.clone();
            Box::pin(async move { Ok(crate::generation_jobs::pinned_for_test(llm)) })
        }),
    };
    let outcome = quiz_worker.run(&context).await?;

    let saved = JobStore::new(pool.clone())
        .checkpoint(&job_id, LESSON_BANK_CHECKPOINT)
        .await?
        .unwrap();
    let dropped = saved["dropped"].as_array().map_or(0, Vec::len);
    assert!(dropped > 0, "the stage kept no reason for its refusals: {saved}");
    let JobOutcome::Completed { message, .. } = outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(
        message,
        format!("0 questions written; {dropped} failed the checks and were left out.")
    );
    Ok(())
}

#[tokio::test]
async fn lesson_stage_report_reads_a_completed_jobs_numbers() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let (published, job_id) = published(&pool).await?;
    assert_eq!(lesson_stage_report(&pool, &job_id).await?, None);

    let outcome_id = lesson_outcome(&pool, &published.lesson_id).await;
    let item_id = uuid::Uuid::new_v4().to_string();
    let bank = LessonBank {
        outcome_id,
        items: vec![AuthoredItem {
            candidate: stored(&item_id, "an outcome", LearningItemFormat::SingleChoice, 3),
            source_quote: "a quote from the lesson".into(),
            lesson_id: Some(published.lesson_id.clone()),
        }],
        dropped: vec!["a key out of range".into(), "a duplicate question".into()],
        author_model: Some("the-authoring-model".into()),
    };
    let context = claimed(&pool, &job_id).await?;
    assert_eq!(lesson_stage_report(&pool, &job_id).await?, None);
    assert!(
        context
            .put_checkpoint(LESSON_BANK_CHECKPOINT, &serde_json::to_value(&bank)?)
            .await?
    );
    let outcome = worker(&pool, Recorder::new()).run(&context).await?;
    let JobOutcome::Completed { result_ref, message } = outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(message, "1 questions written; 2 failed the checks and were left out.");
    JobStore::new(pool.clone())
        .complete(&job_id, &result_ref, &message)
        .await?;

    let stored_rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM learning_assessment_candidates WHERE id=?")
            .bind(&item_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let report = lesson_stage_report(&pool, &job_id).await?.unwrap();
    assert_eq!(i64::try_from(report.written).unwrap(), stored_rows);
    assert_eq!(report.dropped, bank.dropped);
    Ok(())
}
