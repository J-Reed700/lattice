use super::*;
use crate::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse, InferencePriority,
};
use crate::application::ports::LLMPort;
use async_trait::async_trait;
use parking_lot::Mutex as SyncMutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

fn ask(priority: InferencePriority, tokens: usize) -> AdmissionRequest {
    AdmissionRequest {
        priority,
        tokens,
        cache_key: None,
    }
}

fn keyed(key: &str) -> AdmissionRequest {
    AdmissionRequest {
        priority: InferencePriority::Interactive,
        tokens: 10,
        cache_key: Some(key.to_string()),
    }
}

/// Let spawned waiters reach the queue.
async fn settle() {
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
}

/// Queue `requests` in order behind a held slot, release it, and return the
/// order they were admitted in.
async fn admission_order(
    scheduler: &Arc<InferenceScheduler>,
    requests: Vec<(&'static str, AdmissionRequest)>,
) -> Vec<&'static str> {
    let blocker = scheduler
        .admit(ask(InferencePriority::Interactive, 1), None)
        .await
        .unwrap();
    let order = Arc::new(SyncMutex::new(Vec::new()));
    let mut waiters = Vec::new();
    for (name, request) in requests {
        let scheduler = Arc::clone(scheduler);
        let order = Arc::clone(&order);
        waiters.push(tokio::spawn(async move {
            let permit = scheduler.admit(request, None).await.unwrap();
            order.lock().push(name);
            drop(permit);
        }));
        settle().await;
    }
    drop(blocker);
    for waiter in waiters {
        waiter.await.unwrap();
    }
    let order = order.lock().clone();
    order
}

#[tokio::test]
async fn higher_priorities_are_admitted_first_and_equal_ones_in_arrival_order() {
    let scheduler = Arc::new(InferenceScheduler::new(
        "test",
        BackendCapacity::concurrent(1),
    ));
    let order = admission_order(
        &scheduler,
        vec![
            ("lesson", ask(InferencePriority::Background, 1)),
            ("summary", ask(InferencePriority::Maintenance, 1)),
            ("judge-1", ask(InferencePriority::Verification, 1)),
            ("chat", ask(InferencePriority::Interactive, 1)),
            ("judge-2", ask(InferencePriority::Verification, 1)),
        ],
    )
    .await;
    assert_eq!(order, ["chat", "judge-1", "judge-2", "summary", "lesson"]);
}

#[tokio::test]
async fn background_work_never_takes_the_slot_kept_for_an_interactive_turn() {
    let scheduler = Arc::new(InferenceScheduler::new(
        "test",
        BackendCapacity::concurrent(3),
    ));
    let first = scheduler
        .admit(ask(InferencePriority::Maintenance, 1), None)
        .await
        .unwrap();
    let second = scheduler
        .admit(ask(InferencePriority::Background, 1), None)
        .await
        .unwrap();

    // One slot is free, and it is the interactive one.
    let third = {
        let scheduler = Arc::clone(&scheduler);
        tokio::spawn(async move {
            scheduler
                .admit(ask(InferencePriority::Verification, 1), None)
                .await
                .map(|_| ())
        })
    };
    settle().await;
    assert!(
        !third.is_finished(),
        "verification must wait for a non-reserved slot"
    );
    assert_eq!(scheduler.queued(), 1);

    let turn = tokio::time::timeout(
        Duration::from_secs(1),
        scheduler.admit(ask(InferencePriority::Interactive, 1), None),
    )
    .await
    .expect("the reserved slot admits an interactive turn at once")
    .unwrap();

    // Two free slots again: one for the queued check, one still reserved.
    drop(turn);
    drop(first);
    tokio::time::timeout(Duration::from_secs(1), third)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    drop(second);
}

#[tokio::test]
async fn a_single_slot_backend_reserves_nothing() {
    let scheduler = Arc::new(InferenceScheduler::new(
        "test",
        BackendCapacity::llama_server(1, 8_192),
    ));
    tokio::time::timeout(
        Duration::from_secs(1),
        scheduler.admit(ask(InferencePriority::Background, 100), None),
    )
    .await
    .expect("one slot is never held back from background work")
    .unwrap();
}

