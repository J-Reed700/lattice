//! The quiz job a lesson's publication queues: it writes the published
//! lesson's bank once, from what the lesson's rows hold.
use crate::{
    assessment_engine::{LearningAssessmentKey, LearningItemFormat},
    assessment_generation::{AuthoredItem, StoredItem},
    assessment_repository::{
        insert_bank_items_in, BankItem, CandidateReviewStatus, LearningAssessmentRepository,
    },
    dto::*,
    generation_jobs::overlap_tests::{quiz_jobs, run_lesson_job, Published, Recorder, BODY_MARK},
    quiz_bank_jobs::{lesson_stage_report, LessonBank, QuizBankWorker, LESSON_BANK_CHECKPOINT},
};
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;
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

fn worker(db: &Database, model: Arc<Recorder>) -> QuizBankWorker {
    let llm: Arc<dyn LLMPort> = model;
    QuizBankWorker {
        database: db.clone(),
        load_llm: Arc::new(move || {
            let llm = llm.clone();
            Box::pin(async move { Ok(crate::generation_jobs::pinned_for_test(llm)) })
        }),
    }
}

/// The lesson published by its job, with the one quiz job that publication
/// queued.
async fn published(db: &Database) -> Result<(Published, String)> {
    let published = run_lesson_job(db, Recorder::new()).await?;
    let jobs = quiz_jobs(db, &published.program_id).await?;
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
async fn claimed(db: &Database, job_id: &str) -> Result<JobContext> {
    let store = JobStore::new(db.clone());
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
    let db = Database::new(pool);
    let (_, job_id) = published(&db).await?;
    let model = Recorder::new();
    let context = claimed(&db, &job_id).await?;
    // The scripted model writes no items; what matters is what the stage asks.
    let _ = worker(&db, model.clone()).run(&context).await;
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
    let db = Database::new(pool);
    let (published, job_id) = published(&db).await?;
    let scope = lesson_scope(&published.lesson_id);
    let needs = {
        let mut tx = db.begin_write().await.unwrap();
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
                review_status: CandidateReviewStatus::Agreed,
            })
        })
        .collect();
    let mut tx = db.begin_write().await.unwrap();
    insert_bank_items_in(&mut tx, &published.program_id, &items, None).await?;
    tx.commit().await.unwrap();

    let model = Recorder::new();
    let context = claimed(&db, &job_id).await?;
    let outcome = worker(&db, model.clone()).run(&context).await?;
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
    let db = Database::new(pool.clone());
    let (published, job_id) = published(&db).await?;
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
        marks: Vec::new(),
    };
    let context = claimed(&db, &job_id).await?;
    assert!(
        context
            .put_checkpoint(LESSON_BANK_CHECKPOINT, &serde_json::to_value(&bank)?)
            .await?
    );

    let model = Recorder::new();
    let resumed = worker(&db, model.clone());
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
    let db = Database::new(pool.clone());
    let (published, job_id) = published(&db).await?;
    let context = claimed(&db, &job_id).await?;
    let failing = QuizBankWorker {
        database: db.clone(),
        load_llm: Arc::new(|| {
            Box::pin(async { Err(AppError::InvalidInput("the model is gone".into())) })
        }),
    };
    assert!(failing.run(&context).await.is_err());
    JobStore::new(db.clone())
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
        &db,
        &uuid::Uuid::new_v4().to_string(),
        &published.program_id,
        &lesson_scope(lesson),
        false,
    )
    .await?;
    assert_ne!(again, job_id);
    let model = Recorder::new();
    let context = claimed(&db, &again).await?;
    let _ = worker(&db, model.clone()).run(&context).await;
    assert!(
        !model.requests().is_empty(),
        "an ordinary prepare wrote nothing for the lesson"
    );
    Ok(())
}

#[tokio::test]
async fn a_lessons_quiz_is_preparing_while_its_quiz_job_is_unfinished() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool);
    let (published, job_id) = published(&db).await?;
    let repo = LearningAssessmentRepository::new(db.clone());
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

    claimed(&db, &job_id).await?;
    JobStore::new(db.clone())
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
        let (system, user) = crate::lessons::structured_call::task_messages(request);
        let answer = if system.starts_with("Write quiz questions") {
            let item = serde_json::json!({
                "prompt": "Which tray cracks along its base?",
                "options": [
                    {"text": "One left inside past noon", "verdict": "right", "reason": ""},
                    {"text": "One fired before dawn", "verdict": "wrong", "reason": "A tray fired before dawn is not left inside."},
                    {"text": "One glazed at dusk", "verdict": "wrong", "reason": "Glazing does not crack the base."},
                    {"text": "One stacked on the shelf", "verdict": "wrong", "reason": "Stacking does not crack the base."}
                ],
                "explanation": "A tray left inside past noon cracks along its base.",
                "quote": [],
                "difficulty": "2"
            });
            let context: serde_json::Value = serde_json::from_str(user).unwrap_or_default();
            if context["repair"].is_null() {
                serde_json::json!({ "items": [item] }).to_string()
            } else {
                item.to_string()
            }
        } else {
            "{\"issues\":[]}".to_owned()
        };
        Ok(CompletionResponse::from_text(answer))
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
    fn enforces(
        &self,
        _schema: &lattice_core::application::services::model_contract::shape::Schema,
    ) -> bool {
        false
    }
}

