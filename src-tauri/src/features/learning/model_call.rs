//! Learning's structured model calls.
//!
//! Every generator — lessons, outlines, assessments, practice, practical
//! activities, study decks and the evidence challenge — sends its strict-JSON
//! request through the grounded-generation service, which divides the model's
//! window and refuses a request that does not fit. A generator states what it
//! needs (the answer's reservation, its priority and schema); it never does
//! window arithmetic of its own.

use std::time::Duration;

use crate::application::ports::LLMPort;
use crate::application::services::grounded_generation::{
    self, CallOptions, GroundedGenerationError, GroundedOutput, GroundedRequest, Streaming,
};
use crate::shared::error::{AppError, Result};

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

/// A wall-clock limit on one call, queueing included, and what the learner is
/// told when it passes.
pub(in crate::features::learning) struct Deadline {
    pub after: Duration,
    pub message: &'static str,
}

/// Send `request`. A request the model's window cannot hold fails with
/// `too_large` and is never sent.
pub(in crate::features::learning) async fn send(
    llm: &dyn LLMPort,
    request: GroundedRequest,
    streaming: Option<&Streaming<'_>>,
    deadline: Option<Deadline>,
    too_large: &str,
) -> Result<GroundedOutput> {
    let call = async {
        match streaming {
            Some(streaming) => {
                grounded_generation::generate_streaming(llm, request, streaming).await
            }
            None => grounded_generation::generate(llm, request).await,
        }
    };
    let result = match deadline {
        Some(deadline) => tokio::time::timeout(deadline.after, call)
            .await
            .map_err(|_| AppError::ServiceNotAvailable(deadline.message.into()))?,
        None => call.await,
    };
    result.map_err(|error| match error {
        GroundedGenerationError::Budget(_) => AppError::InvalidInput(too_large.into()),
        GroundedGenerationError::Model(error) => error,
    })
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
    use crate::application::ports::llm_port::{CompletionRequest, CompletionResponse};
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
