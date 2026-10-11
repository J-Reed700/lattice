//! Research turns as jobs: carried on after a restart, ended by the chat's
//! stop button.
use super::*;
use crate::application::ports::llm_port::{CompletionInput, CompletionResponse};
use crate::domain::conversation::MessageRole;
use crate::features::conversation::chat::routing::SearchFlags;
use crate::features::conversation::chat::test_runtime::{FakeRuntime, Scripted};
use crate::features::conversation::chat::ToolPreferences;
use crate::features::conversation::ConversationServiceTrait;
use crate::shared::runtime::jobs::JobRuntime;
use std::time::Duration;
use tokio::sync::Notify;

const WAIT: Duration = Duration::from_secs(10);
const QUESTION: &str = "How are lithium batteries recycled?";

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

fn silent() -> ChatEventSink {
    Arc::new(|_| Ok(()))
}

async fn until(mut done: impl FnMut() -> bool) {
    tokio::time::timeout(WAIT, async {
        while !done() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("waited too long");
}

async fn finished(jobs: &JobRuntime, id: &str) -> crate::shared::runtime::jobs::JobRecord {
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
    .expect("the job never finished")
}

fn research(conversation_id: &str) -> TurnRequest {
    TurnRequest {
        conversation_id: Some(conversation_id.to_string()),
        message: QUESTION.into(),
        tool_preferences: Some(ToolPreferences {
            deep_research_mode: true,
            ..Default::default()
        }),
        request_id: Some(uuid::Uuid::new_v4().to_string()),
        ..TurnRequest::default()
    }
}

/// What a first attempt saved before the app closed: the question pending on
/// the thread, the planned request, and one finished round of searching.
struct FirstAttempt {
    request: TurnRequest,
    assembled: AssembledTurn,
    round: ResearchRound,
}

async fn first_attempt(runtime: &FakeRuntime) -> FirstAttempt {
    let conversations = runtime.conversations.clone();
    let conversation = conversations
        .create_conversation("Research".into(), "scripted".into(), None)
        .await
        .unwrap();
    let request = research(&conversation.id.to_string());
    let question = conversations
        .add_message_with_metadata(
            &conversation.id.to_string(),
            MessageRole::User,
            QUESTION.into(),
            8,
            "pending".into(),
            None,
        )
        .await
        .unwrap();
    let call = CompletionInput::ToolCall {
        id: "call-1".into(),
        name: "web_search".into(),
        arguments: serde_json::json!({ "query": "lithium battery recycling" }),
    };
    FirstAttempt {
        assembled: AssembledTurn {
            user_message_id: question.id,
            message_tokens: 8,
            history_len: 0,
            flags: SearchFlags::from_preferences(request.tool_preferences.as_ref()),
            router: None,
            input: vec![CompletionInput::Message {
                role: "user".into(),
                content: QUESTION.into(),
            }],
            tools: Vec::new(),
            max_output_tokens: 1_024,
            input_budget: 16_000,
            memory_usage: None,
            short_circuit_response: None,
            sources: Vec::new(),
            retrieval_trace: None,
            pages_read: Default::default(),
            steps: Vec::new(),
            elapsed_ms: 50,
        },
        round: ResearchRound {
            completed: 1,
            transcript: vec![
                call,
                CompletionInput::ToolResult {
                    id: "call-1".into(),
                    output: "Most packs are shredded, then the metals are leached.".into(),
                },
            ],
            queries: vec!["lithium battery recycling".into()],
            fetched: Vec::new(),
            sources: Vec::new(),
            retrieval_trace: None,
            pages_read: Default::default(),
            steps: Vec::new(),
            timings: Default::default(),
            elapsed_ms: 900,
        },
        request,
    }
}

/// Stands in for the turn the app was running: it saves what the turn had
/// done, then waits for the app to close.
struct SavesThenWaits {
    assembled: AssembledTurn,
    round: ResearchRound,
    saved: Arc<Notify>,
}

#[async_trait]
impl JobHandler for SavesThenWaits {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        context
            .put_checkpoint(ASSEMBLED, &encode(&self.assembled)?)
            .await?;
        context.put_checkpoint(ROUND, &encode(&self.round)?).await?;
        self.saved.notify_one();
        context.cancellation().cancelled().await;
        Ok(JobOutcome::Stopped)
    }
}

/// Runs the research job until it has saved a round, then closes the app.
async fn closed_after_one_round(pool: &sqlx::SqlitePool, attempt: &FirstAttempt) -> String {
    let before = JobRuntime::new(pool.clone());
    let saved = Arc::new(Notify::new());
    before
        .register(
            DEEP_RESEARCH,
            Arc::new(SavesThenWaits {
                assembled: attempt.assembled.clone(),
                round: attempt.round.clone(),
                saved: saved.clone(),
            }),
            job_config(),
        )
        .await
        .unwrap();
    let job = before
        .submit(&research_job(&attempt.request).unwrap())
        .await
        .unwrap();
    tokio::time::timeout(WAIT, saved.notified()).await.unwrap();
    before.close();
    before.drain().await;
    let job = before.store().get(&job.id).await.unwrap();
    assert_eq!(
        job.status,
        JobStatus::Pending,
        "a closing app keeps the job"
    );
    job.id
}

#[tokio::test]
async fn a_research_turn_stopped_by_a_restart_resumes_from_its_last_round() {
    let pool = pool().await;
    let llm = Scripted::new([CompletionResponse::from_text(
        "Shredding, then leaching recovers most of the lithium.",
    )]);
    let jobs = JobRuntime::with_poll_interval(pool.clone(), Duration::from_millis(50));
    let runtime = FakeRuntime {
        llm: Some(llm.clone()),
        jobs: Some(jobs.clone()),
        ..FakeRuntime::default()
    };
    let attempt = first_attempt(&runtime).await;
    let job_id = closed_after_one_round(&pool, &attempt).await;

    // The app opens again.
    jobs.register(
        DEEP_RESEARCH,
        Arc::new(DeepResearchJob::new(runtime.share(), silent())),
        job_config(),
    )
    .await
    .unwrap();
    let job = finished(&jobs, &job_id).await;

    assert_eq!(job.status, JobStatus::Completed, "{:?}", job.error_message);
    let requests = llm.requests();
    assert_eq!(requests.len(), 1, "the saved round is not searched again");
    assert_eq!(
        requests[0].input.len(),
        attempt.assembled.input.len() + attempt.round.transcript.len()
    );
    assert!(matches!(
        requests[0].input.last(),
        Some(CompletionInput::ToolResult { output, .. }) if output.contains("leached")
    ));
    let conversation = runtime
        .conversations
        .get_conversation(attempt.request.conversation_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    let answer = conversation
        .messages()
        .iter()
        .find(|message| message.role == MessageRole::Assistant)
        .expect("the answer is on the thread")
        .clone();
    assert_eq!(job.result_ref.as_deref(), Some(answer.id.as_str()));
    assert!(conversation
        .messages()
        .iter()
        .all(|message| message.status != "pending"));
    jobs.close();
    jobs.drain().await;
}

#[tokio::test]
async fn the_chat_stop_cancels_a_running_research_job() {
    let pool = pool().await;
    // Never answers: the turn runs until it is stopped.
    let llm = Scripted::new([]);
    let jobs = JobRuntime::with_poll_interval(pool.clone(), Duration::from_millis(50));
    let runtime = FakeRuntime {
        llm: Some(llm.clone()),
        jobs: Some(jobs.clone()),
        ..FakeRuntime::default()
    };
    let attempt = first_attempt(&runtime).await;
    let job_id = closed_after_one_round(&pool, &attempt).await;
    jobs.register(
        DEEP_RESEARCH,
        Arc::new(DeepResearchJob::new(runtime.share(), silent())),
        job_config(),
    )
    .await
    .unwrap();
    until(|| !llm.requests().is_empty()).await;

    let stopped = turn::run_turn(
        &runtime,
        attempt.request.conversation_id.clone(),
        String::new(),
        None,
        Some(true),
        attempt.request.request_id.clone(),
        None,
        None,
        silent(),
    )
    .await
    .unwrap();

    assert_eq!(stopped.message, "cancelled");
    let job = finished(&jobs, &job_id).await;
    assert_eq!(
        job.status,
        JobStatus::Cancelled,
        "a stopped turn does not resume"
    );
    let conversation = runtime
        .conversations
        .get_conversation(attempt.request.conversation_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    let question = conversation
        .messages()
        .iter()
        .find(|message| message.id == attempt.assembled.user_message_id)
        .unwrap()
        .clone();
    assert_eq!(question.status, "failed", "the question is left retryable");
    jobs.close();
    jobs.drain().await;
}

/// Holds its conversation until the app closes.
struct Holds;

#[async_trait]
impl JobHandler for Holds {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        context.cancellation().cancelled().await;
        Ok(JobOutcome::Stopped)
    }
}

#[tokio::test]
async fn stopping_a_research_turn_still_queued_answers_its_command() {
    let pool = pool().await;
    let jobs = JobRuntime::with_poll_interval(pool, Duration::from_millis(50));
    jobs.register(DEEP_RESEARCH, Arc::new(Holds), job_config())
        .await
        .unwrap();
    let runtime = FakeRuntime {
        jobs: Some(jobs.clone()),
        ..FakeRuntime::default()
    };
    let earlier = research("queued-conversation");
    jobs.submit(&research_job(&earlier).unwrap()).await.unwrap();
    let later = research("queued-conversation");
    let later_id = later.request_id.clone();
    let command = tokio::spawn({
        let runtime = runtime.clone();
        async move { run(&runtime, later, silent()).await }
    });
    tokio::time::timeout(WAIT, async {
        while jobs
            .store()
            .list_for_subject(DEEP_RESEARCH, "queued-conversation")
            .await
            .unwrap()
            .len()
            < 2
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();

    assert!(stop(&runtime, "queued-conversation", later_id.as_deref())
        .await
        .unwrap());
    let answered = tokio::time::timeout(WAIT, command).await.unwrap().unwrap();

    assert!(cancellation::is_cancellation(
        &answered.expect_err("a stopped turn has no answer")
    ));
    jobs.close();
    jobs.drain().await;
}