#[tokio::test]
async fn a_lesson_quiz_job_keeps_what_its_checks_left_out() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool);
    let (_, job_id) = published(&db).await?;
    let context = claimed(&db, &job_id).await?;
    let llm: Arc<dyn LLMPort> = Arc::new(Unciting);
    let quiz_worker = QuizBankWorker {
        database: db.clone(),
        load_llm: Arc::new(move || {
            let llm = llm.clone();
            Box::pin(async move { Ok(crate::generation_jobs::pinned_for_test(llm)) })
        }),
    };
    let outcome = quiz_worker.run(&context).await?;

    let saved = JobStore::new(db.clone())
        .checkpoint(&job_id, LESSON_BANK_CHECKPOINT)
        .await?
        .unwrap();
    let dropped = saved["dropped"].as_array().map_or(0, Vec::len);
    assert!(
        dropped > 0,
        "the stage kept no reason for its refusals: {saved}"
    );
    let JobOutcome::Completed { message, .. } = outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(
        message,
        format!("0 questions written; {dropped} failed the checks and were left out. The quiz cannot start yet: its questions could not fill a first form.")
    );
    Ok(())
}

#[tokio::test]
async fn lesson_stage_report_reads_a_completed_jobs_numbers() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (published, job_id) = published(&db).await?;
    assert_eq!(lesson_stage_report(&db, &job_id).await?, None);

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
        marks: Vec::new(),
    };
    let context = claimed(&db, &job_id).await?;
    assert_eq!(lesson_stage_report(&db, &job_id).await?, None);
    assert!(
        context
            .put_checkpoint(LESSON_BANK_CHECKPOINT, &serde_json::to_value(&bank)?)
            .await?
    );
    let outcome = worker(&db, Recorder::new()).run(&context).await?;
    let JobOutcome::Completed {
        result_ref,
        message,
    } = outcome
    else {
        panic!("{outcome:?}");
    };
    assert_eq!(
        message,
        "1 questions written; 2 failed the checks and were left out. The quiz cannot start yet: its questions could not fill a first form."
    );
    JobStore::new(db.clone())
        .complete(&job_id, &result_ref, &message)
        .await?;

    let stored_rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM learning_assessment_candidates WHERE id=?")
            .bind(&item_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let report = lesson_stage_report(&db, &job_id).await?.unwrap();
    assert_eq!(i64::try_from(report.written).unwrap(), stored_rows);
    assert_eq!(report.dropped, bank.dropped);
    Ok(())
}

/// What the authoring requests asked for, in order: (format, returnCount).
fn authoring_asks(requests: &[String]) -> Vec<(String, u64)> {
    requests
        .iter()
        .filter_map(|request| serde_json::from_str::<serde_json::Value>(request).ok())
        .filter_map(|request| {
            Some((
                request.get("format")?.as_str()?.to_owned(),
                request.get("returnCount")?.as_u64()?,
            ))
        })
        .collect()
}

/// A lesson's blueprint asks one item of every format but single choice, which asks three, and the lesson's
/// own questions already fill those three: a first form's worth of every other format comes before any more.
fn assert_first_forms_come_first(requests: &[String]) {
    let asks = authoring_asks(requests);
    assert!(!asks.is_empty(), "no authoring request was made");
    for (format, count) in &asks {
        if format != "single_choice" {
            assert!(
                *count <= 1,
                "{format} was asked for {count} items before its first form was written: {asks:?}"
            );
        }
    }
    let last_other = asks
        .iter()
        .rposition(|(format, _)| format != "single_choice");
    let first_more = asks.iter().position(|(_, count)| *count > 3);
    if let (Some(last_other), Some(first_more)) = (last_other, first_more) {
        assert!(
            last_other < first_more,
            "more than a form's worth was asked before every first form: {asks:?}"
        );
    }
}

#[tokio::test]
async fn a_lesson_quiz_job_asks_first_for_one_forms_worth_of_each_format() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool);
    let (_, job_id) = published(&db).await?;
    let model = Recorder::new();
    let context = claimed(&db, &job_id).await?;

    worker(&db, model.clone()).run(&context).await?;

    assert_first_forms_come_first(&model.requests());
    Ok(())
}

#[tokio::test]
async fn preparing_a_quiz_asks_first_for_one_forms_worth_of_each_format() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool);
    let (published, _) = published(&db).await?;
    let model = Recorder::new();

    crate::quiz_bank_jobs::prepare_bank_now(
        &db,
        model.as_ref(),
        &published.program_id,
        &lesson_scope(&published.lesson_id),
        false,
    )
    .await?;

    assert_first_forms_come_first(&model.requests());
    Ok(())
}

#[tokio::test]
async fn a_quiz_job_whose_first_form_cannot_be_filled_says_so() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool);
    let (_, job_id) = published(&db).await?;
    let context = claimed(&db, &job_id).await?;

    let outcome = worker(&db, Recorder::new()).run(&context).await?;

    let JobOutcome::Completed { message, .. } = outcome else {
        panic!("{outcome:?}");
    };
    assert!(
        message.contains("The quiz cannot start yet"),
        "the job said nothing of it: {message}"
    );
    Ok(())
}

#[tokio::test]
async fn a_quiz_job_over_a_bank_short_of_a_format_but_full_of_the_outcome_can_start() -> Result<()>
{
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool);
    let (published, job_id) = published(&db).await?;
    let scope = lesson_scope(&published.lesson_id);
    let needs =
        LearningAssessmentRepository::bank_needs(&db, &published.program_id, &scope, false).await?;
    assert!(!needs.is_empty(), "the lesson's own questions meet no need");
    // Every missing question is written as single choice, so no other format has an item.
    let items: Vec<BankItem> = needs
        .iter()
        .flat_map(|need| {
            (0..need.count).map(|_| BankItem {
                item: stored(
                    &uuid::Uuid::new_v4().to_string(),
                    &need.requirement.outcome_id,
                    LearningItemFormat::SingleChoice,
                    need.requirement.difficulty_min,
                ),
                source_quote: String::new(),
                lesson_id: Some(published.lesson_id.clone()),
                origin: "lesson",
                review_status: CandidateReviewStatus::Agreed,
            })
        })
        .collect();
    let mut tx = db.begin_write().await.unwrap();
    insert_bank_items_in(&mut tx, &published.program_id, &items, None).await?;
    tx.commit().await.unwrap();

    let context = claimed(&db, &job_id).await?;
    let outcome = worker(&db, Recorder::new()).run(&context).await?;

    let JobOutcome::Completed { message, .. } = outcome else {
        panic!("{outcome:?}");
    };
    assert!(
        !message.contains("cannot start"),
        "the job said the quiz cannot start: {message}"
    );
    Ok(())
}

