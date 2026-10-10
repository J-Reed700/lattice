//! A synthesis as a job: carried on after a restart from the parts it saved,
//! and its staged result saved to the Journal once.
use super::*;
use crate::features::conversation::dto::MessageDto;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::Notify;

const WAIT: Duration = Duration::from_secs(10);

async fn pool() -> sqlx::SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

fn request() -> StartJournalSynthesisRequestDto {
    StartJournalSynthesisRequestDto {
        request: SynthesizeJournalEntriesRequestDto {
            conversation_ids: vec!["first".into(), "second".into()],
            scope: Some("conversation".into()),
            max_entries: Some(2),
        },
        destination: SynthesisDestinationDto::Note {
            note_id: "note-1".into(),
        },
        title: "Research".into(),
        heading: "Research".into(),
    }
}

/// Two parts and the merge; answers each prompt with notes on it, and stops
/// answering after `answers` of them, as an app about to close would.
struct FakeWork {
    answers: usize,
    answered: Mutex<Vec<String>>,
    removed: Mutex<Vec<String>>,
    stalled: Notify,
}

impl FakeWork {
    fn answering(answers: usize) -> Arc<Self> {
        Arc::new(Self {
            answers,
            answered: Mutex::default(),
            removed: Mutex::default(),
            stalled: Notify::new(),
        })
    }

    fn answered(&self) -> Vec<String> {
        self.answered.lock().unwrap().clone()
    }
}

#[async_trait]
impl SynthesisWork for FakeWork {
    async fn plan(&self, selection: &SynthesizeJournalEntriesRequestDto) -> Result<SynthesisPlan> {
        Ok(SynthesisPlan {
            scope: "conversation".into(),
            entry_count: 2,
            prompts: vec!["part one".into(), "part two".into()],
            conversation_ids: selection.conversation_ids.clone(),
            citations: Vec::new(),
            sources: Vec::new(),
        })
    }

    async fn open_scratch(&self) -> Result<String> {
        Ok(uuid::Uuid::new_v4().to_string())
    }

    async fn answer(&self, conversation_id: &str, prompt: &str) -> Result<ChatResponse> {
        if self.answered.lock().unwrap().len() >= self.answers {
            self.stalled.notify_one();
            std::future::pending::<()>().await;
        }
        self.answered.lock().unwrap().push(prompt.to_string());
        let notes = format!("notes on {}", prompt.lines().next().unwrap_or_default());
        Ok(ChatResponse {
            conversation_id: conversation_id.into(),
            message: notes.clone(),
            messages: vec![MessageDto {
                id: uuid::Uuid::new_v4().to_string(),
                conversation_id: conversation_id.into(),
                role: "assistant".into(),
                content: notes,
                tokens: 3,
                created_at: String::new(),
                metadata: None,
                status: "completed".into(),
            }],
            context_used: 0,
            sources: Vec::new(),
            timing_metrics: None,
        })
    }

    async fn remove_scratch(&self, conversation_id: String) {
        self.removed.lock().unwrap().push(conversation_id);
    }
}

async fn finished(jobs: &JobRuntime, id: &str) -> JobRecord {
    tokio::time::timeout(WAIT, async {
        loop {
            let job = jobs.store().get(id).await.unwrap();
            if job.status.is_finished() {
                jobs.finished(id).await;
                return job;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the synthesis never finished")
}

async fn runtime_with(pool: &sqlx::SqlitePool, work: Arc<FakeWork>) -> Arc<JobRuntime> {
    let jobs = JobRuntime::with_poll_interval(pool.clone(), Duration::from_millis(50));
    jobs.register(
        JOURNAL_SYNTHESIS,
        Arc::new(JournalSynthesisJob::with(work)),
        synthesis_job_config(),
    )
    .await
    .unwrap();
    jobs
}

#[tokio::test]
async fn a_synthesis_stopped_by_a_restart_resumes_at_the_first_part_it_had_not_saved() {
    let pool = pool().await;
    let before = FakeWork::answering(1);
    let jobs = runtime_with(&pool, before.clone()).await;
    let started = synthesize_journal_entries_impl(request(), &jobs)
        .await
        .unwrap();
    // The first part is saved; the app closes while the second is written.
    tokio::time::timeout(WAIT, before.stalled.notified())
        .await
        .unwrap();
    jobs.close();
    jobs.drain().await;
    assert_eq!(before.answered(), ["part one"]);
    assert_eq!(
        before.removed.lock().unwrap().len(),
        2,
        "no scratch conversation is left behind"
    );
    let saved = jobs.store().get(&started.job.id).await.unwrap();
    assert_eq!(saved.status, JobStatus::Pending);

    let after = FakeWork::answering(usize::MAX);
    let jobs = runtime_with(&pool, after.clone()).await;
    let job = finished(&jobs, &started.job.id).await;

    assert_eq!(job.status, JobStatus::Completed, "{:?}", job.error_message);
    let answered = after.answered();
    assert_eq!(answered.len(), 2, "the saved part is not written again");
    assert_eq!(answered[0], "part two");
    assert!(answered[1].contains("notes on part one") && answered[1].contains("notes on part two"));
    let result = get_journal_synthesis_result_impl(&job.id, &jobs)
        .await
        .unwrap();
    assert!(result
        .synthesis
        .starts_with("notes on Merge the chunk syntheses"));
    assert_eq!(result.chunk_count, 2);
    assert_eq!(result.conversation_ids, ["first", "second"]);
    jobs.close();
    jobs.drain().await;
}

#[tokio::test]
async fn a_synthesis_finished_while_closed_waits_staged_and_is_saved_once() {
    let pool = pool().await;
    let jobs = runtime_with(&pool, FakeWork::answering(usize::MAX)).await;
    let started = synthesize_journal_entries_impl(request(), &jobs)
        .await
        .unwrap();
    finished(&jobs, &started.job.id).await;
    jobs.close();
    jobs.drain().await;

    // The app opens again: the finished synthesis is listed with where it goes.
    let jobs = runtime_with(&pool, FakeWork::answering(0)).await;
    let listed = list_journal_syntheses_impl(&jobs).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].job.status, JobStatus::Completed);
    assert_eq!(listed[0].destination, request().destination);
    assert_eq!(listed[0].title, "Research");
    let result = get_journal_synthesis_result_impl(&started.job.id, &jobs)
        .await
        .unwrap();
    assert!(!result.synthesis.is_empty());

    assert!(mark_journal_synthesis_applied_impl(&started.job.id, &jobs)
        .await
        .unwrap());
    assert!(
        !mark_journal_synthesis_applied_impl(&started.job.id, &jobs)
            .await
            .unwrap(),
        "a result is saved once"
    );
    assert!(list_journal_syntheses_impl(&jobs).await.unwrap().is_empty());
    jobs.close();
    jobs.drain().await;
}

#[tokio::test]
async fn a_synthesis_still_running_cannot_be_marked_applied() {
    let pool = pool().await;
    let work = FakeWork::answering(0);
    let jobs = runtime_with(&pool, work.clone()).await;
    let started = synthesize_journal_entries_impl(request(), &jobs)
        .await
        .unwrap();
    tokio::time::timeout(WAIT, work.stalled.notified())
        .await
        .unwrap();

    let refused = mark_journal_synthesis_applied_impl(&started.job.id, &jobs).await;

    assert!(matches!(refused, Err(AppError::InvalidState(_))));
    assert!(matches!(
        get_journal_synthesis_result_impl(&started.job.id, &jobs).await,
        Err(AppError::InvalidState(_))
    ));
    jobs.close();
    jobs.drain().await;
}