#[tokio::test]
async fn requests_share_the_window_and_one_alone_is_always_admitted() {
    let scheduler = Arc::new(InferenceScheduler::new(
        "test",
        BackendCapacity::llama_server(4, 1_000),
    ));
    let large = scheduler
        .admit(ask(InferencePriority::Interactive, 700), None)
        .await
        .unwrap();
    let small = scheduler
        .admit(ask(InferencePriority::Interactive, 300), None)
        .await
        .unwrap();

    // Slots are free, but the window is not.
    let over = {
        let scheduler = Arc::clone(&scheduler);
        tokio::spawn(async move {
            scheduler
                .admit(ask(InferencePriority::Interactive, 1), None)
                .await
                .map(|_| ())
        })
    };
    settle().await;
    assert!(!over.is_finished(), "a full window admits nothing more");

    drop(small);
    tokio::time::timeout(Duration::from_secs(1), over)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    drop(large);

    // Larger than the whole window, with nothing else in flight: it runs and
    // the server decides, rather than waiting here forever.
    tokio::time::timeout(
        Duration::from_secs(1),
        scheduler.admit(ask(InferencePriority::Background, 50_000), None),
    )
    .await
    .expect("a request alone is always admitted")
    .unwrap();
}

#[tokio::test]
async fn a_request_cancelled_while_queued_leaves_the_queue_with_an_error() {
    let scheduler = Arc::new(InferenceScheduler::new(
        "test",
        BackendCapacity::concurrent(1),
    ));
    let held = scheduler
        .admit(ask(InferencePriority::Interactive, 1), None)
        .await
        .unwrap();
    let cancel = CancellationToken::new();
    let waiting = {
        let scheduler = Arc::clone(&scheduler);
        let cancel = cancel.clone();
        tokio::spawn(async move {
            scheduler
                .admit(ask(InferencePriority::Interactive, 1), Some(&cancel))
                .await
                .map(|_| ())
        })
    };
    settle().await;
    assert_eq!(scheduler.queued(), 1);

    cancel.cancel();
    let result = tokio::time::timeout(Duration::from_secs(1), waiting)
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(result, Err(AppError::InvalidState(_))),
        "{result:?}"
    );
    assert_eq!(
        scheduler.queued(),
        0,
        "the cancelled request left the queue"
    );

    // Its place did not leak: the next request takes the slot when it frees.
    drop(held);
    tokio::time::timeout(
        Duration::from_secs(1),
        scheduler.admit(ask(InferencePriority::Background, 1), None),
    )
    .await
    .unwrap()
    .unwrap();
}

#[tokio::test]
async fn a_dropped_waiter_gives_back_its_place() {
    let scheduler = Arc::new(InferenceScheduler::new(
        "test",
        BackendCapacity::concurrent(1),
    ));
    let held = scheduler
        .admit(ask(InferencePriority::Interactive, 1), None)
        .await
        .unwrap();
    let timed_out = tokio::time::timeout(
        Duration::from_millis(20),
        scheduler.admit(ask(InferencePriority::Interactive, 1), None),
    )
    .await;
    assert!(timed_out.is_err());
    assert_eq!(scheduler.queued(), 0);
    drop(held);
    tokio::time::timeout(
        Duration::from_secs(1),
        scheduler.admit(ask(InferencePriority::Background, 1), None),
    )
    .await
    .unwrap()
    .unwrap();
}

#[tokio::test]
async fn a_conversation_returns_to_the_slot_that_holds_its_prefix() {
    let scheduler = Arc::new(InferenceScheduler::new(
        "test",
        BackendCapacity::llama_server(4, 100_000),
    ));
    let first = scheduler
        .admit(keyed("conversation-a"), None)
        .await
        .unwrap();
    let other = scheduler
        .admit(keyed("conversation-b"), None)
        .await
        .unwrap();
    let a_slot = first.slot().unwrap();
    let b_slot = other.slot().unwrap();
    assert_ne!(a_slot, b_slot);
    drop(first);
    drop(other);

    // An unkeyed request does not evict either conversation's slot while a
    // slot nobody owns is free.
    let unkeyed = scheduler
        .admit(ask(InferencePriority::Interactive, 10), None)
        .await
        .unwrap();
    assert!(![a_slot, b_slot].contains(&unkeyed.slot().unwrap()));
    drop(unkeyed);

    let again = scheduler
        .admit(keyed("conversation-a"), None)
        .await
        .unwrap();
    assert_eq!(again.slot(), Some(a_slot));
    let b_again = scheduler
        .admit(keyed("conversation-b"), None)
        .await
        .unwrap();
    assert_eq!(b_again.slot(), Some(b_slot));
}