#[tokio::test]
async fn a_lesson_edited_while_its_bank_stage_runs_is_read_again() -> Result<()> {
    use crate::edits::{content_edits, keys};
    use crate::lessons::generation_jobs::pinned_for_test;
    use lattice_core::shared::edits::{ContentEditRequest, EditOp, EditSource};
    use std::sync::atomic::{AtomicBool, Ordering};

    const EDITED_MARK: &str = "quibbleflux";

    struct EditOnce {
        inner: Arc<Recorder>,
        edits: Arc<lattice_core::shared::edits::ContentEdits>,
        lesson_id: String,
        edited: AtomicBool,
    }

    #[async_trait::async_trait]
    impl LLMPort for EditOnce {
        async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
            if !self.edited.swap(true, Ordering::SeqCst) {
                let content = keys::section_ref(&self.lesson_id, 0);
                let opened = self.edits.open(&content).await?;
                self.edits
                    .apply(&ContentEditRequest {
                        edit_id: uuid::Uuid::new_v4().to_string(),
                        content,
                        expected_revision: opened.revision,
                        ops: vec![EditOp::Replace {
                            path: "/body".into(),
                            value: serde_json::json!(format!(
                                "Updated lesson section containing {EDITED_MARK}."
                            )),
                        }],
                        source: EditSource::Learner,
                        target: None,
                    })
                    .await?;
            }
            self.inner.complete(request).await
        }
        fn model_name(&self) -> &str {
            self.inner.model_name()
        }
        fn count_tokens(&self, text: &str) -> usize {
            self.inner.count_tokens(text)
        }
        fn max_context_tokens(&self) -> usize {
            self.inner.max_context_tokens()
        }
        async fn is_ready(&self) -> Result<bool> {
            self.inner.is_ready().await
        }
        fn enforces(
            &self,
            schema: &lattice_core::application::services::model_contract::shape::Schema,
        ) -> bool {
            self.inner.enforces(schema)
        }
    }

    let pool = crate::tests::pool().await?;
    let db = Database::new(pool);
    let (published, job_id) = published(&db).await?;
    let inner = Recorder::new();
    let llm: Arc<dyn LLMPort> = Arc::new(EditOnce {
        inner: inner.clone(),
        edits: content_edits(&db)?,
        lesson_id: published.lesson_id.clone(),
        edited: AtomicBool::new(false),
    });
    let worker = QuizBankWorker {
        database: db.clone(),
        load_llm: Arc::new(move || {
            let llm = llm.clone();
            Box::pin(async move { Ok(pinned_for_test(llm)) })
        }),
    };

    let context = claimed(&db, &job_id).await?;
    let outcome = worker.run(&context).await;
    assert!(
        matches!(outcome, Ok(JobOutcome::Completed { .. })),
        "{outcome:?}"
    );

    let requests = inner.requests();
    assert!(
        requests.len() >= 2,
        "expected re-run after edit, got {} request(s)",
        requests.len()
    );
    assert!(
        requests.last().is_some_and(|r| r.contains(EDITED_MARK)),
        "second pass did not read the edited lesson text: {requests:?}"
    );
    Ok(())
}

