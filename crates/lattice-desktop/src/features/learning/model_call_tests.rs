use super::*;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

struct Recorder {
    window: usize,
    sent: Mutex<Vec<CompletionRequest>>,
}

#[async_trait::async_trait]
impl LLMPort for Recorder {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        if let Ok(mut sent) = self.sent.lock() {
            sent.push(request.clone());
        }
        Ok(CompletionResponse::from_text("{}"))
    }
    fn model_name(&self) -> &str {
        "recorder"
    }
    fn max_context_tokens(&self) -> usize {
        self.window
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

fn recorder(window: usize) -> Recorder {
    Recorder {
        window,
        sent: Mutex::new(Vec::new()),
    }
}

#[tokio::test]
async fn a_structured_call_sends_the_prompt_whole_with_its_schema() -> Result<()> {
    let llm = recorder(32_000);
    let schema = serde_json::json!({"type": "object"});
    send(
        &llm,
        structured(
            "Return JSON.",
            "{\"task\":1}".into(),
            schema.clone(),
            2000,
            "low",
        ),
        None,
        "too large",
    )
    .await?;
    let sent = llm
        .sent
        .lock()
        .map_err(|_| AppError::InternalError("lock".into()))?;
    let request = sent
        .first()
        .ok_or_else(|| AppError::InternalError("unsent".into()))?;
    assert_eq!(request.user_text(), "{\"task\":1}");
    assert_eq!(request.json_schema, Some(schema));
    assert_eq!(request.reasoning_effort.as_deref(), Some("low"));
    assert_eq!(request.max_output_tokens, Some(2000));
    // No learning call is killed by a clock.
    assert!(request.no_time_limit);
    assert_eq!(request.wall_clock_budget(), None);
    Ok(())
}

#[tokio::test]
async fn a_prompt_the_window_cannot_hold_is_refused_in_the_callers_words() {
    let llm = recorder(4096);
    let error = send(
        &llm,
        structured(
            "Return JSON.",
            "x".repeat(40_000),
            serde_json::json!({}),
            500,
            "low",
        ),
        None,
        "Choose fewer sources.",
    )
    .await
    .err()
    .map(|error| error.to_string())
    .unwrap_or_default();
    assert!(error.contains("Choose fewer sources."), "{error}");
    assert!(llm.sent.lock().map(|sent| sent.is_empty()).unwrap_or(false));
}

#[tokio::test]
async fn material_the_window_cannot_hold_names_the_learning_model_and_the_fix() {
    let llm = recorder(4096);
    let error = send(
        &llm,
        structured(
            "Return JSON.",
            "x".repeat(40_000),
            serde_json::json!({}),
            500,
            "low",
        ),
        None,
        TooLarge::Window("The requested learning material does not fit"),
    )
    .await
    .err();
    let Some(AppError::InvalidConfig(message)) = error else {
        panic!("expected a configuration refusal, got {error:?}");
    };
    assert_eq!(
        message,
        "The requested learning material does not fit in recorder's 4096-token context \
         window. Raise your Learning model's context length in Settings → Models, or choose \
         a Learning model with a larger window"
    );
    assert!(llm.sent.lock().map(|sent| sent.is_empty()).unwrap_or(false));
}

/// A backend whose own window check refuses every request with the
/// numbers, as the scheduler and the llama.cpp client do.
struct Refusing;

const NUMBERED_REFUSAL: &str = "This request's prompt is 8500 tokens (counted exactly), \
     which leaves no room for an answer; the configured context window is 8192. It was not sent.";

#[async_trait::async_trait]
impl LLMPort for Refusing {
    async fn complete(&self, _: &CompletionRequest) -> Result<CompletionResponse> {
        Err(AppError::PromptExceedsWindow {
            prompt_tokens: 8_500,
            window: 8_192,
            counted_exactly: true,
            message: NUMBERED_REFUSAL.into(),
        })
    }
    fn model_name(&self) -> &str {
        "refusing"
    }
    fn max_context_tokens(&self) -> usize {
        32_000
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn the_backends_numbered_refusal_reaches_the_learner_once_and_unchanged() {
    for too_large in [
        TooLarge::Input("Choose fewer sources."),
        TooLarge::Window("The requested learning material does not fit"),
    ] {
        let error = send(
            &Refusing,
            structured(
                "Return JSON.",
                "{}".into(),
                serde_json::json!({}),
                500,
                "low",
            ),
            None,
            too_large,
        )
        .await
        .err();
        let Some(AppError::PromptExceedsWindow { message, .. }) = error else {
            panic!("expected the backend's refusal, got {error:?}");
        };
        assert_eq!(message, NUMBERED_REFUSAL);
    }
}

#[tokio::test]
async fn source_room_is_what_the_planner_leaves_under_the_ceiling() {
    let llm = recorder(32_000);
    let small = structured("Teach.", "{}".into(), serde_json::json!({}), 4000, "low");
    assert_eq!(source_room(&llm, &small, 7000), 7000);
    let crowded = structured(
        "Teach.",
        "x".repeat(100_000),
        serde_json::json!({}),
        4000,
        "low",
    );
    assert!(source_room(&llm, &crowded, 7000) < 7000);
    let overflowing = structured(
        "Teach.",
        "x".repeat(200_000),
        serde_json::json!({}),
        4000,
        "low",
    );
    assert_eq!(source_room(&llm, &overflowing, 7000), 0);
    assert!(!fits(&llm, &overflowing).await);
}

/// What a gate hands a tokenizer for `request`: each message's role and
/// content, joined by newlines.
fn rendered(request: &CompletionRequest) -> String {
    request
        .input
        .iter()
        .flat_map(|item| match item {
            CompletionInput::Message { role, content } => vec![role.clone(), content.clone()],
            _ => Vec::new(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Counts exactly at two characters a token where the estimate assumes four,
/// as on notation-heavy material, and refuses, as the scheduler's gate does,
/// a prompt its own exact count says fills the window.
struct Dense {
    window: usize,
    counts: AtomicUsize,
    sent: Mutex<Vec<CompletionRequest>>,
}

fn dense(window: usize) -> Dense {
    Dense {
        window,
        counts: AtomicUsize::new(0),
        sent: Mutex::new(Vec::new()),
    }
}

#[async_trait::async_trait]
impl LLMPort for Dense {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let prompt_tokens = rendered(request).chars().count() / 2;
        if prompt_tokens >= self.window {
            return Err(AppError::PromptExceedsWindow {
                prompt_tokens,
                window: self.window,
                counted_exactly: true,
                message: "over the window".into(),
            });
        }
        if let Ok(mut sent) = self.sent.lock() {
            sent.push(request.clone());
        }
        Ok(CompletionResponse::from_text("{}"))
    }
    fn model_name(&self) -> &str {
        "dense"
    }
    fn max_context_tokens(&self) -> usize {
        self.window
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.chars().count() / 4
    }
    async fn count_tokens_exact(&self, text: &str) -> Result<usize> {
        self.counts.fetch_add(1, Ordering::SeqCst);
        Ok(text.chars().count() / 2)
    }
    fn counts_tokens_exactly(&self) -> bool {
        true
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// Cannot count, and refuses, as a server does, any prompt longer than a
/// limit it never states, knowing the prompt's size only by estimate.
struct Blind {
    limit_chars: usize,
    refusals: AtomicUsize,
    sent: Mutex<Vec<CompletionRequest>>,
}

fn blind(limit_chars: usize) -> Blind {
    Blind {
        limit_chars,
        refusals: AtomicUsize::new(0),
        sent: Mutex::new(Vec::new()),
    }
}

#[async_trait::async_trait]
impl LLMPort for Blind {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let chars = rendered(request).chars().count();
        if chars > self.limit_chars {
            self.refusals.fetch_add(1, Ordering::SeqCst);
            return Err(AppError::PromptExceedsWindow {
                prompt_tokens: chars / 4,
                window: 8_192,
                counted_exactly: false,
                message: "the server refused it as too large".into(),
            });
        }
        if let Ok(mut sent) = self.sent.lock() {
            sent.push(request.clone());
        }
        Ok(CompletionResponse::from_text("{}"))
    }
    fn model_name(&self) -> &str {
        "blind"
    }
    fn max_context_tokens(&self) -> usize {
        8_192
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.chars().count() / 4
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// A request carrying `room` estimate tokens of material.
fn packed(room: usize) -> GroundedRequest {
    structured(
        "Return JSON.",
        format!("{{\"material\":\"{}\"}}", "x".repeat(room * 4)),
        serde_json::json!({}),
        500,
        "low",
    )
}

fn sent(sent: &Mutex<Vec<CompletionRequest>>) -> usize {
    sent.lock().map(|sent| sent.len()).unwrap_or(usize::MAX)
}

#[tokio::test]
async fn material_the_estimate_fits_and_the_exact_count_does_not_is_refused_by_fits() {
    let llm = dense(8_192);
    let request = packed(source_room(&llm, &packed(0), usize::MAX));
    let estimated = grounded_generation::estimate(&llm, &request);
    assert!(estimated.is_ok_and(|measured| measured.fits()));
    assert!(!fits(&llm, &request).await);
}

#[tokio::test]
async fn dense_material_shrinks_until_its_exact_count_fits() {
    let llm = dense(8_192);
    let first = source_room(&llm, &packed(0), usize::MAX);
    let mut rooms = Vec::new();
    let fit = fit_room(&llm, first, |room| {
        rooms.push(room);
        packed(room)
    })
    .await;

    let measured = fit.measured.expect("the fitted request was measured");
    assert!(measured.exact);
    assert!(measured.prompt_tokens <= measured.allowed);
    assert!(fit.fits());
    assert!(fit.room > 0 && fit.room < first);
    assert_eq!(fit.request.task, packed(fit.room).task);
    // The skeleton first, then strictly smaller rooms from the first.
    assert_eq!(rooms.first(), Some(&0));
    let tried = rooms.get(1..).unwrap_or_default();
    assert_eq!(tried.first(), Some(&first));
    assert!(tried.windows(2).all(|pair| pair.get(1) < pair.first()));
    assert!(llm.counts.load(Ordering::SeqCst) <= 3);
}

#[tokio::test]
async fn a_skeleton_the_window_cannot_hold_settles_at_room_zero_unsent() {
    let llm = dense(4_096);
    let crowded = |room: usize| {
        structured(
            "Return JSON.",
            format!("{}{}", "s".repeat(40_000), "x".repeat(room * 4)),
            serde_json::json!({}),
            500,
            "low",
        )
    };
    let fit = fit_room(&llm, 1_000, crowded).await;
    assert_eq!(fit.room, 0);
    assert!(!fit.fits());
    assert_eq!(fit.request.task, crowded(0).task);
    assert_eq!(llm.counts.load(Ordering::SeqCst), 0);

    let answer = send_fitted(&llm, 1_000, crowded, |request| {
        send(&llm, request, None, "too large")
    })
    .await;
    assert!(matches!(answer, Ok(None)));
    assert_eq!(llm.counts.load(Ordering::SeqCst), 0);
    assert_eq!(sent(&llm.sent), 0);
}

#[tokio::test]
async fn a_fitted_send_on_dense_material_is_never_refused() -> Result<()> {
    let llm = dense(8_192);
    let room = source_room(&llm, &packed(0), usize::MAX);
    let answer = send_fitted(&llm, room, packed, |request| {
        send(&llm, request, None, "too large")
    })
    .await?;
    assert!(answer.is_some());
    assert_eq!(sent(&llm.sent), 1);
    Ok(())
}

#[tokio::test]
async fn a_servers_estimated_refusal_halves_the_room_until_it_is_accepted() -> Result<()> {
    let llm = blind(6_000);
    let room = source_room(&llm, &packed(0), usize::MAX);
    let answer = send_fitted(&llm, room, packed, |request| {
        send(&llm, request, None, "too large")
    })
    .await?;
    assert!(answer.is_some());
    assert_eq!(sent(&llm.sent), 1);
    let refusals = llm.refusals.load(Ordering::SeqCst);
    assert!(refusals > 0);
    // Each refusal halves the room: no more of them than halvings of it.
    assert!(refusals <= usize::BITS as usize - room.leading_zeros() as usize);

    let never = blind(0);
    let answer = send_fitted(&never, room, packed, |request| {
        send(&never, request, None, "too large")
    })
    .await?;
    assert!(answer.is_none());
    assert_eq!(sent(&never.sent), 0);
    Ok(())
}