#[tokio::test]
async fn a_busy_affine_slot_falls_back_to_the_lowest_free_one() {
    let scheduler = Arc::new(InferenceScheduler::new(
        "test",
        BackendCapacity::llama_server(3, 100_000),
    ));
    let running = scheduler
        .admit(keyed("conversation-a"), None)
        .await
        .unwrap();
    assert_eq!(running.slot(), Some(0));
    let second = scheduler
        .admit(keyed("conversation-a"), None)
        .await
        .unwrap();
    assert_eq!(second.slot(), Some(1));
}

#[tokio::test]
async fn backends_without_slots_send_no_slot() {
    let scheduler = Arc::new(InferenceScheduler::new(
        "test",
        BackendCapacity::concurrent(2),
    ));
    let permit = scheduler
        .admit(keyed("conversation-a"), None)
        .await
        .unwrap();
    assert_eq!(permit.slot(), None);
}

/// Ollama's former semaphore admitted `RECALL_OLLAMA_MAX_CONCURRENCY`
/// requests at once. The scheduler is that bound now.
#[tokio::test]
async fn an_ollama_backend_runs_no_more_than_its_configured_concurrency() {
    let scheduler = registry::ollama_scheduler("http://ollama.test:11434", 2);
    let first = scheduler
        .admit(ask(InferencePriority::Interactive, 1), None)
        .await
        .unwrap();
    let second = scheduler
        .admit(ask(InferencePriority::Interactive, 1), None)
        .await
        .unwrap();
    let third = tokio::time::timeout(
        Duration::from_millis(50),
        scheduler.admit(ask(InferencePriority::Interactive, 1), None),
    )
    .await;
    assert!(third.is_err(), "a third request waits for a slot");
    drop((first, second));

    let same = registry::ollama_scheduler("http://ollama.test:11434", 2);
    assert!(
        Arc::ptr_eq(&scheduler, &same),
        "every role on one endpoint queues in one place"
    );
}

#[test]
fn the_estimate_learns_the_backends_ratio_from_reported_usage() {
    let estimator = TokenEstimator::default();
    assert_eq!(estimator.chars_per_token(), 4.0);

    // Dense text: 3 000 characters the server read as 1 000 tokens.
    for _ in 0..30 {
        estimator.observe(3_000, 1_000);
    }
    let ratio = estimator.chars_per_token();
    assert!((ratio - 3.0).abs() < 0.05, "{ratio}");

    // Nonsense is clamped, and tiny prompts (mostly template) are ignored.
    estimator.observe(100_000, 100);
    assert!(estimator.chars_per_token() <= 6.0);
    let before = estimator.chars_per_token();
    estimator.observe(10, 5);
    assert_eq!(estimator.chars_per_token(), before);
}

#[test]
fn props_give_slots_and_the_shared_window() {
    let props = serde_json::json!({
        "total_slots": 4,
        "default_generation_settings": {"n_ctx": 16384}
    });
    assert_eq!(registry::parse_props(&props), Some((4, Some(16_384))));
    assert_eq!(
        registry::parse_props(&serde_json::json!({"total_slots": 1})),
        Some((1, None))
    );
    assert_eq!(
        registry::parse_props(&serde_json::json!({"model": "x"})),
        None
    );
}

#[test]
fn a_tokenize_reply_counts_ids_or_pieces() {
    assert_eq!(
        tokenize::parse_token_count(&serde_json::json!({"tokens": [1, 2, 3]})).unwrap(),
        3
    );
    assert_eq!(
        tokenize::parse_token_count(
            &serde_json::json!({"tokens": [{"id": 1, "piece": "a"}, {"id": 2, "piece": "b"}]})
        )
        .unwrap(),
        2
    );
    assert!(tokenize::parse_token_count(&serde_json::json!({"error": "x"})).is_err());
}

#[tokio::test]
async fn exact_counts_come_from_the_server_once_per_text() {
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tokenize"))
        .and(body_partial_json(
            serde_json::json!({"content": "hello world", "add_special": false}),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"tokens": [9, 8]})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let tokenizer = ServerTokenizer::new(reqwest::Client::new(), &server.uri());
    assert_eq!(tokenizer.count("hello world").await.unwrap(), 2);
    assert_eq!(
        tokenizer.count("hello world").await.unwrap(),
        2,
        "the second count is served from the cache"
    );
    assert_eq!(tokenizer.count("").await.unwrap(), 0);
}