#[tokio::test]
async fn unverified_claims_are_stripped_from_material_and_exclude_seed_and_bank_items_until_confirmed(
) -> Result<()> {
    use lattice_core::domain::learning::claim_review::{
        Actor, ClaimEvent, ClaimId, ClaimLocation, EventProvenance, HumanBinding, UnitKind,
        UnverifiedReason, UserAction,
    };

    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (published, job_id) = published(&db).await?;
    let outcome_id = lesson_outcome(&pool, &published.lesson_id).await;
    let mut tx = db.begin_write().await?;
    let now = 1_700_000_000i64;
    let prov = EventProvenance {
        actor: Actor::Machine,
        at: now,
        job_id: None,
        app_version: "test".into(),
        checker_model: None,
    };
    let lesson_id = &published.lesson_id;
    let program_id = &published.program_id;
    let block_count = usize::try_from(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM learning_blocks WHERE lesson_id=?")
            .bind(lesson_id)
            .fetch_one(&mut *tx)
            .await?,
    )
    .unwrap();
    assert!(block_count > 0);
    let block0_body: String =
        sqlx::query_scalar("SELECT body FROM learning_blocks WHERE lesson_id=? AND ordinal=0")
            .bind(lesson_id)
            .fetch_one(&mut *tx)
            .await?;
    let poisoned_sentence = "The recommended access token lifetime is 300 seconds.";
    let clean_sentence = "Verified systems separate public configuration from private keys.";
    let new_body = format!("{clean_sentence} {poisoned_sentence}");
    sqlx::query("UPDATE learning_blocks SET body=? WHERE lesson_id=? AND ordinal=0")
        .bind(&new_body)
        .bind(lesson_id)
        .execute(&mut *tx)
        .await?;
    let _ = block0_body;

    // Seed claim ledger review row + unverified claim on block 0 and seed question 0.
    sqlx::query(
        "INSERT OR REPLACE INTO learning_lesson_reviews(lesson_id,program_id,job_id,state,revision,draft_sha256,checker_model,opened_at,updated_at) VALUES(?,?,?,'ready',1,'sha','m',?,?)",
    )
    .bind(lesson_id)
    .bind(program_id)
    .bind(&job_id)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    let c_block = ClaimId("c-block-unverified".into());
    crate::lessons::claim_reviews::repository::append_for_test_in(
        &mut tx,
        lesson_id,
        &c_block,
        prov.clone(),
        ClaimEvent::Extracted {
            location: ClaimLocation::locate(
                0,
                UnitKind::Block,
                "body",
                poisoned_sentence,
                &new_body,
            ),
            statement: poisoned_sentence.into(),
        },
    )
    .await?;
    crate::lessons::claim_reviews::repository::append_for_test_in(
        &mut tx,
        lesson_id,
        &c_block,
        prov.clone(),
        ClaimEvent::PublishedUnverified {
            reason: UnverifiedReason::Unsupported,
        },
    )
    .await?;

    let c_question = ClaimId("c-question-unverified".into());
    crate::lessons::claim_reviews::repository::append_for_test_in(
        &mut tx,
        lesson_id,
        &c_question,
        prov.clone(),
        ClaimEvent::Extracted {
            location: ClaimLocation::locate(
                block_count,
                UnitKind::AnswerKey,
                "explanation",
                poisoned_sentence,
                poisoned_sentence,
            ),
            statement: poisoned_sentence.into(),
        },
    )
    .await?;
    crate::lessons::claim_reviews::repository::append_for_test_in(
        &mut tx,
        lesson_id,
        &c_question,
        prov.clone(),
        ClaimEvent::PublishedUnverified {
            reason: UnverifiedReason::Unsupported,
        },
    )
    .await?;

    let tainted_bank_id = uuid::Uuid::new_v4().to_string();
    insert_bank_items_in(
        &mut tx,
        program_id,
        &[BankItem {
            item: stored(
                &tainted_bank_id,
                &outcome_id,
                LearningItemFormat::SingleChoice,
                2,
            ),
            source_quote: poisoned_sentence.into(),
            lesson_id: Some(lesson_id.clone()),
            origin: "bank",
            review_status: CandidateReviewStatus::Agreed,
        }],
        None,
    )
    .await?;
    crate::assessment_repository::sync_disputed_questions_in(&mut tx, lesson_id).await?;
    tx.commit().await?;

    // 1. Authoring material omits the unverified sentence while keeping the verified sentence.
    let mut read = db.begin_read().await?;
    let material = LearningAssessmentRepository::lesson_material_in(&mut read, lesson_id).await?;
    assert!(
        !material[0].text.contains("300 seconds"),
        "{}",
        material[0].text
    );
    assert!(
        material[0].text.contains(clean_sentence),
        "{}",
        material[0].text
    );

    // 2. Seeds & bank draws exclude both the unverified lesson seed question and tainted bank item.
    let seeds =
        LearningAssessmentRepository::lesson_seeds_in(&mut read, program_id, lesson_id).await?;
    let seed0_id: String = sqlx::query_scalar(
        "SELECT id FROM learning_assessment_candidates WHERE lesson_id=? AND origin='lesson' ORDER BY rowid LIMIT 1",
    )
    .bind(lesson_id)
    .fetch_one(&mut *read)
    .await?;
    assert!(seeds.iter().all(|s| s.id != seed0_id));
    let bank = LearningAssessmentRepository::bank_in(&mut read, program_id).await?;
    assert!(bank.iter().all(|b| b.candidate.id != seed0_id));
    assert!(bank.iter().all(|b| b.candidate.id != tainted_bank_id));
    read.commit().await?;

    // 3. Confirming the unverified question claim restores it to 'agreed' and unblocks it.
    let mut tx2 = db.begin_write().await?;
    let user_prov = EventProvenance {
        actor: Actor::User,
        ..prov.clone()
    };
    crate::lessons::claim_reviews::repository::append_for_test_in(
        &mut tx2,
        lesson_id,
        &c_question,
        user_prov,
        ClaimEvent::UserAction {
            action: UserAction::Confirm {
                bound: HumanBinding::new(poisoned_sentence, Vec::new()),
            },
            note: Some("Verified internally".into()),
        },
    )
    .await?;
    crate::assessment_repository::sync_disputed_questions_in(&mut tx2, lesson_id).await?;
    tx2.commit().await?;

    let mut read2 = db.begin_read().await?;
    let bank_after = LearningAssessmentRepository::bank_in(&mut read2, program_id).await?;
    assert!(bank_after.iter().any(|b| b.candidate.id == seed0_id));
    Ok(())
}

