use super::*;
use async_trait::async_trait;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse, InferencePriority,
};
use lattice_core::application::ports::LLMPort;
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
async fn unkeyed_utility_calls_reuse_one_slot_and_preserve_conversation_prefixes() {
    let scheduler = Arc::new(InferenceScheduler::new(
        "test",
        BackendCapacity::llama_server(4, 100_000),
    ));

    // Turn 1: Intent classifier (unkeyed) runs before the chat turn.
    let intent_1 = scheduler
        .admit(ask(InferencePriority::Interactive, 10), None)
        .await
        .unwrap();
    let utility_slot = intent_1.slot().unwrap();
    drop(intent_1);

    // Turn 1: Chat turn picks a fresh slot rather than overwriting utility_slot
    // or colliding with subsequent utility calls.
    let chat_1 = scheduler
        .admit(keyed("conversation-a"), None)
        .await
        .unwrap();
    let chat_slot = chat_1.slot().unwrap();
    assert_ne!(chat_slot, utility_slot);
    drop(chat_1);

    // Turn 2: Intent classifier + HyDE (both unkeyed) reuse utility_slot,
    // leaving chat_slot's KV prefix intact.
    let intent_2 = scheduler
        .admit(ask(InferencePriority::Interactive, 10), None)
        .await
        .unwrap();
    assert_eq!(intent_2.slot(), Some(utility_slot));
    drop(intent_2);

    let hyde_2 = scheduler
        .admit(ask(InferencePriority::Interactive, 10), None)
        .await
        .unwrap();
    assert_eq!(hyde_2.slot(), Some(utility_slot));
    drop(hyde_2);

    // Turn 2: Chat turn returns to its preserved slot.
    let chat_2 = scheduler
        .admit(keyed("conversation-a"), None)
        .await
        .unwrap();
    assert_eq!(chat_2.slot(), Some(chat_slot));
}

#[tokio::test]
async fn full_slots_evict_the_least_recently_used_conversation() {
    let scheduler = Arc::new(InferenceScheduler::new(
        "test",
        BackendCapacity::llama_server(2, 100_000),
    ));

    let a = scheduler.admit(keyed("conv-a"), None).await.unwrap();
    let a_slot = a.slot().unwrap();
    drop(a);

    let b = scheduler.admit(keyed("conv-b"), None).await.unwrap();
    let b_slot = b.slot().unwrap();
    drop(b);

    // Touch conv-a again so conv-b becomes the least-recently-used slot.
    let a_again = scheduler.admit(keyed("conv-a"), None).await.unwrap();
    assert_eq!(a_again.slot(), Some(a_slot));
    drop(a_again);

    // A third conversation must evict conv-b (LRU), not conv-a (slot 0).
    let c = scheduler.admit(keyed("conv-c"), None).await.unwrap();
    assert_eq!(c.slot(), Some(b_slot));
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
    let schedulers = registry::SchedulerRegistry::default();
    let scheduler = schedulers.ollama("http://ollama.test:11434", 2);
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

    let same = schedulers.ollama("http://ollama.test:11434", 2);
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
    ) -> lattice_core::shared::error::Result<CompletionResponse> {
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
    async fn is_ready(&self) -> lattice_core::shared::error::Result<bool> {
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

fn scheduled(model: Arc<GatedModel>) -> ScheduledLlm {
    let scheduler = Arc::new(InferenceScheduler::new(
        "test",
        BackendCapacity::concurrent(1),
    ));
    ScheduledLlm::new(model, scheduler, 512)
}

/// Whatever backend is behind the port, nothing whose prompt alone fills its
/// configured window is queued or sent.
#[tokio::test]
async fn a_request_the_configured_window_cannot_hold_never_reaches_the_backend() {
    let model = GatedModel::new();
    model.gate.add_permits(1);
    let llm = scheduled(model.clone());
    // About 10 000 tokens by the estimate, in the model's 8 192-token window.
    let full = llm
        .complete(&request(&"x".repeat(40_000), None))
        .await
        .unwrap_err();
    assert!(matches!(full, AppError::InvalidInput(_)), "{full}");
    let told = full.to_string();
    for expected in [
        "(estimated)",
        "leaves no room for an answer",
        "configured context window is 8192",
    ] {
        assert!(told.contains(expected), "{told}");
    }
    assert!(model.seen.lock().is_empty(), "nothing reached the backend");
}

/// The lesson check seen in production: a caller sized its answer to the
/// room its own count of the prompt left, the gate counts the prompt a few
/// tokens higher, and the request is sent with its answer cut to the room
/// left instead of being refused.
#[tokio::test]
async fn an_answer_larger_than_the_room_left_is_cut_to_fit_not_refused() {
    let model = GatedModel::new();
    model.gate.add_permits(2);
    let llm = scheduled(model.clone());
    // "user" and 31 564 characters: 7 892 tokens by the estimate, 300 left.
    let prompt = "x".repeat(31_564);
    llm.complete(&CompletionRequest {
        max_output_tokens: Some(302),
        ..request(&prompt, None)
    })
    .await
    .unwrap();
    // An answer the room holds is sent as asked.
    llm.complete(&CompletionRequest {
        max_output_tokens: Some(256),
        ..request(&prompt, None)
    })
    .await
    .unwrap();
    let seen = model.seen.lock();
    assert_eq!(seen[0].max_output_tokens, Some(300));
    assert_eq!(seen[1].max_output_tokens, Some(256));
}

/// The estimate never refuses what the backend's own tokenizer says fits.
#[tokio::test]
async fn an_exact_count_decides_before_a_request_is_refused() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tokenize"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "tokens": vec![1; 3_000] })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let model = GatedModel::new();
    model.gate.add_permits(1);
    let llm = scheduled(model.clone())
        .with_tokenizer(ServerTokenizer::new(reqwest::Client::new(), &server.uri()));
    // About 10 000 tokens by the estimate; the server counts 3 000.
    llm.complete(&request(&"x".repeat(40_000), None))
        .await
        .unwrap();
    assert_eq!(model.seen.lock().len(), 1);
}
