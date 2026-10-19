//! Learning's structured model calls.
//!
//! Every generator — lessons, outlines, assessments, practice, practical
//! activities and study decks — sends its strict-JSON
//! request through the grounded-generation service, which divides the model's
//! window and refuses a request that does not fit. A generator states what it
//! needs (the answer's reservation, its priority and schema); it never does
//! window arithmetic of its own.
//!
//! No learning call has a wall-clock limit. A slow model is not a failed
//! one: the adapter's stall detection ends a call whose server has gone
//! quiet, the answer's output reservation ends a runaway one, and dropping
//! the future (a cancelled job, a closed window) ends one nobody wants.

use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::grounded_generation::{
    self, CallOptions, GroundedGenerationError, GroundedOutput, GroundedRequest, Streaming,
};
use lattice_core::shared::error::{AppError, Result};

/// Whether authoring calls — outline and lesson drafts and the reviews that
/// judge them — work through the task in prose before the schema-constrained
/// answer. Asking for the format in the same call that does the reasoning
/// costs small open models most of their structured-output accuracy, while
/// the grammar itself is neutral at 4B and above, so these calls reason in
/// one request and are held to the schema in a second.
pub(in crate::features::learning) const REASON_FIRST_AUTHORING: bool = true;

/// The same for short checks that choose among fixed verdicts. Their answer
/// is a label and a sentence, so a prose step would roughly double their
/// latency for little gain.
pub(in crate::features::learning) const REASON_FIRST_CLASSIFIERS: bool = false;

/// A strict-JSON request: the system instructions, a prompt the model reads
/// whole, and the schema its answer must match. `output_tokens` is the
/// answer's reservation; the planner clamps it to what the model can give.
pub(in crate::features::learning) fn structured(
    system: &str,
    prompt: String,
    schema: serde_json::Value,
    output_tokens: usize,
    reasoning_effort: &str,
) -> GroundedRequest {
    let mut request = GroundedRequest::new(system, prompt);
    request.output_tokens = output_tokens;
    request.call = CallOptions {
        json_schema: Some(schema),
        reasoning_effort: Some(reasoning_effort.to_owned()),
        ..Default::default()
    };
    request
}

/// A free-text request with the same call options as [`structured`] and no
/// schema: the reasoning step of a reason-first call.
pub(in crate::features::learning) fn prose(
    system: &str,
    prompt: String,
    output_tokens: usize,
    reasoning_effort: &str,
) -> GroundedRequest {
    let mut request = GroundedRequest::new(system, prompt);
    request.output_tokens = output_tokens;
    request.call = CallOptions {
        reasoning_effort: Some(reasoning_effort.to_owned()),
        ..Default::default()
    };
    request
}

/// What a request the model's window cannot hold fails with.
pub(in crate::features::learning) enum TooLarge<'a> {
    /// Input the learner can shorten: this message, as invalid input.
    Input(&'a str),
    /// Material the learner cannot trim: [`window_too_small`], saying what
    /// does not fit, with the Learning model, its window and the fix.
    Window(&'a str),
}

impl<'a> From<&'a str> for TooLarge<'a> {
    fn from(message: &'a str) -> Self {
        Self::Input(message)
    }
}

/// Send `request` with no elapsed-time limit. A request the model's window
/// cannot hold fails as `too_large` says and is never sent.
pub(in crate::features::learning) async fn send<'a>(
    llm: &dyn LLMPort,
    mut request: GroundedRequest,
    streaming: Option<&Streaming<'_>>,
    too_large: impl Into<TooLarge<'a>>,
) -> Result<GroundedOutput> {
    request.call.time_budget = None;
    request.call.no_time_limit = true;
    let result = match streaming {
        Some(streaming) => grounded_generation::generate_streaming(llm, request, streaming).await,
        None => grounded_generation::generate(llm, request).await,
    };
    result.map_err(|error| match error {
        GroundedGenerationError::Budget(_) => match too_large.into() {
            TooLarge::Input(message) => AppError::InvalidInput(message.into()),
            TooLarge::Window(what) => window_too_small(llm, what),
        },
        GroundedGenerationError::Model(error) => error,
    })
}

/// The refusal for material that does not fit the Learning model's window:
/// `what` says what does not fit, and the message names the model, its
/// window and the fix. A configuration error, because the window is a
/// setting, not something the learner typed.
pub(in crate::features::learning) fn window_too_small(llm: &dyn LLMPort, what: &str) -> AppError {
    AppError::InvalidConfig(format!(
        "{what} in {}'s {}-token context window. Raise your Learning model's context length in Settings → Models, or choose a Learning model with a larger window",
        llm.model_name(),
        llm.max_context_tokens()
    ))
}

/// Whether the model's window holds `request` as it stands.
pub(in crate::features::learning) fn fits(llm: &dyn LLMPort, request: &GroundedRequest) -> bool {
    grounded_generation::evidence_room(llm, request).is_ok()
}

/// The room the planner leaves for source material once `request` — the
/// prompt without its sources — and the answer's reservation are paid for,
/// at most `ceiling`. Zero when the request does not fit at all.
pub(in crate::features::learning) fn source_room(
    llm: &dyn LLMPort,
    request: &GroundedRequest,
    ceiling: usize,
) -> usize {
    grounded_generation::evidence_room(llm, request).map_or(0, |room| room.min(ceiling))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
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

    #[test]
    fn source_room_is_what_the_planner_leaves_under_the_ceiling() {
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
        assert!(!fits(&llm, &overflowing));
    }
}