#[tokio::test]
async fn unverified_material_checks_span_quotes_unlinked_bank_items_and_partial_repairs(
) -> Result<()> {
    use lattice_core::domain::learning::claim_review::{
        Actor, ClaimEvent, ClaimId, ClaimLocation, EventProvenance, HumanBinding, ReopenCause,
        UnitKind, UnverifiedReason, UserAction,
    };

    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (published, job_id) = published(&db).await?;
    let outcome_id = lesson_outcome(&pool, &published.lesson_id).await;
    let mut tx = db.begin_write().await?;
    let now = 1_700_000_100i64;
    let prov = EventProvenance {
        actor: Actor::Machine,
        at: now,
        job_id: None,
        app_version: "test".into(),
        checker_model: None,
    };
    let lesson_id = &published.lesson_id;
    let program_id = &published.program_id;
    let block_count = usize::try_from(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM learning_blocks WHERE lesson_id=?")
            .bind(lesson_id)
            .fetch_one(&mut *tx)
            .await?,
    )
    .unwrap();
    let verified_first = "Verified systems rotate client keys using explicit kid headers.";
    let unverified_word_exp = "Each assertion sets exp strictly.";
    let unverified_later = "The default token expiry in this profile is 420 seconds.";
    let new_body = format!("{verified_first} {unverified_word_exp} {unverified_later}");
    sqlx::query("UPDATE learning_blocks SET body=? WHERE lesson_id=? AND ordinal=0")
        .bind(&new_body)
        .bind(lesson_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query(
        "INSERT OR REPLACE INTO learning_lesson_reviews(lesson_id,program_id,job_id,state,revision,draft_sha256,checker_model,opened_at,updated_at) VALUES(?,?,?,'ready',1,'sha','m',?,?)",
    )
    .bind(lesson_id)
    .bind(program_id)
    .bind(&job_id)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    // Stale byte span points inside `verified_first`, while `quote` names `unverified_later`.
    let shifted_claim = ClaimId("c-shifted-span".into());
    crate::lessons::claim_reviews::repository::append_for_test_in(
        &mut tx,
        lesson_id,
        &shifted_claim,
        prov.clone(),
        ClaimEvent::Extracted {
            location: ClaimLocation {
                unit_index: 0,
                unit_kind: UnitKind::Block,
                field: "body".into(),
                quote: unverified_later.into(),
                span: Some((0, verified_first.len())),
            },
            statement: unverified_later.into(),
        },
    )
    .await?;
    crate::lessons::claim_reviews::repository::append_for_test_in(
        &mut tx,
        lesson_id,
        &shifted_claim,
        prov.clone(),
        ClaimEvent::PublishedUnverified {
            reason: UnverifiedReason::Unsupported,
        },
    )
    .await?;

    // Short (<20-char) unverified quote must not taint unrelated longer bank source_quotes.
    let short_claim = ClaimId("c-short-quote".into());
    crate::lessons::claim_reviews::repository::append_for_test_in(
        &mut tx,
        lesson_id,
        &short_claim,
        prov.clone(),
        ClaimEvent::Extracted {
            location: ClaimLocation {
                unit_index: 0,
                unit_kind: UnitKind::Block,
                field: "body".into(),
                quote: "exp".into(),
                span: None,
            },
            statement: "exp field".into(),
        },
    )
    .await?;
    crate::lessons::claim_reviews::repository::append_for_test_in(
        &mut tx,
        lesson_id,
        &short_claim,
        prov.clone(),
        ClaimEvent::PublishedUnverified {
            reason: UnverifiedReason::Unsupported,
        },
    )
    .await?;

    // Two unverified claims on seed question 0.
    for (cid, kind, field) in [
        ("c-q-prompt", UnitKind::Question, "prompt"),
        ("c-q-expl", UnitKind::AnswerKey, "explanation"),
    ] {
        let claim_id = ClaimId(cid.into());
        crate::lessons::claim_reviews::repository::append_for_test_in(
            &mut tx,
            lesson_id,
            &claim_id,
            prov.clone(),
            ClaimEvent::Extracted {
                location: ClaimLocation::locate(
                    block_count,
                    kind,
                    field,
                    unverified_later,
                    unverified_later,
                ),
                statement: unverified_later.into(),
            },
        )
        .await?;
        crate::lessons::claim_reviews::repository::append_for_test_in(
            &mut tx,
            lesson_id,
            &claim_id,
            prov.clone(),
            ClaimEvent::PublishedUnverified {
                reason: UnverifiedReason::Unsupported,
            },
        )
        .await?;
    }

    // Module/course bank item (`lesson_id: None`) referencing `unverified_later`, plus a safe item
    // whose quote happens to contain the short `"exp"` token.
    let unlinked_tainted_id = uuid::Uuid::new_v4().to_string();
    let safe_short_match_id = uuid::Uuid::new_v4().to_string();
    insert_bank_items_in(
        &mut tx,
        program_id,
        &[
            BankItem {
                item: stored(
                    &unlinked_tainted_id,
                    &outcome_id,
                    LearningItemFormat::SingleChoice,
                    2,
                ),
                source_quote: unverified_later.into(),
                lesson_id: None,
                origin: "bank",
                review_status: CandidateReviewStatus::Agreed,
            },
            BankItem {
                item: stored(
                    &safe_short_match_id,
                    &outcome_id,
                    LearningItemFormat::SingleChoice,
                    2,
                ),
                source_quote: "Validate client_assertion expiration claims strictly.".into(),
                lesson_id: None,
                origin: "bank",
                review_status: CandidateReviewStatus::Agreed,
            },
        ],
        None,
    )
    .await?;
    crate::assessment_repository::sync_disputed_questions_in(&mut tx, lesson_id).await?;
    tx.commit().await?;

    let mut read = db.begin_read().await?;
    let material = LearningAssessmentRepository::lesson_material_in(&mut read, lesson_id).await?;
    assert!(
        material[0].text.contains(verified_first),
        "{}",
        material[0].text
    );
    assert!(
        !material[0].text.contains("sets exp strictly"),
        "{}",
        material[0].text
    );
    assert!(
        !material[0].text.contains("420 seconds"),
        "{}",
        material[0].text
    );

    let bank = LearningAssessmentRepository::bank_in(&mut read, program_id).await?;
    assert!(bank.iter().all(|b| b.candidate.id != unlinked_tainted_id));
    assert!(bank.iter().any(|b| b.candidate.id == safe_short_match_id));
    let seed0_id: String = sqlx::query_scalar(
        "SELECT id FROM learning_assessment_candidates WHERE lesson_id=? AND origin='lesson' ORDER BY rowid LIMIT 1",
    )
    .bind(lesson_id)
    .fetch_one(&mut *read)
    .await?;
    read.commit().await?;

    // Confirming only `c-q-prompt` leaves seed 0 `disputed` because `c-q-expl` is still unverified.
    let mut tx2 = db.begin_write().await?;
    let user_prov = EventProvenance {
        actor: Actor::User,
        ..prov.clone()
    };
    crate::lessons::claim_reviews::repository::append_for_test_in(
        &mut tx2,
        lesson_id,
        &ClaimId("c-q-prompt".into()),
        user_prov.clone(),
        ClaimEvent::UserAction {
            action: UserAction::Confirm {
                bound: HumanBinding::new(unverified_later, Vec::new()),
            },
            note: None,
        },
    )
    .await?;
    crate::assessment_repository::sync_disputed_questions_in(&mut tx2, lesson_id).await?;
    let status_partial: String =
        sqlx::query_scalar("SELECT review_status FROM learning_assessment_candidates WHERE id=?")
            .bind(&seed0_id)
            .fetch_one(&mut *tx2)
            .await?;
    assert_eq!(status_partial, "disputed");

    // Confirming `c-q-expl` restores `'agreed'`, and reopening flips back to `'disputed'`.
    crate::lessons::claim_reviews::repository::append_for_test_in(
        &mut tx2,
        lesson_id,
        &ClaimId("c-q-expl".into()),
        user_prov,
        ClaimEvent::UserAction {
            action: UserAction::Confirm {
                bound: HumanBinding::new(unverified_later, vec!["v-src".into()]),
            },
            note: None,
        },
    )
    .await?;
    crate::assessment_repository::sync_disputed_questions_in(&mut tx2, lesson_id).await?;
    let status_agreed: String =
        sqlx::query_scalar("SELECT review_status FROM learning_assessment_candidates WHERE id=?")
            .bind(&seed0_id)
            .fetch_one(&mut *tx2)
            .await?;
    assert_eq!(status_agreed, "agreed");

    crate::lessons::claim_reviews::repository::append_for_test_in(
        &mut tx2,
        lesson_id,
        &ClaimId("c-q-expl".into()),
        prov,
        ClaimEvent::Reopened {
            cause: ReopenCause::SourceDeleted("v-src".into()),
        },
    )
    .await?;
    crate::assessment_repository::sync_disputed_questions_in(&mut tx2, lesson_id).await?;
    let status_reopened: String =
        sqlx::query_scalar("SELECT review_status FROM learning_assessment_candidates WHERE id=?")
            .bind(&seed0_id)
            .fetch_one(&mut *tx2)
            .await?;
    assert_eq!(status_reopened, "disputed");
    tx2.commit().await?;
    Ok(())
}

#[tokio::test]
async fn lesson_material_strips_code_fences_and_preserves_verbatim_contiguous_runs() -> Result<()> {
    use lattice_core::domain::learning::claim_review::{
        Actor, ClaimEvent, ClaimId, ClaimLocation, EventProvenance, UnitKind, UnverifiedReason,
    };
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool);
    let (published, job_id) = published(&db).await?;
    let lesson_id = &published.lesson_id;
    let program_id = &published.program_id;

    let verified_a = "First verified sentence explains token discovery clearly.";
    let verified_b = "Second verified sentence explains client_assertion expiration.";
    let unverified_mid = "Unverified middle claim missing a source.";
    let verified_c = "Third verified sentence covers bearer authorization headers.";
    let raw_body = format!(
        "{verified_a}\n\n```typescript\nasync function demo() {{\n  return true;\n}}\n```\n\n{verified_b} {unverified_mid} {verified_c}"
    );
    let mut tx = db.begin_write().await?;
    sqlx::query("UPDATE learning_blocks SET body=? WHERE lesson_id=? AND ordinal=0")
        .bind(&raw_body)
        .bind(lesson_id)
        .execute(&mut *tx)
        .await?;
    let now = 1_700_000_000_000i64;
    sqlx::query(
        "INSERT OR REPLACE INTO learning_lesson_reviews(lesson_id,program_id,job_id,state,revision,draft_sha256,checker_model,opened_at,updated_at) VALUES(?,?,?,'ready',1,'sha','m',?,?)",
    )
    .bind(lesson_id)
    .bind(program_id)
    .bind(&job_id)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    let prov = EventProvenance {
        actor: Actor::Machine,
        at: now,
        job_id: None,
        app_version: "0.1.0".into(),
        checker_model: Some("test-model".into()),
    };
    crate::lessons::claim_reviews::repository::append_for_test_in(
        &mut tx,
        lesson_id,
        &ClaimId("c-mid".into()),
        prov.clone(),
        ClaimEvent::Extracted {
            location: ClaimLocation::locate(
                0,
                UnitKind::Block,
                "body",
                unverified_mid,
                unverified_mid,
            ),
            statement: unverified_mid.into(),
        },
    )
    .await?;
    crate::lessons::claim_reviews::repository::append_for_test_in(
        &mut tx,
        lesson_id,
        &ClaimId("c-mid".into()),
        prov,
        ClaimEvent::PublishedUnverified {
            reason: UnverifiedReason::Unsupported,
        },
    )
    .await?;
    tx.commit().await?;

    let mut read = db.begin_read().await?;
    let material = LearningAssessmentRepository::lesson_material_in(&mut read, lesson_id).await?;
    read.commit().await?;
    assert!(!material.is_empty());
    let text = &material[0].text;
    assert!(!text.contains("```"), "{text}");
    assert!(!text.contains("return true;"), "{text}");
    assert!(!text.contains(unverified_mid), "{text}");
    assert!(
        text.contains(&format!("{verified_a}\n\n\n{verified_b}")),
        "{text}"
    );
    assert!(
        text.contains(&format!("{verified_b}\n\n\n{verified_c}")),
        "{text}"
    );
    assert!(crate::grade_rationale::verbatim(&raw_body, verified_a));
    assert!(crate::grade_rationale::verbatim(&raw_body, verified_b));
    assert!(crate::grade_rationale::verbatim(&raw_body, verified_c));
    Ok(())
}