/// A model that records what reached it and waits until released.
struct GatedModel {
    started: AtomicUsize,
    gate: tokio::sync::Semaphore,
    seen: SyncMutex<Vec<CompletionRequest>>,
}

impl GatedModel {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            started: AtomicUsize::new(0),
            gate: tokio::sync::Semaphore::new(0),
            seen: SyncMutex::new(Vec::new()),
        })
    }
}

#[async_trait]
impl LLMPort for GatedModel {
    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> crate::shared::error::Result<CompletionResponse> {
        self.started.fetch_add(1, Ordering::SeqCst);
        self.seen.lock().push(request.clone());
        let _released = self.gate.acquire().await.unwrap();
        Ok(CompletionResponse {
            text: "ok".into(),
            input_tokens: 1_000,
            finish_reason: "stop".into(),
            ..Default::default()
        })
    }
    fn model_name(&self) -> &str {
        "gated"
    }
    fn max_context_tokens(&self) -> usize {
        8_192
    }
    async fn is_ready(&self) -> crate::shared::error::Result<bool> {
        Ok(true)
    }
}

fn request(text: &str, cancel: Option<CancellationToken>) -> CompletionRequest {
    CompletionRequest {
        input: vec![CompletionInput::Message {
            role: "user".into(),
            content: text.into(),
        }],
        priority: InferencePriority::Interactive,
        cancel,
        cache_key: Some("conversation".into()),
        ..Default::default()
    }
}

#[tokio::test]
async fn a_request_cancelled_in_flight_is_aborted_and_frees_its_slot() {
    let model = GatedModel::new();
    let scheduler = Arc::new(InferenceScheduler::new(
        "test",
        BackendCapacity::llama_server(1, 100_000),
    ));
    let llm = Arc::new(ScheduledLlm::new(
        model.clone(),
        Arc::clone(&scheduler),
        512,
    ));
    let cancel = CancellationToken::new();
    let running = {
        let llm = Arc::clone(&llm);
        let cancel = cancel.clone();
        tokio::spawn(async move { llm.complete(&request("first", Some(cancel))).await })
    };
    while model.started.load(Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
    }
    cancel.cancel();
    let result = tokio::time::timeout(Duration::from_secs(1), running)
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(result, Err(AppError::InvalidState(_))),
        "{result:?}"
    );

    // The aborted call gave its only slot back.
    model.gate.add_permits(1);
    tokio::time::timeout(Duration::from_secs(1), llm.complete(&request("next", None)))
        .await
        .expect("the slot was released")
        .unwrap();
}

#[tokio::test]
async fn an_admitted_request_reaches_the_backend_pinned_to_its_slot() {
    let model = GatedModel::new();
    model.gate.add_permits(10);
    let scheduler = Arc::new(InferenceScheduler::new(
        "test",
        BackendCapacity::llama_server(2, 100_000),
    ));
    let llm = ScheduledLlm::new(model.clone(), scheduler, 512);
    llm.complete(&request(&"x".repeat(5_000), None))
        .await
        .unwrap();
    let seen = model.seen.lock().clone();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].assigned_slot, Some(0));
    assert_eq!(seen[0].cache_key.as_deref(), Some("conversation"));

    // 5 000 characters reported as 1 000 tokens moves the estimate off four.
    assert!(llm.chars_per_token() > 4.0, "{}", llm.chars_per_token());
    assert_eq!(
        llm.count_tokens_exact("abcd").await.unwrap(),
        llm.count_tokens("abcd")
    );
    assert!(!llm.counts_tokens_exactly());
}

#[tokio::test]
async fn a_call_queues_behind_a_busy_slot() {
    let model = GatedModel::new();
    model.gate.add_permits(1);
    let scheduler = Arc::new(InferenceScheduler::new(
        "test",
        BackendCapacity::concurrent(1),
    ));
    let llm = ScheduledLlm::new(model, Arc::clone(&scheduler), 512);
    let held = scheduler
        .admit(ask(InferencePriority::Interactive, 1), None)
        .await
        .unwrap();
    let waiting = tokio::time::timeout(
        Duration::from_millis(50),
        llm.complete(&request("hi", None)),
    )
    .await;
    assert!(waiting.is_err(), "the call waits for the slot");
    drop(held);
    assert_eq!(llm.complete(&request("hi", None)).await.unwrap().text, "ok");
}