#[test]
fn lesson_question_item_clears_right_choice_rationale_before_bank_validation() {
    let question_id = uuid::Uuid::new_v4().to_string();
    let outcome_id = uuid::Uuid::new_v4().to_string();
    let q = LessonQuestion {
        id: question_id.clone(),
        kind: LessonQuestionKind::Quiz,
        prompt: "Which scope creates a Binary resource?".into(),
        options: vec![
            "system/Binary.c".into(),
            "system/Binary.r".into(),
            "system/Binary.u".into(),
            "system/Binary.d".into(),
        ],
        source_ids: vec![],
    };
    let key = LessonAnswerKey {
        question_id,
        correct_index: 0,
        explanation: "SMART v2 uses .c for create permissions.".into(),
        rationales: vec![
            "Correct because .c creates Binary resources.".into(),
            "Read-only does not create resources.".into(),
            "Update modifies existing resources.".into(),
            "Delete removes resources.".into(),
        ],
    };
    let candidate = crate::quiz_bank_jobs::lesson_question_item(&q, &key, vec![outcome_id]);
    assert_eq!(
        candidate.distractor_rationales.first().map(String::as_str),
        Some("")
    );
    assert!(crate::assessment_engine::validate_candidate(&candidate).is_ok());
}

#[tokio::test]
async fn block_dominated_by_no_source_claims_excludes_orphan_scenario_and_taints_bank() -> Result<()>
{
    use lattice_core::domain::learning::acceptance::{
        End, Outcome as StepOutcome, Reading, Step, Then,
    };
    use lattice_core::domain::learning::claim_review::{
        Actor, ClaimEvent, ClaimId, ClaimLocation, EventProvenance, UnitKind, UnverifiedReason,
    };

    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (published, job_id) = published(&db).await?;
    let lesson_id = &published.lesson_id;
    let outcome_id = lesson_outcome(&pool, lesson_id).await;
    let mut tx = db.begin_write().await?;
    let now = 1_700_000_200i64;
    sqlx::query(
        "INSERT OR REPLACE INTO learning_lesson_reviews(lesson_id,program_id,job_id,state,revision,draft_sha256,checker_model,opened_at,updated_at) VALUES(?,?,?,'ready',1,'sha','m',?,?)",
    )
    .bind(lesson_id)
    .bind(&published.program_id)
    .bind(&job_id)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    let prov = EventProvenance {
        actor: Actor::Machine,
        at: now,
        job_id: None,
        app_version: "test".into(),
        checker_model: Some("test-model".into()),
    };

    let scenario_setup =
        "Scenario A: You omit the configuration flag assuming R4 works by default.";
    let unverified_c1 = "Proprietary validation fails at onboarding when this setting is omitted.";
    let unverified_c2 = "Some endpoints silently fall back to legacy defaults.";
    let block0_body = format!("{scenario_setup} {unverified_c1} {unverified_c2}");
    sqlx::query("UPDATE learning_blocks SET body=? WHERE lesson_id=? AND ordinal=0")
        .bind(&block0_body)
        .bind(lesson_id)
        .execute(&mut *tx)
        .await?;

    let mut clean_ordinal = None;
    let blocks: Vec<(i64,)> = sqlx::query_as(
        "SELECT ordinal FROM learning_blocks WHERE lesson_id=? AND ordinal > 0 ORDER BY ordinal",
    )
    .bind(lesson_id)
    .fetch_all(&mut *tx)
    .await?;
    if let Some((ord,)) = blocks.first() {
        clean_ordinal = Some(*ord);
        let clean_claim_text =
            "HTTP 429 responses may return a Retry-After header dictating wait time.";
        sqlx::query("UPDATE learning_blocks SET body=? WHERE lesson_id=? AND ordinal=?")
            .bind(clean_claim_text)
            .bind(lesson_id)
            .bind(ord)
            .execute(&mut *tx)
            .await?;
        let cid = ClaimId("c-clean".into());
        crate::lessons::claim_reviews::repository::append_for_test_in(
            &mut tx,
            lesson_id,
            &cid,
            prov.clone(),
            ClaimEvent::Extracted {
                location: ClaimLocation::locate(
                    usize::try_from(*ord).unwrap_or(1),
                    UnitKind::Block,
                    "body",
                    clean_claim_text,
                    clean_claim_text,
                ),
                statement: clean_claim_text.into(),
            },
        )
        .await?;
        crate::lessons::claim_reviews::repository::append_for_test_in(
            &mut tx,
            lesson_id,
            &cid,
            prov.clone(),
            ClaimEvent::Stepped {
                policy: "acceptance-v4".into(),
                step: Step::Read(Reading::Consistency),
                outcome: StepOutcome::Stands,
                then: Then::End(End::Consistent),
                reason: "Consistent fallback".into(),
                rewrite_ahead: false,
            },
        )
        .await?;
    }

    let c_scenario = ClaimId("c-scenario".into());
    crate::lessons::claim_reviews::repository::append_for_test_in(
        &mut tx,
        lesson_id,
        &c_scenario,
        prov.clone(),
        ClaimEvent::Extracted {
            location: ClaimLocation::locate(
                0,
                UnitKind::Block,
                "body",
                scenario_setup,
                scenario_setup,
            ),
            statement: scenario_setup.into(),
        },
    )
    .await?;
    crate::lessons::claim_reviews::repository::append_for_test_in(
        &mut tx,
        lesson_id,
        &c_scenario,
        prov.clone(),
        ClaimEvent::Stepped {
            policy: "acceptance-v4".into(),
            step: Step::Read(Reading::Consistency),
            outcome: StepOutcome::Stands,
            then: Then::End(End::Consistent),
            reason: "Internal scenario setup".into(),
            rewrite_ahead: false,
        },
    )
    .await?;

    for (cid, sentence) in [("c-u1", unverified_c1), ("c-u2", unverified_c2)] {
        let id = ClaimId(cid.into());
        crate::lessons::claim_reviews::repository::append_for_test_in(
            &mut tx,
            lesson_id,
            &id,
            prov.clone(),
            ClaimEvent::Extracted {
                location: ClaimLocation::locate(0, UnitKind::Block, "body", sentence, sentence),
                statement: sentence.into(),
            },
        )
        .await?;
        crate::lessons::claim_reviews::repository::append_for_test_in(
            &mut tx,
            lesson_id,
            &id,
            prov.clone(),
            ClaimEvent::PublishedUnverified {
                reason: UnverifiedReason::Unsupported,
            },
        )
        .await?;
    }

    let orphan_bank_id = uuid::Uuid::new_v4().to_string();
    insert_bank_items_in(
        &mut tx,
        &published.program_id,
        &[BankItem {
            item: stored(
                &orphan_bank_id,
                &outcome_id,
                LearningItemFormat::SingleChoice,
                2,
            ),
            source_quote: scenario_setup.into(),
            lesson_id: Some(lesson_id.clone()),
            origin: "bank",
            review_status: CandidateReviewStatus::Agreed,
        }],
        None,
    )
    .await?;
    tx.commit().await?;

    let mut read = db.begin_read().await?;
    let material = LearningAssessmentRepository::lesson_material_in(&mut read, lesson_id).await?;
    assert!(
        clean_ordinal.is_none() || material.iter().all(|m| !m.text.contains("Scenario A")),
        "orphan scenario block should be excluded when clean blocks remain: {material:?}"
    );
    let bank = LearningAssessmentRepository::bank_in(&mut read, &published.program_id).await?;
    assert!(
        bank.iter().all(|b| b.candidate.id != orphan_bank_id),
        "bank item quoting orphan scenario from unverified-dominated block should be held back"
    );
    Ok(())
}

#[tokio::test]
async fn consistent_claims_count_as_grounded_against_minority_unverified_claims() -> Result<()> {
    use lattice_core::domain::learning::acceptance::{
        End, Outcome as StepOutcome, Reading, Step, Then,
    };
    use lattice_core::domain::learning::claim_review::{
        Actor, ClaimEvent, ClaimId, ClaimLocation, EventProvenance, UnitKind, UnverifiedReason,
    };

    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (published, job_id) = published(&db).await?;
    let lesson_id = &published.lesson_id;
    let mut tx = db.begin_write().await?;
    let now = 1_700_000_300i64;
    sqlx::query(
        "INSERT OR REPLACE INTO learning_lesson_reviews(lesson_id,program_id,job_id,state,revision,draft_sha256,checker_model,opened_at,updated_at) VALUES(?,?,?,'ready',1,'sha','m',?,?)",
    )
    .bind(lesson_id)
    .bind(&published.program_id)
    .bind(&job_id)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    let marker = "Consistent token validation keeps active sessions healthy.";
    sqlx::query("UPDATE learning_blocks SET body=? WHERE lesson_id=? AND ordinal=0")
        .bind(marker)
        .bind(lesson_id)
        .execute(&mut *tx)
        .await?;

    let prov = EventProvenance {
        actor: Actor::Machine,
        at: now,
        job_id: None,
        app_version: "test".into(),
        checker_model: Some("test-model".into()),
    };
    for i in 0..8usize {
        let id = ClaimId(format!("c-cons-{i}"));
        crate::lessons::claim_reviews::repository::append_for_test_in(
            &mut tx,
            lesson_id,
            &id,
            prov.clone(),
            ClaimEvent::Extracted {
                location: ClaimLocation::locate(0, UnitKind::Block, "body", marker, marker),
                statement: format!("Consistent claim {i}."),
            },
        )
        .await?;
        crate::lessons::claim_reviews::repository::append_for_test_in(
            &mut tx,
            lesson_id,
            &id,
            prov.clone(),
            ClaimEvent::Stepped {
                policy: "acceptance-v4".into(),
                step: Step::Read(Reading::Consistency),
                outcome: StepOutcome::Stands,
                then: Then::End(End::Consistent),
                reason: "Consistent claim".into(),
                rewrite_ahead: false,
            },
        )
        .await?;
    }
    for i in 0..2usize {
        let id = ClaimId(format!("c-unv-{i}"));
        crate::lessons::claim_reviews::repository::append_for_test_in(
            &mut tx,
            lesson_id,
            &id,
            prov.clone(),
            ClaimEvent::Extracted {
                location: ClaimLocation::locate(0, UnitKind::Block, "body", marker, marker),
                statement: format!("Unverified claim {i}."),
            },
        )
        .await?;
        crate::lessons::claim_reviews::repository::append_for_test_in(
            &mut tx,
            lesson_id,
            &id,
            prov.clone(),
            ClaimEvent::PublishedUnverified {
                reason: UnverifiedReason::NoSource,
            },
        )
        .await?;
    }
    tx.commit().await?;

    let mut read = db.begin_read().await?;
    let material = LearningAssessmentRepository::lesson_material_in(&mut read, lesson_id).await?;
    assert!(
        material.iter().any(|m| m.text.contains(marker)),
        "block with 8 consistent, 0 verified, 2 unverified claims should remain grounded: {material:?}"
    );
    Ok(())
}
